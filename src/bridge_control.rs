//! Local bridge lifecycle. Only the desktop IPC invokes the injector; this is
//! deliberately not exposed as an agent tool.
use std::{path::PathBuf, time::Duration};

use serde_json::{Value, json};

use crate::{
    config::BridgeConfig,
    error::{Error, Result},
};

pub fn directory() -> Result<PathBuf> {
    let beside_exe = std::env::current_exe()?.parent().unwrap().to_path_buf();
    if beside_exe.join("BgiBridge.Injector.exe").is_file() {
        return Ok(beside_exe);
    }
    // Cargo development builds run outside the distribution directory.
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bgi-bridge/dist");
    if cfg!(debug_assertions) && development.join("BgiBridge.Injector.exe").is_file() {
        return Ok(development);
    }
    Err(Error::Config(
        "找不到桥组件，请运行 build-desktop.cmd，将桥组件放到程序同一目录。".into(),
    ))
}

fn endpoint(config: &BridgeConfig) -> Result<(String, u16)> {
    let url =
        url::Url::parse(&config.base_url).map_err(|_| Error::Config("BetterGI 地址无效".into()))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Config(
            "注入桥地址须为 http://127.0.0.1:端口（也可使用 localhost）".into(),
        ));
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
            "该端口不是兼容的 BetterGI 桥，请检查地址或更新桥组件。".into(),
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
            "当前桥不支持开关，请更新桥组件并重启 BetterGI。".into(),
        ));
    }
    let result = request(config, Some(enabled))?;
    if result["enabled"] != enabled {
        return Err(Error::Tool("桥未确认开关状态，请重试。".into()));
    }
    Ok(())
}

/// Preserve method/group settings and reuse the token across retries. The app
/// persists this token before injection, so even a delayed start is recoverable.
pub fn prepare(config: &mut BridgeConfig) -> Result<()> {
    let (listen, _) = endpoint(config)?;
    let dir = directory()?;
    let path = dir.join("bridge.config.json");
    let mut settings: Value = if path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&path)?)?
    } else {
        serde_json::from_str(include_str!("../bgi-bridge/bridge.config.example.json"))?
    };
    if !settings.is_object() {
        return Err(Error::Config("bridge.config.json 必须是 JSON 对象".into()));
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
    let (_, port) = endpoint(config)?;
    if std::net::TcpStream::connect_timeout(
        &([127, 0, 0, 1], port).into(),
        Duration::from_millis(500),
    )
    .is_ok()
    {
        info(config).map_err(|_| {
            Error::Tool(
                "桥端口已有服务，但鉴权或协议检查失败。请核对 token，或退出旧 BetterGI 后重试。"
                    .into(),
            )
        })?;
        return Ok(());
    }
    for file in [
        "BgiBridge.Bootstrap.dll",
        "BgiBridge.dll",
        "BgiBridge.runtimeconfig.json",
        "BgiBridge.deps.json",
    ] {
        if !dir.join(file).is_file() {
            return Err(Error::Config(format!("缺少桥组件 {file}，请重新构建。")));
        }
    }
    settings["enabled"] = json!(true);
    settings["listen"] = json!(listen);
    settings["token"] = json!(config.token);
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
    Err(Error::Tool(format!(
        "桥启动未确认。请查看 {} 中的 bootstrap.log 和 bridge.log；修复后重启 BetterGI 再连接。",
        directory()?.display()
    )))
}

pub fn disable(config: &BridgeConfig) -> Option<String> {
    control(config, false)
        .err()
        .map(|_| "Sleepy Doll 已断开；未能确认宿主桥已停用。退出 BetterGI 可完全卸载桥。".into())
}

pub fn recovery(action: &str, arguments: &[&str]) -> Result<Value> {
    let directory = directory()?;
    let executable = directory.join("BgiBridge.Recovery.exe");
    if !executable.is_file() {
        return Err(Error::Config(
            "未找到配置恢复组件，请重新构建并更新桥组件。".into(),
        ));
    }
    crate::runtime::executor().block_on(async {
        let mut command = tokio::process::Command::new(executable);
        command
            .arg(action)
            .args(arguments)
            .current_dir(directory)
            .kill_on_drop(true);
        crate::runtime::process::isolate_environment(&mut command);
        crate::runtime::process::hide_console(&mut command);
        let output = tokio::time::timeout(std::time::Duration::from_secs(12), command.output())
            .await
            .map_err(|_| {
                Error::Tool(
                    "恢复请求超时，结果尚未确认。请刷新记录后核对，暂勿启动 BetterGI。".into(),
                )
            })??;
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| Error::Tool("恢复组件没有返回有效结果，请检查 .NET 运行环境。".into()))?;
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
    let mut child = std::process::Command::new(dir.join("BgiBridge.Injector.exe"))
        .arg("--process")
        .arg("BetterGI.exe")
        .arg("--bridge")
        .arg(&dir)
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
                Some(10) => "加载等待超时，状态未知。请核对桥日志后重启 BetterGI，勿连续重试。",
                Some(11) => "桥已加载或无法检查模块。请核对连接凭据；更新桥后需要重启 BetterGI。",
                Some(12) => "目标架构不兼容，请使用 x64 版本 BetterGI。",
                _ => "桥加载失败，请查看桥目录中的 injector.log 和 bootstrap.log。",
            };
            return Err(Error::Tool(message.into()));
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Tool(
                "注入器超时；请检查桥日志，并在重启 BetterGI 后重试。".into(),
            ));
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
