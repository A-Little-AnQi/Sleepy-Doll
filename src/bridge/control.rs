//! Local bridge lifecycle. Only the desktop IPC invokes the injector; this is
//! deliberately not exposed as an agent tool.
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
    // Shipping packages keep the components in a `bridge` subdirectory; earlier
    // packages put them directly beside the executable.
    for candidate in [beside_exe.join("bridge"), beside_exe.clone()] {
        if candidate.join("BgiBridge.Injector.exe").is_file() {
            return Ok(candidate);
        }
    }
    // Cargo writes to <manifest>/target in both profiles, so a binary running
    // from there is a development build even in release. The bridge lands in
    // target/bridge, laid out flat, beside Cargo's own output.
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

/// The one data root, `<install>/user`. Components live in `<install>/bridge`
/// and would otherwise put their logs and recovery records in a second `user`
/// directory inside the product folder, which an upgrade replaces wholesale.
/// Anything else — a flat package, the development layout — keeps the root the
/// bridge itself resolves, so `None` here means "do not override it".
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
    let port = url.port_or_known_default().unwrap_or(3499);
    Ok((format!("127.0.0.1:{port}"), port))
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

/// Preserve method/group settings and reuse the token across retries. The app
/// persists this token before injection, so even a delayed start is recoverable.
pub fn prepare(config: &mut BridgeConfig) -> Result<()> {
    let (listen, port) = endpoint(config)?;
    let port_busy = std::net::TcpStream::connect_timeout(
        &([127, 0, 0, 1], port).into(),
        Duration::from_millis(500),
    )
    .is_ok();
    if !host_running() && !port_busy {
        return Err(Error::Tool("请先启动 BetterGI。".into()));
    }
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
    // An existing listener must be authenticated before changing its settings.
    if port_busy {
        info(config)
            .map_err(|_| Error::Tool("BetterGI 端口已被占用。请退出 BetterGI 后重试。".into()))?;
        return Ok(());
    }
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
    // The native and managed sides take their data root from here, because the
    // bridge directory is no longer the installation directory.
    if let Some(root) = data_root(&exe_dir()?, &dir) {
        settings["userDirectory"] = json!(root.to_string_lossy());
    }
    crate::config::atomic_write(&path, &settings)
}

pub fn enable(config: &BridgeConfig) -> Result<()> {
    if info(config).is_ok() {
        return control(config, true);
    }
    inject()?;
    let until = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if info(config).is_ok() {
            return control(config, true);
        }
        if std::time::Instant::now() >= until {
            break;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
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
        // A flat package and the development layout resolve their own root.
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
    fn injection_endpoint_is_local_and_unambiguous() {
        let mut config = BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:3499".into(),
            token: None,
            instance_id: None,
            timeout_ms: 1000,
        };
        assert_eq!(endpoint(&config).unwrap(), ("127.0.0.1:3499".into(), 3499));
        for invalid in [
            "https://127.0.0.1:3499",
            "http://example.com",
            "http://user:secret@localhost",
            "http://localhost/path",
            "http://localhost/?x=1",
        ] {
            config.base_url = invalid.into();
            assert!(endpoint(&config).is_err(), "{invalid}");
        }
    }
}
