//! 通用本机文件与命令工具：路径、读取、搜索与 PowerShell 执行。
//! 文件逻辑在 `workspace_io`，这里负责工具注册、shell 与环境信息。

#[path = "workspace_io.rs"]
mod workspace_io;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
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

const WRITE_LIMIT: usize = 2 * 1024 * 1024;
const COMMAND_LIMIT: usize = 32 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const MAX_TIMEOUT_MS: u64 = 3_600_000;
const DEFAULT_OUTPUT_LIMIT: usize = 256 * 1024;
const MAX_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

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
    host_info: Arc<OnceLock<Value>>,
}

impl Workspace {
    pub fn from_config(config_path: &Path, bridge: Option<Arc<BgiClient>>) -> Self {
        Self {
            root: software_root(config_path),
            bridge,
            host_info: Arc::new(OnceLock::new()),
        }
    }

    /// 仅 environment 工具读一次宿主信息并缓存；read/list/search 不做桥 RPC。
    fn host_directories(&self) -> Value {
        let cached = self.host_info.get_or_init(|| {
            self.bridge
                .as_ref()
                .and_then(|bridge| bridge.host().ok())
                .unwrap_or_else(|| json!({}))
        });
        let mut dirs = json!({});
        for key in ["installPath", "userPath", "logPath"] {
            if let Some(path) = cached[key].as_str().filter(|path| !path.is_empty()) {
                dirs[key] = json!(path);
            }
        }
        dirs
    }

    fn write(&self, arguments: &Value) -> Result<Value> {
        let content = arguments["content"].as_str().unwrap_or("");
        if content.len() > WRITE_LIMIT {
            return Err(Error::Tool("写入内容超过 2 MiB 上限".into()));
        }
        let raw = arguments["path"].as_str().unwrap_or("");
        let path = workspace_io::resolve(&self.root, raw)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let expected = arguments["expectedSha256"].as_str();
        if path.exists() {
            let current = fs::read(&path)?;
            let actual = sha256(&current);
            let expected = expected.ok_or_else(|| {
                Error::Tool("目标已存在；必须先读取并提交 expectedSha256，未写入".into())
            })?;
            if actual != expected {
                return Err(Error::Stale(
                    "目标文件在读取后发生变化；未覆盖较新的内容，请重新读取".into(),
                ));
            }
        } else if expected.is_some() {
            return Err(Error::Stale(
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
            use std::io::Write as _;
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
            "path": display_path(&self.root, &path),
            "absolutePath": path.to_string_lossy(),
            "sha256": sha256(content.as_bytes()),
            "bytes": content.len(),
        }))
    }

    fn delete(&self, arguments: &Value) -> Result<Value> {
        let path = workspace_io::resolve(&self.root, arguments["path"].as_str().unwrap_or(""))?;
        if path.parent().is_none_or(|parent| parent == path) {
            return Err(Error::Tool("拒绝删除盘符根目录".into()));
        }
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
            "path": display_path(&self.root, &path),
            "absolutePath": path.to_string_lossy(),
            "deleted": true,
            "directory": directory,
        }))
    }

    fn shell_sync(&self, arguments: &Value) -> Result<Value> {
        crate::runtime::executor().block_on(self.shell(arguments, CancellationToken::new()))
    }

    async fn shell(&self, arguments: &Value, cancel: CancellationToken) -> Result<Value> {
        let command = arguments["command"].as_str().unwrap_or("").trim();
        if command.is_empty() {
            return Err(Error::Tool("PowerShell 命令为空".into()));
        }
        if command.len() > COMMAND_LIMIT {
            return Err(Error::Tool("PowerShell 命令超过 32 KiB 上限".into()));
        }
        let timeout_ms = arguments["timeoutMs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(1_000, MAX_TIMEOUT_MS);
        let output_limit = arguments["maxOutputChars"]
            .as_u64()
            .unwrap_or(DEFAULT_OUTPUT_LIMIT as u64)
            .clamp(1024, MAX_OUTPUT_LIMIT as u64) as usize;
        let cwd = match arguments["cwd"]
            .as_str()
            .filter(|cwd| !cwd.trim().is_empty())
        {
            Some(raw) => {
                let resolved = workspace_io::resolve(&self.root, raw)?;
                if !resolved.is_dir() {
                    return Err(Error::Tool(format!(
                        "cwd 不是有效目录：{}",
                        resolved.to_string_lossy()
                    )));
                }
                resolved
            }
            None => self.root.clone(),
        };

        let exe = powershell_executable()?;
        #[cfg(test)]
        eprintln!("workspace.shell runtime: {}", exe.display());
        let byte_limit = output_limit.saturating_mul(4);
        let payload = json!({
            "workspace": cwd.to_string_lossy(),
            "command": command,
        });
        let script = host_script(&STANDARD.encode(payload.to_string()));

        let mut process = tokio::process::Command::new(&exe);
        process
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                // 包装器保持 ASCII 单行；命令本体在 base64 JSON 中保留
                // 引号、中文与换行，避免 Windows 命令行再解析多行包装器。
                "-Command",
                &script,
            ])
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        crate::runtime::host::process::isolate_environment(&mut process);
        restore_common_environment(&mut process, &self.root)?;
        crate::runtime::host::process::hide_console(&mut process);
        let mut child = process.spawn()?;
        let constraint = crate::runtime::host::process::constrain_process(&child)?;
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let stdout_task = tokio::spawn(drain_bounded(stdout.take(), byte_limit));
        let stderr_task = tokio::spawn(drain_bounded(stderr.take(), byte_limit));

        // 先丢 Job 让它连带终止所有派生进程，再收 PowerShell 本体；派生进程若仍
        // 持有 stdout 句柄，reader 会等到管道关闭才退出。
        let wait = tokio::time::timeout(Duration::from_millis(timeout_ms), async {
            tokio::select! {
                _ = cancel.cancelled() => Err(Error::Cancelled),
                status = child.wait() => status.map_err(Error::from),
            }
        });
        let status = match wait.await {
            Ok(Ok(status)) => status,
            Ok(Err(Error::Cancelled)) => {
                drop(constraint);
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(Error::Cancelled);
            }
            Ok(Err(error)) => {
                drop(constraint);
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(error);
            }
            Err(_) => {
                drop(constraint);
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(Error::Tool(format!(
                    "PowerShell 执行超时（{timeout_ms} ms）"
                )));
            }
        };
        drop(constraint);
        let (stdout_bytes, stdout_overflow) = stdout_task.await.unwrap_or_default();
        let (stderr_bytes, stderr_overflow) = stderr_task.await.unwrap_or_default();
        let (stdout, stdout_cut) = truncate_output(&stdout_bytes, output_limit);
        let (stderr, stderr_cut) = truncate_output(&stderr_bytes, output_limit);
        Ok(json!({
            "cwd": cwd.to_string_lossy(),
            "exitCode": status.code().unwrap_or(-1),
            "stdout": stdout,
            "stdoutTruncated": stdout_overflow || stdout_cut,
            "stderr": stderr,
            "stderrTruncated": stderr_overflow || stderr_cut,
        }))
    }
}

/// 有界读取：最多保留 keep_bytes 字节，超量继续读完管道但丢弃，避免
/// read_to_end 对海量输出无限缓冲；返回 (保留字节, 是否溢出)。
async fn drain_bounded<R>(mut pipe: Option<R>, keep_bytes: usize) -> (Vec<u8>, bool)
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut kept = Vec::new();
    let mut overflow = false;
    let Some(pipe) = pipe.as_mut() else {
        return (kept, overflow);
    };
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if kept.len() < keep_bytes {
                    let room = keep_bytes - kept.len();
                    let keep = read.min(room);
                    kept.extend_from_slice(&chunk[..keep]);
                    if keep < read {
                        overflow = true;
                    }
                } else {
                    overflow = true;
                }
            }
        }
    }
    (kept, overflow)
}

pub fn register_tools(registry: &mut ToolRegistry, workspace: Arc<Workspace>) -> Result<()> {
    let read = ToolExecution {
        deferred: false,
        always_load: true,
        max_result_chars: 1_000_000,
        search_hint: Some("读取文件".into()),
        ..ToolExecution::read_only()
    };
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.list",
            "列出指定目录的文件和目录，可选递归与通配筛选。path 支持绝对路径或相对默认目录，空为默认根。",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"绝对或相对路径；空字符串表示默认根目录"},
                "recursive":{"type":"boolean","description":"是否递归列出子目录，默认 false"},
                "include":{"type":["string","array"],"description":"文件名通配筛选，如 *.log，可多个"},
                "limit":{"type":"integer","description":"最多返回条数"}
            },"additionalProperties":false}),
            "core:workspace",
            move |arguments| workspace_io::list(&files.root, arguments),
        )
        .with_label("查看目录")
        .with_execution(read.clone()),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.read",
            "读取一个文本文件。默认按 524288 字符预算返回（maxChars 可调最大 8388608），支持 startLine/maxLines/tailLines 分页；返回总行数、nextLine、截断标记与全文件 sha256。分页可能漏看内容时，改用 workspace.search 做全文件统计。",
            json!({"type":"object","properties":{
                "path":{"type":"string"},
                "startLine":{"type":"integer","minimum":1,"description":"从第几行开始，1 起算"},
                "maxLines":{"type":"integer","minimum":1},
                "tailLines":{"type":"integer","minimum":1,"description":"只读末尾 N 行，优先于 startLine"},
                "maxChars":{"type":"integer","minimum":1,"maximum":8388608,"description":"文本字符上限，默认 524288，最大 8388608"}
            },"required":["path"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| workspace_io::read(&files.root, arguments),
        )
        .with_label("读取文件")
        .with_execution(read.clone()),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.read_many",
            "批量读取最多 64 个文本文件，单次总文本默认 524288 字符、最大 8388608；单个文件失败只记录错误，不影响其余文件。",
            json!({"type":"object","properties":{
                "paths":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":64},
                "maxChars":{"type":"integer","minimum":1,"maximum":8388608,"description":"单文件文本字符上限"},
                "textBudget":{"type":"integer","minimum":1,"maximum":8388608,"description":"单次批量总文本字符预算，默认 524288，最大 8388608"}
            },"required":["paths"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| workspace_io::read_many(&files.root, arguments),
        )
        .with_label("批量读取文件")
        .with_execution(read.clone()),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.search",
            "在文件或目录中做正则搜索与统计：pattern 或 patterns（最多 64 个）至少其一；可选 include 通配、recursive（默认 true）、caseSensitive（默认 false）、capture 命名捕获分组计数、sumCapture 命名捕获数字求和、sampleLimit（默认 20，最大 2000）。全文件扫描计数，样本才受 sampleLimit 限制。",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"单个文件或目录，与 paths 至少提供其一"},
                "paths":{"type":"array","items":{"type":"string"},"minItems":1,"description":"文件或目录列表，与 path 至少提供其一"},
                "include":{"type":["string","array"],"items":{"type":"string"},"description":"文件名通配筛选，目录递归时生效"},
                "recursive":{"type":"boolean","description":"目录是否递归，默认 true"},
                "pattern":{"type":"string","description":"单个正则"},
                "patterns":{"type":"array","maxItems":64,"items":{"type":"object","properties":{
                    "name":{"type":"string"},
                    "pattern":{"type":"string"}
                },"required":["name","pattern"],"additionalProperties":false}},
                "caseSensitive":{"type":"boolean","description":"默认 false"},
                "capture":{"type":"string","description":"按该命名捕获的值分组计数"},
                "sumCapture":{"type":"string","description":"对该命名捕获的数字求和"},
                "sampleLimit":{"type":"integer","minimum":0,"maximum":2000}
            },"allOf":[
                {"anyOf":[{"required":["path"]},{"required":["paths"]}]},
                {"anyOf":[{"required":["pattern"]},{"required":["patterns"]}]}
            ],"additionalProperties":false}),
            "core:workspace",
            move |arguments| workspace_io::search(&files.root, arguments),
        )
        .with_label("搜索文件内容")
        .with_execution(read),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.write",
            "创建或替换一个文本文件。已有文件必须提交本轮 workspace.read 返回的 sha256；校验不一致会以版本冲突返回，重新读取后再改。",
            json!({"type":"object","properties":{
                "path":{"type":"string"},
                "content":{"type":"string"},
                "expectedSha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}
            },"required":["path","content"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| files.write(arguments),
        )
        .with_label("写入文件")
        .with_execution(ToolExecution {
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::Standard,
            concurrency_safe: false,
            deferred: false,
            always_load: true,
            search_hint: Some("修改文件".into()),
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
            "删除文件或目录。拒绝删除盘符根目录。",
            json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
            "core:workspace",
            move |arguments| files.delete(arguments),
        )
        .with_label("删除文件")
        .with_execution(ToolExecution {
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::High,
            concurrency_safe: false,
            deferred: false,
            always_load: true,
            search_hint: Some("删除文件".into()),
            scope: ScopeKind::Delete,
            scope_target: Some("path".into()),
            model_usage: crate::extension::ModelUsage::None,
            ..ToolExecution::default()
        }),
    )?;
    let files = workspace.clone();
    registry.register(
        FunctionTool::new(
            "workspace.environment",
            "只读环境信息：默认工作目录、软件根目录、PATH 上可用的常用可执行文件（PowerShell/node/python/py/git/jq 等）以及已连接宿主的安装/User/日志目录。",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            "core:workspace",
            move |_| {
                Ok(json!({
                    "defaultCwd": files.root.to_string_lossy(),
                    "softwareRoot": files.root.to_string_lossy(),
                    "userDirectory": user_directory(),
                    "executables": common_executables(),
                    "hostDirectories": files.host_directories(),
                }))
            },
        )
        .with_label("查看环境信息")
        .with_execution(ToolExecution {
            deferred: false,
            always_load: true,
            max_result_chars: 16_000,
            search_hint: Some("查看环境信息".into()),
            ..ToolExecution::read_only()
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
            description: "执行一段 PowerShell。可选 cwd（默认软件目录）、timeoutMs（默认 120000，最大 3600000）与 maxOutputChars（默认 262144，最大 8388608）。".into(),
            input_schema: json!({"type":"object","properties":{
                "command":{"type":"string","description":"一段 PowerShell 脚本"},
                "cwd":{"type":"string","description":"工作目录，绝对或相对路径，默认软件目录"},
                "timeoutMs":{"type":"integer","minimum":1000,"maximum":3600000},
                "maxOutputChars":{"type":"integer","minimum":1024,"maximum":8388608}
            },"required":["command"],"additionalProperties":false}),
            output_schema: None,
            source: "core:workspace".into(),
            provider_version: None,
            execution: ToolExecution {
                effect: ToolEffect::LocalWrite,
                risk: RiskLevel::High,
                concurrency_safe: false,
                deferred: false,
                always_load: true,
                // 外层运行时按这个预算包住调用（runtime/mod.rs 的写路径 timeout），
                // 因此取最大值，让 shell 自身的 timeoutMs 决定真实超时。
                timeout_ms: MAX_TIMEOUT_MS,
                max_result_chars: 1_000_000,
                search_hint: Some("执行 PowerShell".into()),
                cancellation: crate::extension::CancellationMode::Reliable,
                verification: crate::extension::VerificationMode::None,
                compensation: crate::extension::CompensationMode::None,
                model_usage: crate::extension::ModelUsage::None,
                ..ToolExecution::default()
            },
        }
    }

    fn call(&self, arguments: &Value) -> Result<Value> {
        self.workspace.shell_sync(arguments)
    }

    fn call_async(
        self: Arc<Self>,
        arguments: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>>
    where
        Self: 'static,
    {
        let workspace = self.workspace.clone();
        Box::pin(async move { workspace.shell(&arguments, cancel).await })
    }
}

/// isolate_environment 只保留 PATH/SystemRoot/WINDIR/TEMP/TMP，这里补回常用
/// 系统变量，并把 TEMP/TMP 固定到软件标准缓存目录，避免落系统 Temp。
fn restore_common_environment(process: &mut tokio::process::Command, root: &Path) -> Result<()> {
    for key in [
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "COMSPEC",
        "PATHEXT",
        "HOME",
        "HOMEDRIVE",
        "HOMEPATH",
        "USERNAME",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "NUMBER_OF_PROCESSORS",
        "PROCESSOR_ARCHITECTURE",
        "PSModulePath",
    ] {
        if let Some(value) = std::env::var_os(key) {
            process.env(key, value);
        }
    }
    let cache = root.join(".cache").join("shell");
    fs::create_dir_all(&cache).map_err(|error| {
        Error::Tool(format!(
            "创建执行缓存目录失败（{}）：{error}；不会回退到系统 Temp",
            cache.to_string_lossy()
        ))
    })?;
    process.env("TEMP", &cache);
    process.env("TMP", &cache);
    Ok(())
}

fn user_directory() -> String {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn common_executables() -> Value {
    let mut executables = json!({});
    for name in [
        "powershell",
        "node",
        "npm",
        "python",
        "py",
        "git",
        "jq",
        "cmd",
    ] {
        if let Some(path) = find_in_path(name) {
            executables[name] = json!(path);
        }
    }
    executables
}

fn find_in_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let extensions: Vec<String> = std::env::var("PATHEXT")
        .map(|value| {
            value
                .split(';')
                .filter(|item| !item.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_else(|_| vec![".exe".into()]);
    for directory in std::env::split_paths(&path) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        let direct = directory.join(name);
        if direct.is_file() {
            return Some(direct.to_string_lossy().into_owned());
        }
        for extension in &extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn display_path(root: &Path, path: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(root) {
        relative.to_string_lossy().replace('\\', "/")
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn truncate_output(bytes: &[u8], limit: usize) -> (String, bool) {
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= limit {
        (text.into_owned(), false)
    } else {
        (text.chars().take(limit).collect::<String>(), true)
    }
}

fn powershell_executable() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        // Prefer the installed PowerShell 7 runtime. Actions runs its Windows
        // steps with it as well; Windows PowerShell remains the fallback.
        for key in ["ProgramFiles", "ProgramW6432"] {
            if let Some(root) = std::env::var_os(key) {
                let exe = PathBuf::from(root)
                    .join("PowerShell")
                    .join("7")
                    .join("pwsh.exe");
                if exe.is_file() {
                    return Ok(exe);
                }
            }
        }
        if let Some(exe) = find_in_path("pwsh") {
            let exe = PathBuf::from(exe);
            if exe
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                return Ok(exe);
            }
        }
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
        "$ErrorActionPreference = 'Stop'; \
         [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); \
         $OutputEncoding = [Text.UTF8Encoding]::new($false); \
         $ctx = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{payload_b64}')) | ConvertFrom-Json; \
         Set-Location -LiteralPath $ctx.workspace; \
         $LASTEXITCODE = $null; \
         & ([scriptblock]::Create($ctx.command)); \
         $commandSucceeded = $?; \
         if ($null -ne $LASTEXITCODE) {{ exit $LASTEXITCODE }}; \
         if (-not $commandSucceeded) {{ exit 1 }}; exit 0"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试夹具统一放 target/.tmp/general-tools；测试内不做任何清理，
    /// 由 root 结束时统一走 Remove-Directory.ps1 专用通道删除。
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(".tmp")
                .join("general-tools")
                .join(name);
            fs::create_dir_all(dir.join("user")).expect("创建夹具目录");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    fn registry(root: &Path) -> ToolRegistry {
        let mut registry = ToolRegistry::default();
        let workspace = Workspace::from_config(&root.join("user/config.json"), None);
        register_tools(&mut registry, Arc::new(workspace)).expect("注册 workspace 工具");
        registry
    }

    fn call(registry: &ToolRegistry, name: &str, arguments: Value) -> Value {
        registry.call(name, &arguments)
    }

    #[test]
    fn write_requires_expected_sha_for_existing_target() {
        let scratch = Scratch::new("write-hash");
        let registry = registry(scratch.path());
        let target = scratch.path().join("notes.txt");

        let created = call(
            &registry,
            "workspace.write",
            json!({"path": target.to_string_lossy(), "content": "第一版"}),
        );
        assert!(created["ok"].as_bool().unwrap(), "{created}");

        let missing = call(
            &registry,
            "workspace.write",
            json!({"path": target.to_string_lossy(), "content": "第二版"}),
        );
        assert!(!missing["ok"].as_bool().unwrap(), "{missing}");

        let stale = call(
            &registry,
            "workspace.write",
            json!({"path": target.to_string_lossy(), "content": "第二版",
                   "expectedSha256": "0".repeat(64)}),
        );
        assert!(!stale["ok"].as_bool().unwrap(), "{stale}");
        assert!(
            stale["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("读取后发生变化")),
            "{stale}"
        );
    }

    #[test]
    fn delete_rejects_drive_root() {
        let scratch = Scratch::new("delete-root");
        let registry = registry(scratch.path());

        let rejected = call(&registry, "workspace.delete", json!({"path": "C:\\"}));
        assert!(!rejected["ok"].as_bool().unwrap(), "{rejected}");
        assert!(
            rejected["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("盘符根")),
            "{rejected}"
        );

        let file = scratch.path().join("temp.txt");
        fs::write(&file, "x").expect("写入夹具");
        let removed = call(
            &registry,
            "workspace.delete",
            json!({"path": file.to_string_lossy()}),
        );
        assert!(removed["ok"].as_bool().unwrap(), "{removed}");
        assert!(!file.exists());
    }

    #[test]
    fn search_schema_requires_target_and_pattern() {
        let scratch = Scratch::new("search-schema");
        let registry = registry(scratch.path());

        let no_target = call(&registry, "workspace.search", json!({"pattern": "abc"}));
        assert!(!no_target["ok"].as_bool().unwrap(), "{no_target}");

        let no_pattern = call(
            &registry,
            "workspace.search",
            json!({"path": scratch.path().to_string_lossy()}),
        );
        assert!(!no_pattern["ok"].as_bool().unwrap(), "{no_pattern}");

        let nameless = call(
            &registry,
            "workspace.search",
            json!({"path": scratch.path().to_string_lossy(),
                   "patterns": [{"pattern": "abc"}]}),
        );
        assert!(!nameless["ok"].as_bool().unwrap(), "{nameless}");
    }

    #[test]
    fn environment_reports_roots_without_bridge() {
        let scratch = Scratch::new("environment");
        let registry = registry(scratch.path());
        let result = call(&registry, "workspace.environment", json!({}));
        assert!(result["ok"].as_bool().unwrap(), "{result}");
        let value = &result["value"];
        assert_eq!(
            value["softwareRoot"]
                .as_str()
                .map(|root| root.replace('\\', "/")),
            Some(scratch.path().to_string_lossy().replace('\\', "/")),
            "{result}"
        );
        assert!(value["executables"].is_object());
        assert!(value["hostDirectories"].is_object());
    }

    #[cfg(windows)]
    #[test]
    fn shell_runs_in_requested_cwd_and_captures_exit_code() {
        let scratch = Scratch::new("shell-cwd");
        let working = scratch.path().join("work");
        fs::create_dir_all(&working).expect("创建工作目录");
        let registry = registry(scratch.path());

        let ok = call(
            &registry,
            "workspace.shell",
            json!({
                "command": "Write-Output 'hello'; $PWD.Path",
                "cwd": working.to_string_lossy(),
                "timeoutMs": 20000,
                "maxOutputChars": 4096
            }),
        );
        assert!(ok["ok"].as_bool().unwrap(), "{ok}");
        let value = &ok["value"];
        assert_eq!(value["exitCode"].as_i64(), Some(0));
        let stdout = value["stdout"].as_str().unwrap_or_default();
        assert!(stdout.contains("hello"), "{stdout}");
        assert!(stdout.contains("work"), "{stdout}");
        assert_eq!(value["stdoutTruncated"].as_bool(), Some(false));

        let failure = call(
            &registry,
            "workspace.shell",
            json!({"command": "exit 3", "timeoutMs": 20000}),
        );
        assert!(failure["ok"].as_bool().unwrap(), "{failure}");
        assert_eq!(failure["value"]["exitCode"].as_i64(), Some(3));

        let native_failure = call(
            &registry,
            "workspace.shell",
            json!({"command": "cmd.exe /d /c exit 7", "timeoutMs": 20000}),
        );
        assert!(native_failure["ok"].as_bool().unwrap(), "{native_failure}");
        assert_eq!(native_failure["value"]["exitCode"].as_i64(), Some(7));
    }

    #[cfg(windows)]
    #[test]
    fn shell_output_is_bounded_and_marked_truncated() {
        let scratch = Scratch::new("shell-bounded");
        let registry = registry(scratch.path());
        let result = call(
            &registry,
            "workspace.shell",
            json!({
                "command": "foreach ($i in 1..64) { 'x' * 1024 }",
                "timeoutMs": 20000,
                "maxOutputChars": 2048
            }),
        );
        assert!(result["ok"].as_bool().unwrap(), "{result}");
        let value = &result["value"];
        assert_eq!(value["stdoutTruncated"].as_bool(), Some(true));
        let stdout = value["stdout"].as_str().unwrap_or_default();
        assert!(stdout.chars().count() <= 2048, "{}", stdout.chars().count());
    }

    /// 真实命令参数含双引号、中文、多行与反引号时仍按原样执行。
    #[cfg(windows)]
    #[test]
    fn shell_preserves_quotes_chinese_and_multiline_commands() {
        let scratch = Scratch::new("shell-quoting");
        let registry = registry(scratch.path());
        let command =
            "$name = \"快捷任务\"\nWrite-Output \"创建$name`n第二行\"\nWrite-Output '原样\"引号\"'";
        let result = call(
            &registry,
            "workspace.shell",
            json!({
                "command": command,
                "timeoutMs": 20000,
                "maxOutputChars": 4096
            }),
        );
        assert!(result["ok"].as_bool().unwrap(), "{result}");
        let value = &result["value"];
        assert_eq!(value["exitCode"].as_i64(), Some(0));
        let stdout = value["stdout"].as_str().unwrap_or_default();
        assert!(stdout.contains("创建快捷任务"), "{stdout}");
        assert!(stdout.contains("第二行"), "{stdout}");
        assert!(stdout.contains("原样\"引号\""), "{stdout}");
    }
}
