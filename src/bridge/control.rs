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
    let mut command = std::process::Command::new(&executable);
    command
        .current_dir(&directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn()?;
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

#[cfg(target_os = "windows")]
fn inject() -> Result<()> {
    use std::os::windows::process::CommandExt;
    let dir = directory()?;
    let install = exe_dir()?;
    let user = data_root(&install, &dir).unwrap_or_else(|| dir.join("user"));
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

    fn endpoint_config() -> BridgeConfig {
        BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:26101".into(),
            token: None,
            instance_id: None,
            timeout_ms: 30_000,
            host_install_path: None,
        }
    }
}
