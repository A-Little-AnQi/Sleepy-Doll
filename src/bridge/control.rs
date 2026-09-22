//! 本地桥的生命周期：注入器只由桌面 IPC 调用，不暴露为 agent 工具。
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{Value, json};

use crate::{
    config::BridgeConfig,
    error::{Error, Result},
};

fn exe_dir() -> Result<PathBuf> {
    Ok(std::env::current_exe()?.parent().unwrap().to_path_buf())
}

pub fn directory() -> Result<PathBuf> {
    let beside_exe = exe_dir()?;
    // 交付布局：组件在 `bridge` 子目录；扁平包直接放在可执行文件旁。
    for candidate in [beside_exe.join("bridge"), beside_exe.clone()] {
        if candidate.join("BgiBridge.Injector.exe").is_file() {
            return Ok(candidate);
        }
    }
    // 开发布局：二进制在 target 下运行，桥在 target/bridge。
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let development = manifest.join("target").join("bridge");
    if beside_exe.starts_with(manifest.join("target"))
        && development.join("BgiBridge.Injector.exe").is_file()
    {
        return Ok(development);
    }
    Err(Error::Config(
        "无法连接 BetterGI：安装不完整。请重新安装 Sleepy Doll。".into(),
    ))
}

/// 唯一的数据根 `<install>/user`。
///
/// 其它布局（扁平包、开发布局）沿用桥自己解析的数据根，`None` 表示不覆盖。
fn data_root(install: &Path, bridge_dir: &Path) -> Option<PathBuf> {
    if bridge_dir == install.join("bridge").as_path() {
        Some(install.join("user"))
    } else {
        None
    }
}

fn host_running() -> bool {
    let mut command = if cfg!(windows) {
        std::process::Command::new("tasklist")
    } else {
        std::process::Command::new("tasklist.exe")
    };
    command
        .args(["/FI", "IMAGENAME eq BetterGI.exe", "/NH"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.output().ok().is_some_and(|output| {
        String::from_utf8_lossy(&output.stdout)
            .to_ascii_lowercase()
            .contains("bettergi.exe")
    })
}

/// 宿主是否在运行。给桌面壳的监视循环用。
pub fn is_host_running() -> bool {
    host_running()
}

/// 连接前保证宿主在运行：没运行就找到安装位置启动它，并等进程出现。
fn ensure_host_running(config: &BridgeConfig) -> Result<()> {
    if host_running() {
        return Ok(());
    }
    let executable = locate_host(config).ok_or_else(|| {
        Error::Tool(
            "没有找到 BetterGI 的安装位置。请手动启动一次 BetterGI，连接成功后会记住它的位置。"
                .into(),
        )
    })?;
    let directory = executable
        .parent()
        .ok_or_else(|| Error::Config("BetterGI 安装路径无效。".into()))?
        .to_path_buf();
    launch_host(&executable, &directory, config.launch_silently)?;
    let started = std::time::Instant::now();
    while !host_running() {
        if started.elapsed() >= Duration::from_secs(30) {
            return Err(Error::Tool(
                "已尝试启动 BetterGI，但进程迟迟没有出现。请手动启动后重试。".into(),
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

fn launch_host(executable: &Path, directory: &Path, silently: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    if silently {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, PROCESS_INFORMATION,
                ResumeThread, STARTF_USESHOWWINDOW, STARTUPINFOW,
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, SW_HIDE},
        };

        let executable: Vec<u16> = executable.as_os_str().encode_wide().chain([0]).collect();
        let directory: Vec<u16> = directory.as_os_str().encode_wide().chain([0]).collect();
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        startup.dwFlags = STARTF_USESHOWWINDOW;
        startup.wShowWindow = SW_HIDE as u16;
        let mut process: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let foreground = unsafe { GetForegroundWindow() } as isize;
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_SUSPENDED | CREATE_NO_WINDOW,
                std::ptr::null(),
                directory.as_ptr(),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(Error::Tool(format!(
                "后台启动 BetterGI 失败：{}",
                std::io::Error::last_os_error()
            )));
        }
        let process_id = process.dwProcessId;
        let process_handle = process.hProcess as isize;
        std::thread::spawn(move || {
            hide_process_windows(process_id, process_handle, foreground);
        });
        unsafe {
            ResumeThread(process.hThread);
            CloseHandle(process.hThread);
        }
        return Ok(());
    }

    let mut command = std::process::Command::new(executable);
    command
        .current_dir(directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn()?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn hide_process_windows(process_id: u32, process_handle: isize, foreground: isize) {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, WAIT_OBJECT_0},
        System::Threading::WaitForSingleObject,
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_HIDE, SetForegroundWindow,
            ShowWindow,
        },
    };

    struct Context {
        process_id: u32,
        hidden: bool,
    }

    unsafe extern "system" fn hide(hwnd: HWND, lparam: LPARAM) -> i32 {
        let context = unsafe { &mut *(lparam as *mut Context) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
        if owner == context.process_id && unsafe { IsWindowVisible(hwnd) } != 0 {
            unsafe { ShowWindow(hwnd, SW_HIDE) };
            context.hidden = true;
        }
        1
    }

    let process_handle = process_handle as *mut std::ffi::c_void;
    for _ in 0..500 {
        if unsafe { WaitForSingleObject(process_handle, 0) } == WAIT_OBJECT_0 {
            break;
        }
        let mut context = Context {
            process_id,
            hidden: false,
        };
        unsafe { EnumWindows(Some(hide), (&mut context as *mut Context) as LPARAM) };
        if context.hidden && foreground != 0 {
            unsafe { SetForegroundWindow(foreground as HWND) };
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    unsafe { CloseHandle(process_handle) };
}

/// BetterGI 可执行文件的位置：上次连接记住的目录 → 卸载注册表 → 常见安装目录。
fn locate_host(config: &BridgeConfig) -> Option<PathBuf> {
    if let Some(remembered) = config.host_install_path.as_ref() {
        let executable = remembered.join("BetterGI.exe");
        if executable.is_file() {
            return Some(executable);
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(executable) = registry_host_executable() {
        return Some(executable);
    }
    common_host_roots()
        .into_iter()
        .map(|root| root.join("BetterGI").join("BetterGI.exe"))
        .find(|executable| executable.is_file())
}

fn common_host_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for key in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        let Some(value) = std::env::var_os(key) else {
            continue;
        };
        let base = PathBuf::from(value);
        for root in [base.clone(), base.join("Programs")] {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

/// 从卸载注册表（安装器写入的标准位置）找 BetterGI 的安装目录。
#[cfg(target_os = "windows")]
fn registry_host_executable() -> Option<PathBuf> {
    const UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    for hive in ["HKCU", r"HKLM\SOFTWARE", r"HKLM\SOFTWARE\WOW6432Node"] {
        let root = format!(r"{hive}\{UNINSTALL}");
        let Some(search) = reg_query(&[
            "reg",
            "query",
            &root,
            "/s",
            "/f",
            "BetterGI",
            "/v",
            "DisplayName",
            "/d",
        ]) else {
            continue;
        };
        for key in parse_matching_keys(&search) {
            let Some(values) = reg_query(&["reg", "query", &key, "/v", "InstallLocation"]) else {
                continue;
            };
            if let Some(location) = parse_reg_value(&values, "InstallLocation") {
                let executable = PathBuf::from(&location).join("BetterGI.exe");
                if executable.is_file() {
                    return Some(executable);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn reg_query(args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(args[0]);
    command
        .args(&args[1..])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// 从 `reg query /s /f` 的输出里取匹配到的键路径（`HKEY_` 开头的行）。
#[cfg(target_os = "windows")]
fn parse_matching_keys(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("HKEY_"))
        .map(str::to_string)
        .collect()
}

/// 从 `reg query <键> /v <名>` 的输出里取字符串值。
#[cfg(target_os = "windows")]
fn parse_reg_value(output: &str, name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let (_, value) = line.split_once("REG_SZ")?;
        if !line.starts_with(name) {
            return None;
        }
        let value = value.trim().trim_matches('"').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// 产品默认监听端口。占用时从这里往后找空位。
const DEFAULT_PORT: u16 = 26101;
const PORT_SPAN: u16 = 64;

fn endpoint(config: &BridgeConfig) -> Result<(String, u16)> {
    let url = url::Url::parse(&config.base_url)
        .map_err(|_| Error::Config("BetterGI 地址无效。".into()))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Config("BetterGI 地址无效。".into()));
    }
    let port = url.port().unwrap_or(DEFAULT_PORT);
    Ok((format!("127.0.0.1:{port}"), port))
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn allocate_listen(preferred: u16) -> Result<(String, u16)> {
    let start = preferred.max(1);
    for offset in 0..PORT_SPAN {
        let Some(port) = start.checked_add(offset) else {
            break;
        };
        if port_free(port) {
            return Ok((format!("127.0.0.1:{port}"), port));
        }
    }
    Err(Error::Tool(
        "本地端口已被占满。请关掉占用 26101 附近端口的程序后重试。".into(),
    ))
}

fn request(config: &BridgeConfig, enabled: Option<bool>) -> Result<Value> {
    endpoint(config)?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .new_agent();
    let base = config.base_url.trim_end_matches('/');
    let authorization = format!("Bearer {}", config.token.as_deref().unwrap_or_default());
    let mut response = if let Some(enabled) = enabled {
        agent
            .post(&format!("{base}/bridge/v1/control"))
            .header("authorization", &authorization)
            .send_json(json!({"enabled":enabled}))?
    } else {
        agent
            .get(&format!("{base}/bridge/v1/info"))
            .header("authorization", &authorization)
            .call()?
    };
    Ok(response.body_mut().read_json()?)
}

pub fn info(config: &BridgeConfig) -> Result<Value> {
    let value = request(config, None)?;
    if value["protocolVersion"] != "1" || value["instanceId"].as_str().is_none() {
        return Err(Error::Tool(
            "连上的不是可用的 BetterGI。请退出 BetterGI 后重试。".into(),
        ));
    }
    Ok(value)
}

fn control(config: &BridgeConfig, enabled: bool) -> Result<()> {
    let info = info(config)?;
    if !info["features"]
        .as_array()
        .is_some_and(|v| v.iter().any(|f| f == "control"))
    {
        return Err(Error::Tool(
            "当前 BetterGI 连接方式已过期。请退出 BetterGI 后重新连接。".into(),
        ));
    }
    let result = request(config, Some(enabled))?;
    if result["enabled"] != enabled {
        return Err(Error::Tool("连接状态未确认，请重试。".into()));
    }
    Ok(())
}

/// 保留方法/分组设置，重试时复用 token。
pub fn prepare(config: &mut BridgeConfig) -> Result<()> {
    let (_, preferred) = endpoint(config)?;
    let occupied = !port_free(preferred);
    ensure_host_running(config)?;
    let dir = directory()?;
    let path = dir.join("bridge.config.json");
    let mut settings: Value = if path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&path)?)?
    } else {
        serde_json::from_str(include_str!("../../bgi-bridge/bridge.config.example.json"))?
    };
    if !settings.is_object() {
        return Err(Error::Config(
            "连接配置损坏。请重新安装 Sleepy Doll。".into(),
        ));
    }
    if config.token.as_deref().is_none_or(|t| t.trim().is_empty()) {
        config.token = Some(
            settings["token"]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    )
                }),
        );
    }
    if occupied {
        // 占用方已是本产品的桥：listen 与 token 不改，只钉数据根。
        if info(config).is_ok() {
            if pin_configured_root(&mut settings, &exe_dir()?, &dir) {
                crate::config::atomic_write(&path, &settings)?;
            }
            return Ok(());
        }
        if !host_running() {
            return Err(Error::Tool("请先启动 BetterGI。".into()));
        }
        let (_, port) = allocate_listen(preferred.saturating_add(1))?;
        log::warn!("端口 {preferred} 已被占用，本次连接改用 {port}");
        config.base_url = format!("http://127.0.0.1:{port}");
    }
    let (listen, _) = endpoint(config)?;
    for file in [
        "BgiBridge.Bootstrap.dll",
        "BgiBridge.dll",
        "BgiBridge.runtimeconfig.json",
        "BgiBridge.deps.json",
    ] {
        if !dir.join(file).is_file() {
            return Err(Error::Config(
                "无法连接 BetterGI：安装不完整。请重新安装 Sleepy Doll。".into(),
            ));
        }
    }
    settings["enabled"] = json!(true);
    settings["listen"] = json!(listen);
    settings["token"] = json!(config.token);
    pin_configured_root(&mut settings, &exe_dir()?, &dir);
    crate::config::atomic_write(&path, &settings)
}

fn pin_configured_root(settings: &mut Value, install: &Path, dir: &Path) -> bool {
    let Some(root) = data_root(install, dir) else {
        return false;
    };
    let wanted = root.to_string_lossy().to_string();
    if settings.get("userDirectory").and_then(Value::as_str) == Some(wanted.as_str()) {
        return false;
    }
    settings["userDirectory"] = json!(wanted);
    true
}

pub fn enable(config: &BridgeConfig) -> Result<()> {
    if info(config).is_ok() {
        return control(config, true);
    }
    log::info!("桥未就绪，开始注入 BetterGI 进程");
    inject()?;
    let started = std::time::Instant::now();
    let until = started + Duration::from_secs(20);
    loop {
        if info(config).is_ok() {
            log::info!("注入后 {:.1}s 桥已响应", started.elapsed().as_secs_f64());
            return control(config, true);
        }
        if std::time::Instant::now() >= until {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    log::warn!("注入完成，但 20 秒内没有得到桥的响应");
    Err(Error::Tool(
        "BetterGI 已启动，但还没连上。请退出 BetterGI 后重试。".into(),
    ))
}

pub fn disable(config: &BridgeConfig) -> Option<String> {
    control(config, false)
        .err()
        .map(|_| "已断开。如需完全卸下连接，请退出 BetterGI。".into())
}

pub fn recovery(action: &str, arguments: &[&str]) -> Result<Value> {
    let directory = directory().map_err(|_| Error::Config("暂时无法恢复配置。".into()))?;
    let executable = directory.join("BgiBridge.Recovery.exe");
    if !executable.is_file() {
        return Err(Error::Config(
            "暂时无法恢复配置。请重新安装 Sleepy Doll。".into(),
        ));
    }
    crate::runtime::executor().block_on(async {
        let mut command = tokio::process::Command::new(executable);
        command
            .arg(action)
            .args(arguments)
            .current_dir(directory)
            .kill_on_drop(true);
        crate::runtime::host::process::isolate_environment(&mut command);
        crate::runtime::host::process::hide_console(&mut command);
        let output = tokio::time::timeout(std::time::Duration::from_secs(12), command.output())
            .await
            .map_err(|_| Error::Tool("恢复超时，结果尚未确认。先不要启动 BetterGI。".into()))??;
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| Error::Tool("恢复配置失败，请稍后重试。".into()))?;
        if value["ok"] != true {
            return Err(Error::Tool(
                value["error"]["message"]
                    .as_str()
                    .unwrap_or("配置恢复失败。")
                    .into(),
            ));
        }
        Ok(value["result"].clone())
    })
}

/// 桥在安装目录里的全部交付文件，注入副本按这份清单整体复制。
#[cfg(target_os = "windows")]
const BRIDGE_FILES: [&str; 9] = [
    "BgiBridge.Injector.exe",
    "BgiBridge.Bootstrap.dll",
    "BgiBridge.dll",
    "BgiBridge.runtimeconfig.json",
    "BgiBridge.deps.json",
    "BgiBridge.Recovery.exe",
    "BgiBridge.Recovery.dll",
    "BgiBridge.Recovery.runtimeconfig.json",
    "BgiBridge.Recovery.deps.json",
];

/// 注入用的工作副本（影拷贝）。
///
/// 安装目录里的桥文件一旦加载进 BetterGI 就锁到进程退出，升级、重装、重打包都换不了
/// 文件。注入一律从这份按内容指纹缓存的副本走：安装目录永远不被加载、永远可覆盖；
/// 换版本得到新指纹目录；旧副本等不再被加载后自动清掉。
#[cfg(target_os = "windows")]
fn injection_copy(source: &Path, root: &Path) -> Result<PathBuf> {
    let target = root.join(fingerprint(source)?);
    if !BRIDGE_FILES.iter().all(|file| target.join(file).is_file()) {
        let staging = root.join(format!(".staging-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        for file in BRIDGE_FILES {
            std::fs::copy(source.join(file), staging.join(file))?;
        }
        // 撞上并发建好的同指纹副本就直接用；别的失败才是真失败。
        if std::fs::rename(&staging, &target).is_err() {
            let _ = std::fs::remove_dir_all(&staging);
            if !BRIDGE_FILES.iter().all(|file| target.join(file).is_file()) {
                return Err(Error::Config("桥的注入副本建立失败。请重试。".into()));
            }
        }
    }
    // 配置不参与指纹：token 与监听端口以安装目录为准，每次注入都带最新的。
    std::fs::copy(
        source.join("bridge.config.json"),
        target.join("bridge.config.json"),
    )?;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path() != target {
                // 还被加载着的旧副本删不掉，跳过，下次再清。
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    Ok(target)
}

/// 副本目录名：桥文件内容的指纹。内容变了就是新目录，安装目录随之可整体替换。
#[cfg(target_os = "windows")]
fn fingerprint(source: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for file in BRIDGE_FILES {
        hasher.update(file.as_bytes());
        hasher.update(std::fs::read(source.join(file))?);
    }
    Ok(hasher
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// 副本放哪：优先安装目录旁边；安装位置不可写时退到漫游目录。
#[cfg(target_os = "windows")]
fn cache_root() -> Result<PathBuf> {
    let beside = exe_dir()?.join("bridge-cache");
    if std::fs::create_dir_all(&beside).is_ok() && directory_writable(&beside) {
        return Ok(beside);
    }
    let roaming = crate::config::fallback_directory()
        .ok_or_else(|| Error::Config("无法定位可写的桥缓存目录。".into()))?
        .join("bridge-cache");
    std::fs::create_dir_all(&roaming)?;
    Ok(roaming)
}

#[cfg(target_os = "windows")]
fn directory_writable(directory: &Path) -> bool {
    let probe = directory.join(format!(".probe-{}", std::process::id()));
    std::fs::write(&probe, b"").is_ok() && std::fs::remove_file(&probe).is_ok()
}

#[cfg(target_os = "windows")]
fn inject() -> Result<()> {
    use std::os::windows::process::CommandExt;
    let source = directory()?;
    let install = exe_dir()?;
    let dir = injection_copy(&source, &cache_root()?)?;
    let user = data_root(&install, &source).unwrap_or_else(|| source.join("user"));
    let mut child = std::process::Command::new(dir.join("BgiBridge.Injector.exe"))
        .arg("--process")
        .arg("BetterGI.exe")
        .arg("--bridge")
        .arg(&dir)
        .arg("--user")
        .arg(&user)
        .current_dir(&dir)
        .creation_flags(0x08000000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(65);
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            log::warn!("注入器退出码 {:?}", status.code());
            let message = match status.code() {
                Some(2) => "请以管理员身份运行 Sleepy Doll，再开启 BetterGI 连接。",
                Some(5) => {
                    "未能选定 BetterGI 进程。请先启动 BetterGI，保持仅一个实例，并确认 Sleepy Doll 具有管理员权限。"
                }
                Some(10) => "连接超时。请重启 BetterGI 后再试。",
                Some(11) => "BetterGI 里已有旧连接。请先退出 BetterGI，再重新连接。",
                Some(12) => "请使用 64 位的 BetterGI。",
                _ => "没能连上 BetterGI。请确认 BetterGI 已启动，然后重试。",
            };
            return Err(Error::Tool(message.into()));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            log::warn!("注入器 65 秒未退出，已终止");
            return Err(Error::Tool("连接超时。请重启 BetterGI 后再试。".into()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(target_os = "windows"))]
fn inject() -> Result<()> {
    Err(Error::Config("BetterGI 注入仅支持 Windows。".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_packaged_layout_repins_the_data_root() {
        let install = Path::new(r"C:\Program Files\Sleepy Doll");
        assert_eq!(
            data_root(install, &install.join("bridge")),
            Some(install.join("user"))
        );
        // 扁平包与开发布局解析自己的数据根。
        assert_eq!(data_root(install, install), None);
        assert_eq!(
            data_root(
                Path::new(r"C:\repo\target\debug"),
                &PathBuf::from(r"C:\repo\target\bridge")
            ),
            None
        );
    }

    #[test]
    fn pin_configured_root_only_writes_when_the_value_changes() {
        let install = Path::new(r"D:\Sleepy Doll");
        let dir = install.join("bridge");
        let mut settings = json!({"token": "keep"});
        assert!(pin_configured_root(&mut settings, install, &dir));
        assert_eq!(
            settings["userDirectory"],
            install.join("user").to_string_lossy().as_ref()
        );
        assert!(!pin_configured_root(&mut settings, install, &dir));
        assert_eq!(settings["token"], "keep");
        assert!(!pin_configured_root(&mut settings, install, install));
    }

    #[test]
    fn injection_endpoint_is_local_and_unambiguous() {
        let mut config = BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:26101".into(),
            token: None,
            instance_id: None,
            timeout_ms: 1000,
            host_install_path: None,
            launch_silently: true,
        };
        assert_eq!(
            endpoint(&config).unwrap(),
            ("127.0.0.1:26101".into(), 26101)
        );
        config.base_url = "http://127.0.0.1".into();
        assert_eq!(
            endpoint(&config).unwrap(),
            ("127.0.0.1:26101".into(), 26101)
        );
        for invalid in [
            "https://127.0.0.1:26101",
            "http://example.com",
            "http://user:secret@localhost",
            "http://localhost/path",
            "http://localhost/?x=1",
        ] {
            config.base_url = invalid.into();
            assert!(endpoint(&config).is_err(), "{invalid}");
        }
    }

    #[test]
    fn allocate_listen_skips_an_occupied_port() {
        let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let (listen, next) = allocate_listen(port).unwrap();
        assert_ne!(next, port);
        assert_eq!(listen, format!("127.0.0.1:{next}"));
    }

    #[test]
    fn locate_host_prefers_a_remembered_install() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("BetterGI.exe"), b"").unwrap();
        let config = BridgeConfig {
            host_install_path: Some(dir.path().to_path_buf()),
            ..endpoint_config()
        };
        assert_eq!(locate_host(&config), Some(dir.path().join("BetterGI.exe")));
    }

    #[test]
    fn remembered_install_without_the_executable_falls_through() {
        let dir = tempfile::tempdir().unwrap();
        let config = BridgeConfig {
            host_install_path: Some(dir.path().to_path_buf()),
            ..endpoint_config()
        };
        // 目录在但可执行文件不在：不能拿无效路径去启动；找没找到另说，反正不是它。
        assert_ne!(locate_host(&config), Some(dir.path().join("BetterGI.exe")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn reg_search_output_yields_matching_keys_only() {
        let output = "\
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\BetterGI
    DisplayName    REG_SZ    BetterGI

End of search: 1 match(es) found.
";
        assert_eq!(
            parse_matching_keys(output),
            vec![
                r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\BetterGI"
                    .to_string()
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn reg_value_output_yields_the_location() {
        let output = "\
HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\BetterGI
    InstallLocation    REG_SZ    C:\\Apps\\BetterGI

";
        assert_eq!(
            parse_reg_value(output, "InstallLocation").as_deref(),
            Some(r"C:\Apps\BetterGI")
        );
        assert_eq!(parse_reg_value(output, "DisplayName"), None);
        assert_eq!(
            parse_reg_value("    InstallLocation    REG_SZ       ", "InstallLocation"),
            None
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn injection_copy_reuses_one_dir_and_refreshes_only_the_config() {
        let source = tempfile::tempdir().unwrap();
        for file in BRIDGE_FILES {
            std::fs::write(source.path().join(file), b"v1").unwrap();
        }
        std::fs::write(source.path().join("bridge.config.json"), r#"{"token":"a"}"#).unwrap();
        let root = tempfile::tempdir().unwrap();

        let first = injection_copy(source.path(), root.path()).unwrap();
        assert!(first.join("BgiBridge.dll").is_file());
        assert_eq!(
            std::fs::read_to_string(first.join("bridge.config.json")).unwrap(),
            r#"{"token":"a"}"#
        );

        // token 或监听端口变了：同一份副本，只刷新配置，不重建目录。
        std::fs::write(source.path().join("bridge.config.json"), r#"{"token":"b"}"#).unwrap();
        let second = injection_copy(source.path(), root.path()).unwrap();
        assert_eq!(second, first);
        assert_eq!(
            std::fs::read_to_string(second.join("bridge.config.json")).unwrap(),
            r#"{"token":"b"}"#
        );

        // 桥内容变了：新指纹目录接替，旧副本被清掉。
        std::fs::write(source.path().join("BgiBridge.dll"), b"v2").unwrap();
        let third = injection_copy(source.path(), root.path()).unwrap();
        assert_ne!(third, first);
        assert!(third.join("BgiBridge.dll").is_file());
        assert!(!first.exists());
        // 只有当前副本留在缓存里。
        let remaining: Vec<_> = std::fs::read_dir(root.path()).unwrap().flatten().collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path(), third);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn fingerprint_tracks_file_content_not_order_noise() {
        let source = tempfile::tempdir().unwrap();
        for file in BRIDGE_FILES {
            std::fs::write(source.path().join(file), b"same").unwrap();
        }
        let first = fingerprint(source.path()).unwrap();
        // 内容不变：重算指纹稳定。
        assert_eq!(fingerprint(source.path()).unwrap(), first);
        // 任意交付文件变化：指纹必须变。
        std::fs::write(source.path().join("BgiBridge.deps.json"), b"changed").unwrap();
        assert_ne!(fingerprint(source.path()).unwrap(), first);
    }

    fn endpoint_config() -> BridgeConfig {
        BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:26101".into(),
            token: None,
            instance_id: None,
            timeout_ms: 30_000,
            host_install_path: None,
            launch_silently: true,
        }
    }
}
