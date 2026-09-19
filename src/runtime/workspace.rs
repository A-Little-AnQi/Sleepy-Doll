//! 软件目录内的本机操作。用户没有 Node / Python / Git 等开发环境，命令只通过
//! PowerShell；文件工具在同一套边界内直接读写。
//!
//! BetterGI 的安装目录与 User 目录即使落在软件目录里也一律拒绝：宿主配置
//! 只能走桥的设置事务。

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

use crate::{
    bridge::BgiClient,
    error::{Error, Result},
    extension::{
        FunctionTool, RiskLevel, ScopeKind, Tool, ToolDefinition, ToolEffect, ToolExecution,
        ToolRegistry,
    },
};

const READ_LIMIT: usize = 128 * 1024;
const WRITE_LIMIT: usize = 2 * 1024 * 1024;
const COMMAND_LIMIT: usize = 32 * 1024;
const SHELL_OUTPUT_LIMIT: usize = 64 * 1024;

/// 产品主目录：`user/config.json` 的上两级，也就是安装目录或数据根。
pub fn software_root(config_path: &Path) -> PathBuf {
    let Some(parent) = config_path.parent() else {
        return PathBuf::from(".");
    };
    if parent.file_name().is_some_and(|name| name == "user") {
        parent.parent().unwrap_or(parent).to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    bridge: Option<Arc<BgiClient>>,
    denied: Vec<PathBuf>,
}

impl Workspace {
    pub fn from_config(config_path: &Path, bridge: Option<Arc<BgiClient>>) -> Self {
        Self {
            root: software_root(config_path),
            bridge,
            denied: Vec::new(),
        }
    }

    fn denials(&self) -> Vec<PathBuf> {
        let mut denied = self.denied.clone();
        if let Some(bridge) = &self.bridge
            && let Ok(host) = bridge.host()
        {
            for key in ["userPath", "installPath"] {
                if let Some(path) = host[key].as_str().filter(|path| !path.is_empty()) {
                    denied.push(PathBuf::from(path));
                }
            }
        }
        denied
    }

    fn resolve(&self, relative: &str) -> Result<PathBuf> {
        resolve_inside(&self.root, relative, &self.denials())
    }

    fn list(&self, relative: &str) -> Result<Value> {
        let path = self.resolve(relative)?;
        let read =
            fs::read_dir(&path).map_err(|error| Error::Tool(format!("读取目录失败：{error}")))?;
        let mut entries = Vec::new();
        for entry in read {
            let entry = entry.map_err(|error| Error::Tool(format!("读取目录失败：{error}")))?;
            let meta = entry.metadata().ok();
            entries.push(json!({
                "name": entry.file_name().to_string_lossy(),
                "directory": meta.as_ref().is_some_and(|meta| meta.is_dir()),
                "bytes": meta.as_ref().filter(|meta| meta.is_file()).map(|meta| meta.len()),
            }));
        }
        entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        Ok(json!({
            "root": self.root.to_string_lossy(),
            "path": display_relative(&self.root, &path),
            "count": entries.len(),
            "entries": entries,
        }))
    }

    fn read(&self, relative: &str) -> Result<Value> {
        let path = self.resolve(relative)?;
        let bytes = fs::read(&path).map_err(|error| Error::Tool(format!("读取失败：{error}")))?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(json!({
            "path": display_relative(&self.root, &path),
            "bytes": bytes.len(),
            "sha256": sha256(&bytes),
            "chars": text.chars().count(),
            "truncated": text.chars().count() > READ_LIMIT,
            "text": text.chars().take(READ_LIMIT).collect::<String>(),
        }))
    }

    fn write(&self, relative: &str, content: &str, expected: Option<&str>) -> Result<Value> {
        if content.len() > WRITE_LIMIT {
            return Err(Error::Tool("写入内容超过 2 MiB 上限".into()));
        }
        let path = self.resolve(relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if path.exists() {
            let current = fs::read(&path)?;
            let actual = sha256(&current);
            let expected = expected.ok_or_else(|| {
                Error::Tool("目标已存在；必须先读取并提交 expectedSha256，未写入".into())
            })?;
            if actual != expected {
                return Err(Error::Conflict(
                    "目标文件在读取后发生变化；未覆盖较新的内容，请重新读取".into(),
                ));
            }
        } else if expected.is_some() {
            return Err(Error::Conflict(
                "目标文件已不存在；未按替换请求重新创建".into(),
            ));
        }
        let parent = path
            .parent()
            .ok_or_else(|| Error::Tool("写入路径无效".into()))?;
        let temporary = parent.join(format!(
            ".{}.{}.sleepy-tmp",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default(),
            uuid::Uuid::new_v4()
        ));
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&temporary, &path)?;
        Ok(json!({
            "path": display_relative(&self.root, &path),
            "sha256": sha256(content.as_bytes()),
            "bytes": content.len(),
        }))
    }

    fn delete(&self, relative: &str) -> Result<Value> {
        let path = self.resolve(relative)?;
        if !path.exists() {
            return Err(Error::Tool("目标不存在".into()));
        }
        let directory = path.is_dir();
        if directory {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
        Ok(json!({
            "path": display_relative(&self.root, &path),
            "deleted": true,
            "directory": directory,
        }))
    }

    fn shell_sync(&self, command: &str, timeout: Duration) -> Result<Value> {
        crate::runtime::executor().block_on(self.shell(command, timeout, CancellationToken::new()))
    }

    async fn shell(
        &self,
        command: &str,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let command = command.trim();
        if command.is_empty() {
            return Err(Error::Tool("PowerShell 命令为空".into()));
        }
        if command.len() > COMMAND_LIMIT {
            return Err(Error::Tool("PowerShell 命令超过 32 KiB 上限".into()));
        }
        let exe = powershell_executable()?;
        let payload = json!({
            "workspace": self.root.to_string_lossy(),
            "deny": self.denials().iter().map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "command": command,
        });
        let encoded = STANDARD.encode(payload.to_string());
        let script = host_script(&encoded);
        let encoded_command = encode_powershell(&script);

        let mut process = tokio::process::Command::new(&exe);
        process
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-EncodedCommand",
                &encoded_command,
            ])
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::runtime::host::process::isolate_environment(&mut process);
        crate::runtime::host::process::hide_console(&mut process);
        let mut child = process.spawn()?;
        let _constraint = crate::runtime::host::process::constrain_process(&child)?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let stdout_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = stdout.take() {
                let _ = pipe.read_to_end(&mut buffer).await;
            }
            buffer
        });
        let stderr_task = tokio::spawn(async move {
            let mut buffer = Vec::new();
            if let Some(mut pipe) = stderr.take() {
                let _ = pipe.read_to_end(&mut buffer).await;
            }
            buffer
        });

        let wait = tokio::time::timeout(timeout, async {
            tokio::select! {
                _ = cancel.cancelled() => Err(Error::Cancelled),
                status = child.wait() => status.map_err(Error::from),
            }
        });
        let status = match wait.await {
            Ok(Ok(status)) => status,
            Ok(Err(Error::Cancelled)) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(Error::Cancelled);
            }
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(Error::Tool("PowerShell 执行超时".into()));
            }
        };
        let stdout = truncate_output(&stdout_task.await.unwrap_or_default());
        let stderr = truncate_output(&stderr_task.await.unwrap_or_default());
        Ok(json!({
            "cwd": self.root.to_string_lossy(),
            "exitCode": status.code().unwrap_or(-1),
            "stdout": stdout,
            "stderr": stderr,
        }))
    }
}

pub fn register_tools(registry: &mut ToolRegistry, workspace: Arc<Workspace>) -> Result<()> {
    let read = ToolExecution {
        deferred: false,
        always_load: true,
        max_result_chars: 96_000,
        search_hint: Some("读取软件目录文件".into()),
        ..ToolExecution::read_only()
    };
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.list",
            "列出软件目录内指定位置的一层文件和目录。path 为空表示软件目录根。用户没有开发环境，不要改用外部运行时。",
            json!({"type":"object","properties":{"path":{"type":"string","description":"相对软件目录；空字符串表示根目录"}},"additionalProperties":false}),
            "core:workspace",
            move |arguments| files.list(arguments["path"].as_str().unwrap_or("")),
        )
        .with_label("查看软件目录")
        .with_execution(read.clone()),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.read",
            "读取软件目录内的一个文本文件。路径必须相对软件目录。",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| files.read(arguments["path"].as_str().unwrap_or("")),
        )
        .with_label("读取软件目录文件")
        .with_execution(read),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.write",
            "在软件目录内创建或替换一个文本文件。已有文件必须提交 workspace.read 返回的 sha256。不要用它修改宿主软件的配置，那些只能走对应的桥。",
            json!({"type":"object","properties":{
                "path":{"type":"string"},
                "content":{"type":"string"},
                "expectedSha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}
            },"required":["path","content"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| {
                files.write(
                    arguments["path"].as_str().unwrap_or(""),
                    arguments["content"].as_str().unwrap_or(""),
                    arguments["expectedSha256"].as_str(),
                )
            },
        )
        .with_label("写入软件目录文件")
        .with_execution(ToolExecution {
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::Standard,
            concurrency_safe: false,
            deferred: false,
            always_load: true,
            search_hint: Some("修改软件目录文件".into()),
            scope: ScopeKind::Fields,
            scope_target: Some("path".into()),
            scope_reader: Some("workspace.read".into()),
            model_usage: crate::extension::ModelUsage::None,
            ..ToolExecution::default()
        }),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.delete",
            "删除软件目录内的文件或目录。不要用它删除宿主软件的配置。",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| files.delete(arguments["path"].as_str().unwrap_or("")),
        )
        .with_label("删除软件目录文件")
        .with_execution(ToolExecution {
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::High,
            concurrency_safe: false,
            deferred: false,
            always_load: true,
            search_hint: Some("删除软件目录文件".into()),
            scope: ScopeKind::Delete,
            scope_target: Some("path".into()),
            model_usage: crate::extension::ModelUsage::None,
            ..ToolExecution::default()
        }),
    )?;
    registry.register_shared(Arc::new(WorkspaceShellTool { workspace }))?;
    Ok(())
}

struct WorkspaceShellTool {
    workspace: Arc<Workspace>,
}

impl Tool for WorkspaceShellTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "workspace.shell".into(),
            label: "执行 PowerShell".into(),
            description: "在软件目录内执行 PowerShell。用户没有 Node、Python、Git 或其他开发环境，本机命令只走这一条；不要让用户安装中间件。工作目录固定为软件目录，不能访问目录外的路径，也不能改宿主软件配置。".into(),
            input_schema: json!({"type":"object","properties":{"command":{"type":"string","description":"一段 PowerShell。工作目录已是软件目录，用相对路径。"}},"required":["command"],"additionalProperties":false}),
            output_schema: None,
            source: "core:workspace".into(),
            provider_version: None,
            execution: ToolExecution {
                effect: ToolEffect::LocalWrite,
                risk: RiskLevel::High,
                concurrency_safe: false,
                deferred: false,
                always_load: true,
                timeout_ms: 120_000,
                max_result_chars: 96_000,
                search_hint: Some("在软件目录执行 PowerShell".into()),
                cancellation: crate::extension::CancellationMode::Reliable,
                verification: crate::extension::VerificationMode::None,
                compensation: crate::extension::CompensationMode::None,
                model_usage: crate::extension::ModelUsage::None,
                ..ToolExecution::default()
            },
        }
    }

    fn call(&self, arguments: &Value) -> Result<Value> {
        self.workspace.shell_sync(
            arguments["command"].as_str().unwrap_or(""),
            Duration::from_millis(self.definition().execution.timeout_ms),
        )
    }

    fn call_async(
        self: Arc<Self>,
        arguments: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>>
    where
        Self: 'static,
    {
        Box::pin(async move {
            let timeout = Duration::from_millis(self.definition().execution.timeout_ms);
            self.workspace
                .shell(arguments["command"].as_str().unwrap_or(""), timeout, cancel)
                .await
        })
    }
}

fn resolve_inside(root: &Path, relative: &str, denied: &[PathBuf]) -> Result<PathBuf> {
    let cleaned = relative.replace('\\', "/");
    if cleaned.starts_with('/') || cleaned.contains(':') {
        return Err(Error::Tool("路径必须是相对于软件目录的相对路径".into()));
    }
    let mut path = root.to_path_buf();
    for segment in cleaned.split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(Error::Tool("路径不能越出软件目录".into())),
            segment => path.push(segment),
        }
    }
    if let Ok(real) = path.canonicalize() {
        let root = canonical(root);
        if !is_within(&root, &real) {
            return Err(Error::Tool("路径不能越出软件目录".into()));
        }
        reject_denied(&real, denied)?;
        return Ok(real);
    }
    reject_denied(&path, denied)?;
    if !is_within(root, &path) {
        return Err(Error::Tool("路径不能越出软件目录".into()));
    }
    Ok(path)
}

fn reject_denied(path: &Path, denied: &[PathBuf]) -> Result<()> {
    for deny in denied {
        let deny = canonical(deny);
        if is_within(&deny, path) {
            return Err(Error::Tool(
                "宿主配置只能通过桥的设置接口修改，不能用软件目录命令改".into(),
            ));
        }
    }
    Ok(())
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_within(root: &Path, path: &Path) -> bool {
    let root = normalize_compare(root);
    let path = normalize_compare(path);
    path == root || path.starts_with(&format!("{root}\\")) || path.starts_with(&format!("{root}/"))
}

fn normalize_compare(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    let value = value.trim_end_matches('\\').to_string();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn truncate_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= SHELL_OUTPUT_LIMIT {
        text.into_owned()
    } else {
        format!(
            "{}…(输出已截断)",
            text.chars().take(SHELL_OUTPUT_LIMIT).collect::<String>()
        )
    }
}

fn encode_powershell(script: &str) -> String {
    STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<u8>>(),
    )
}

fn powershell_executable() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let exe = PathBuf::from(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if exe.is_file() {
            return Ok(exe);
        }
        Err(Error::Tool("未找到 Windows PowerShell".into()))
    }
    #[cfg(not(windows))]
    {
        Err(Error::Tool("PowerShell 仅在 Windows 上可用".into()))
    }
}

fn host_script(payload_b64: &str) -> String {
    format!(
        r#"
$ErrorActionPreference = 'Stop'
# 默认输出编码是 ANSI，中文错误和文件内容到调用方就成了乱码。
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$ctx = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{payload_b64}')) | ConvertFrom-Json
$Workspace = [IO.Path]::GetFullPath($ctx.workspace)
$Deny = @()
if ($ctx.deny) {{ $Deny = @($ctx.deny) }}
function Test-SleepyPath([string]$Path) {{
  if ([string]::IsNullOrWhiteSpace($Path)) {{ $Path = (Get-Location).Path }}
  if (-not [IO.Path]::IsPathRooted($Path)) {{ $Path = [IO.Path]::Combine((Get-Location).Path, $Path) }}
  $full = [IO.Path]::GetFullPath($Path)
  $root = $Workspace.TrimEnd('\','/') + '\'
  if (-not ($full.Equals($Workspace, 'OrdinalIgnoreCase') -or $full.StartsWith($root, 'OrdinalIgnoreCase'))) {{
    throw "路径不能越出软件目录：$full"
  }}
  foreach ($item in $Deny) {{
    if ([string]::IsNullOrWhiteSpace($item)) {{ continue }}
    $deny = [IO.Path]::GetFullPath($item).TrimEnd('\','/')
    $prefix = $deny + '\'
    if ($full.Equals($deny, 'OrdinalIgnoreCase') -or $full.StartsWith($prefix, 'OrdinalIgnoreCase')) {{
      throw "宿主配置只能通过桥的设置接口修改，不能用软件目录命令改：$full"
    }}
  }}
  return $full
}}
function Assert-SleepyBound($bound) {{
  foreach ($name in @('Path','LiteralPath','Destination','Target','FilePath','Root','PSPath')) {{
    if ($bound.ContainsKey($name)) {{
      $value = $bound[$name]
      if ($value -is [System.Array]) {{ foreach ($item in $value) {{ [void](Test-SleepyPath ([string]$item)) }} }}
      elseif ($null -ne $value) {{ [void](Test-SleepyPath ([string]$value)) }}
    }}
  }}
}}
Get-PSDrive -PSProvider FileSystem -ErrorAction SilentlyContinue | ForEach-Object {{
  try {{ Remove-PSDrive -Name $_.Name -Force -ErrorAction SilentlyContinue }} catch {{}}
}}
New-PSDrive -Name SD -PSProvider FileSystem -Root $Workspace -Scope Global | Out-Null
Set-Location -LiteralPath $Workspace
$global:SleepyProxies = @{{}}
foreach ($name in @('Set-Location','Push-Location','Get-ChildItem','Get-Item','Get-Content','Set-Content','Add-Content','Out-File','Remove-Item','Rename-Item','Copy-Item','Move-Item','New-Item','Clear-Content','New-PSDrive','Remove-PSDrive','Start-Process','Invoke-Item','Set-Acl','Set-ItemProperty')) {{
  $original = Microsoft.PowerShell.Core\Get-Command -Name $name -CommandType Cmdlet -ErrorAction Stop
  $text = [System.Management.Automation.ProxyCommand]::Create((New-Object System.Management.Automation.CommandMetadata($original)))
  # ProxyCommand 生成的 begin 与左花括号分处两行，锚点不带换行就匹配不到。
  $text = $text.Replace("begin`r`n{{", "begin`r`n{{`r`n    Assert-SleepyBound `$PSBoundParameters")
  Microsoft.PowerShell.Management\Set-Item -Path "function:global:$name" -Value ([ScriptBlock]::Create($text))
  $global:SleepyProxies[$name] = Microsoft.PowerShell.Core\Get-Command -Name $name -CommandType Function
}}
$ExecutionContext.SessionState.InvokeCommand.PreCommandLookupAction = {{
  param($CommandName, $EventArgs)
  $short = ($CommandName -split '\\')[-1]
  $blocked = @('cmd','cmd.exe','wscript','wscript.exe','cscript','cscript.exe','mshta','mshta.exe','reg','reg.exe','diskpart','diskpart.exe','format','format.exe','powershell','powershell.exe','pwsh','pwsh.exe','bash','bash.exe','sh','python','python.exe','py.exe','node','node.exe','npm','npm.exe','git','git.exe','dotnet','dotnet.exe')
  if ($blocked -contains $short.ToLowerInvariant()) {{
    throw '本机没有开发环境，只能使用当前 PowerShell 会话内的命令。'
  }}
  # 代理函数靠带模块限定的名字取回真实 cmdlet，重定向会把代理指回它自己。
  if ($CommandName -notlike '*\*' -and $global:SleepyProxies.ContainsKey($short)) {{
    $EventArgs.Command = $global:SleepyProxies[$short]
    $EventArgs.StopSearch = $true
  }}
}}
$ExecutionContext.SessionState.InvokeCommand.PostCommandLookupAction = {{
  param($CommandName, $EventArgs)
  if ($EventArgs.Command -is [System.Management.Automation.ApplicationInfo]) {{
    [void](Test-SleepyPath $EventArgs.Command.Path)
  }}
}}
$ExecutionContext.SessionState.LanguageMode = 'ConstrainedLanguage'
Set-Location -LiteralPath $Workspace
Invoke-Expression $ctx.command
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(root: &Path) -> Workspace {
        Workspace {
            root: root.to_path_buf(),
            bridge: None,
            denied: Vec::new(),
        }
    }

    #[test]
    fn software_root_uses_the_directory_above_user_config() {
        assert_eq!(
            software_root(Path::new("/opt/sleepy/user/config.json")),
            PathBuf::from("/opt/sleepy")
        );
        assert_eq!(
            software_root(Path::new("/opt/sleepy/config.json")),
            PathBuf::from("/opt/sleepy")
        );
    }

    #[test]
    fn resolve_rejects_escapes_and_absolute_paths() {
        let root = Path::new(r"/opt/sleepy");
        for bad in [
            r"..\..\Windows\System32",
            "../config.json",
            "a/../../b",
            "..",
            r"C:\Windows\System32",
            "/etc/passwd",
            r"\\server\share",
        ] {
            assert!(resolve_inside(root, bad, &[]).is_err(), "未拒绝：{bad}");
        }
        assert_eq!(
            resolve_inside(root, "user/notes.txt", &[]).unwrap(),
            root.join("user").join("notes.txt")
        );
    }

    #[test]
    fn resolve_rejects_host_config_directories() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("BetterGI").join("User");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("config.json"), "{}").unwrap();
        let denied = vec![nested.clone()];
        assert!(resolve_inside(root.path(), "BetterGI/User/config.json", &denied).is_err());
        assert!(resolve_inside(root.path(), "BetterGI/User", &denied).is_err());
        fs::create_dir_all(root.path().join("user")).unwrap();
        assert!(resolve_inside(root.path(), "user", &denied).is_ok());
    }

    #[test]
    fn file_tools_stay_inside_the_software_directory() {
        let root = tempfile::tempdir().unwrap();
        let ws = workspace(root.path());
        ws.write("notes/hello.txt", "你好", None).unwrap();
        let listed = ws.list("notes").unwrap();
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["entries"][0]["name"], "hello.txt");
        let read = ws.read("notes/hello.txt").unwrap();
        assert_eq!(read["text"], "你好");
        let sha = read["sha256"].as_str().unwrap().to_string();
        ws.write("notes/hello.txt", "第二版", Some(&sha)).unwrap();
        assert_eq!(ws.read("notes/hello.txt").unwrap()["text"], "第二版");
        ws.delete("notes/hello.txt").unwrap();
        assert!(ws.read("notes/hello.txt").is_err());
        assert!(ws.write("../escape.txt", "no", None).is_err());
    }

    #[test]
    fn replace_without_hash_does_not_write() {
        let root = tempfile::tempdir().unwrap();
        let ws = workspace(root.path());
        ws.write("a.txt", "keep", None).unwrap();
        assert!(ws.write("a.txt", "lose", None).is_err());
        assert_eq!(ws.read("a.txt").unwrap()["text"], "keep");
    }

    #[test]
    fn host_script_carries_the_command_and_deny_list() {
        let payload = STANDARD.encode(
            json!({
                "workspace": r"C:\Sleepy-Doll",
                "deny": [r"D:\BetterGI\User"],
                "command": "Get-ChildItem",
            })
            .to_string(),
        );
        let script = host_script(&payload);
        assert!(script.contains(&payload));
        assert!(script.contains("ConstrainedLanguage"));
        assert!(script.contains("本机没有开发环境"));
        assert!(script.contains("宿主配置只能通过桥的设置接口修改"));
    }

    #[test]
    fn powershell_is_unavailable_outside_windows() {
        #[cfg(not(windows))]
        {
            let root = tempfile::tempdir().unwrap();
            let error = workspace(root.path())
                .shell_sync("Get-ChildItem", Duration::from_secs(2))
                .unwrap_err();
            assert!(error.to_string().contains("Windows"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn powershell_runs_inside_the_software_directory_and_rejects_host_config() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("marker.txt"), "in-root").unwrap();
        let mut ws = workspace(root.path());
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "host-config").unwrap();
        ws.denied = vec![outside.path().to_path_buf()];

        let inside = ws
            .shell_sync(
                "Get-Content -LiteralPath .\\marker.txt",
                Duration::from_secs(20),
            )
            .unwrap();
        assert_eq!(inside["exitCode"], 0);
        assert!(inside["stdout"].as_str().unwrap().contains("in-root"));

        let escaped = ws
            .shell_sync(
                &format!(
                    "Get-Content -LiteralPath '{}'",
                    outside.path().join("secret.txt").display()
                ),
                Duration::from_secs(20),
            )
            .unwrap();
        assert_ne!(escaped["exitCode"], 0);
        let err = format!("{}{}", escaped["stdout"], escaped["stderr"]);
        assert!(
            err.contains("宿主配置") || err.contains("软件目录"),
            "{err}"
        );
    }
}
