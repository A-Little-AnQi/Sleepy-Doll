//! 与 BetterGI 的接触面：桥的客户端、工具，以及桥进程的生命周期。

pub mod control;
pub(crate) mod resolve;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    config::BridgeConfig,
    error::{Error, Result},
    extension::{FunctionTool, ToolEffect, ToolExecution, ToolRegistry},
};

type BridgeToolFn = Arc<dyn Fn(&Value) -> Result<Value> + Send + Sync>;
/// 名字、界面用名、给模型看的说明、输入 Schema、实现。
type BridgeToolDefinition<'a> = (&'a str, &'a str, &'a str, Value, BridgeToolFn);

/// 单次读取的上限，超出部分截断并告知调用方。
const USER_READ_LIMIT: usize = 128 * 1024;

/// 按顶层字段裁剪一段 JSON 文本，返回裁剪结果与被丢掉的字段名。
///
/// 不是 JSON 对象时返回 `None`，调用方退回全文。
fn select_keys(text: &str, keys: &[&str]) -> Option<(String, Vec<String>)> {
    let Value::Object(map) = serde_json::from_str::<Value>(text).ok()? else {
        return None;
    };
    let kept = keys
        .iter()
        .filter_map(|key| map.get(*key).map(|value| (key.to_string(), value.clone())))
        .collect::<serde_json::Map<_, _>>();
    let dropped = map
        .keys()
        .filter(|key| !keys.contains(&key.as_str()))
        .cloned()
        .collect();
    Some((Value::Object(kept).to_string(), dropped))
}

fn project_json(path: &Path, keys: &[&str]) -> Option<Value> {
    if keys.is_empty()
        || fs::metadata(path).ok()?.len() > USER_READ_LIMIT as u64
        || !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    let (selected, _) = select_keys(&text, keys)?;
    serde_json::from_str(&selected).ok()
}

fn read_small_text(path: &Path, limit: usize) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    (bytes.len() <= limit).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn read_small_json(path: &Path, limit: usize) -> Option<Value> {
    serde_json::from_str(&read_small_text(path, limit)?).ok()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_resource(path: &Path, content: &[u8]) -> Result<()> {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        serde_json::from_slice::<Value>(content)
            .map_err(|error| Error::Tool(format!("JSON 无效，未写入：{error}")))?;
    }
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "resource".into());
    path.with_file_name(format!("{name}.sleepy-doll.{}.bak", Uuid::new_v4()))
}

/// 落盘同目录的临时文件后原子替换目标。
/// 原内容在同一步移到唯一命名的同目录备份。
fn replace_resource(path: &Path, content: &[u8]) -> Result<Option<PathBuf>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| Error::Tool(format!("创建目标目录失败，未写入：{error}")))?;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "resource".into());
    let temporary = path.with_file_name(format!("{name}.{}.tmp", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| Error::Tool(format!("创建临时文件失败，未写入：{error}")))?;
    if let Err(error) = file.write_all(content).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(Error::Tool(format!(
            "写入临时文件失败，未修改目标：{error}"
        )));
    }
    drop(file);

    let backup = path.exists().then(|| backup_path(path));
    let result = replace_file(path, &temporary, backup.as_deref());
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    let actual = fs::read(path).map_err(|error| Error::Tool(format!("写后核验失败：{error}")))?;
    if actual != content {
        return Err(Error::Tool(
            "写后内容核验不一致；请使用返回的备份恢复".into(),
        ));
    }
    Ok(backup)
}

#[cfg(not(target_os = "windows"))]
fn replace_file(path: &Path, temporary: &Path, backup: Option<&Path>) -> Result<()> {
    if let Some(backup) = backup {
        fs::rename(path, backup)?;
        if let Err(error) = fs::rename(temporary, path) {
            let _ = fs::rename(backup, path);
            return Err(error.into());
        }
    } else {
        fs::rename(temporary, path)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_file(path: &Path, temporary: &Path, backup: Option<&Path>) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};

    if backup.is_none() {
        fs::rename(temporary, path)?;
        return Ok(());
    }
    let wide = |value: &Path| {
        value
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let path = wide(path);
    let temporary = wide(temporary);
    let backup = wide(backup.unwrap());
    let success = unsafe {
        ReplaceFileW(
            path.as_ptr(),
            temporary.as_ptr(),
            backup.as_ptr(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Err(Error::Tool(format!(
            "原子替换失败，目标未修改：{}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// 把请求路径解析到 `<BGI 用户目录>` 之下。
///
/// 绝对路径、盘符和 `..` 一律拒绝。目录穿越在这一步挡掉。
fn user_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let cleaned = relative.replace('\\', "/");
    if cleaned.starts_with('/') || cleaned.contains(':') {
        return Err(Error::Tool(
            "路径必须是相对于 BGI 用户目录的相对路径".into(),
        ));
    }
    let mut path = root.to_path_buf();
    for segment in cleaned.split('/') {
        match segment {
            "" | "." => {}
            ".." => return Err(Error::Tool("路径不能越出 BGI 用户目录".into())),
            segment => path.push(segment),
        }
    }
    // 目录内已有的符号链接可能指向外部，落盘前再核一次真实路径。
    if let Ok(real) = path.canonicalize() {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if !real.starts_with(&root) {
            return Err(Error::Tool("路径不能越出 BGI 用户目录".into()));
        }
    }
    Ok(path)
}

#[derive(Clone)]
pub struct BgiClient {
    config: BridgeConfig,
    agent: ureq::Agent,
}

impl BgiClient {
    pub fn new(config: BridgeConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(config.timeout_ms)))
            .build()
            .new_agent();
        Self { config, agent }
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled
    }
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }
    pub fn info(&self) -> Result<Value> {
        self.request("GET", "/bridge/v1/info", None, None)
    }
    pub fn state(&self) -> Result<Value> {
        self.request("GET", "/bridge/v1/state", None, None)
    }
    pub fn host(&self) -> Result<Value> {
        self.request("GET", "/bridge/v1/host", None, None)
    }
    /// BGI 安装目录下的 `User\`。
    pub fn user_root(&self) -> Result<PathBuf> {
        self.host()?["userPath"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| Error::Tool("桥没有返回 BGI 用户目录".into()))
    }
    pub fn catalog(&self, query: &str) -> Result<Value> {
        self.catalog_page(query, None, 0)
    }
    pub fn catalog_page(&self, query: &str, group: Option<&str>, offset: u64) -> Result<Value> {
        self.catalog_page_with_limit(query, group, offset, 50)
    }
    pub fn catalog_page_with_limit(
        &self,
        query: &str,
        group: Option<&str>,
        offset: u64,
        limit: u64,
    ) -> Result<Value> {
        let mut params = url::form_urlencoded::Serializer::new(String::new());
        params
            .append_pair("q", query)
            .append_pair("limit", &limit.clamp(1, 50).to_string())
            .append_pair("offset", &offset.to_string());
        if let Some(group) = group.filter(|group| !group.is_empty()) {
            params.append_pair("group", group);
        }
        self.request(
            "GET",
            &format!("/bridge/v1/catalog?{}", params.finish()),
            None,
            None,
        )
    }
    pub fn describe(&self, method_id: &str) -> Result<Value> {
        let encoded: String = url::form_urlencoded::byte_serialize(method_id.as_bytes()).collect();
        self.request("GET", &format!("/bridge/v1/catalog/{encoded}"), None, None)
    }
    pub fn job(&self, job_id: &str) -> Result<Value> {
        self.request("GET", &format!("/bridge/v1/jobs/{job_id}"), None, None)
    }
    pub fn cancel(&self, job_id: &str) -> Result<Value> {
        self.request(
            "POST",
            &format!("/bridge/v1/jobs/{job_id}/cancel"),
            Some(&json!({})),
            None,
        )
    }
    pub fn invoke(&self, method_id: &str, arguments: &Value) -> Result<Value> {
        let key = Uuid::new_v4().to_string();
        let info = self.info()?;
        self.request("POST", "/bridge/v1/invoke", Some(&json!({"requestId":key,"instanceId":info["instanceId"],"catalogVersion":info["catalogVersion"],"methodId":method_id,"arguments":arguments,"execution":{"onDisconnect":"continue"}})), Some(&key))
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        if !self.config.enabled {
            return Err(Error::Tool("BGI Bridge 未启用".into()));
        }
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);
        let token = self.config.token.as_deref().unwrap_or_default();
        let mut response = if method == "GET" {
            self.agent
                .get(&url)
                .header("authorization", &format!("Bearer {token}"))
                .call()?
        } else {
            let mut request = self
                .agent
                .post(&url)
                .header("authorization", &format!("Bearer {token}"));
            if let Some(key) = idempotency_key {
                request = request.header("idempotency-key", key);
            }
            request.send_json(body.unwrap_or(&Value::Null))?
        };
        Ok(response.body_mut().read_json()?)
    }
}

pub fn register_tools(registry: &mut ToolRegistry, client: Arc<BgiClient>) -> Result<()> {
    let mut definitions: Vec<BridgeToolDefinition<'_>> = vec![
        (
            "bgi.state.get",
            "读取游戏状态",
            "读取一次 BetterGI、截图器、游戏窗口和任务锁状态。仅在准备执行、执行后核验或排障时使用；查询和编辑 User 文件不需要先调它。",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |_| client.state())
            },
        ),
        (
            "bgi.capability.search",
            "检索插件能力",
            "搜索已安装扩展登记的语义能力和资源。它不包含 BetterGI 自身接口，也不用于查用户配置；没有扩展时调用一次空结果即结束。",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.catalog(a["query"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.capability.describe",
            "读取能力定义",
            "读取已由 capability.search 找到的扩展能力契约。BetterGI 原生接口使用 bgi.api.describe。",
            json!({"type":"object","properties":{"methodId":{"type":"string"}},"required":["methodId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.describe(a["methodId"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.capability.invoke",
            "执行游戏操作",
            "执行已读取契约的扩展能力。只使用 capability.search 返回的精确 ID，并继续核验 Job 结果。",
            json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object"}},"required":["methodId","arguments"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| {
                    client.invoke(a["methodId"].as_str().unwrap_or_default(), &a["arguments"])
                })
            },
        ),
        (
            "bgi.job.get",
            "查询执行结果",
            "仅在已有 Job ID、但原调用没有返回终态证据时查询 Job。bgi.api.invoke 已返回 completed/failed/cancelled 时不要重复查询。verification 才表示业务是否已核验。",
            json!({"type":"object","properties":{"jobId":{"type":"string"}},"required":["jobId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.job(a["jobId"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.job.cancel",
            "取消正在执行的任务",
            "请求取消指定 Job。cancellationRequested 不等于宿主任务已经停止，继续查询同一 Job。",
            json!({"type":"object","properties":{"jobId":{"type":"string"}},"required":["jobId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.cancel(a["jobId"].as_str().unwrap_or_default()))
            },
        ),
    ];
    definitions.extend([
        ("bgi.api.search", "检索 BetterGI 接口", "在当前 BetterGI 宿主中发现设置或动作。它不搜索配置组、路线、脚本等用户资源。group 必须来自目录实际返回的分组；用一个业务词查询，一次零结果后检查证据源。", json!({"type":"object","properties":{"query":{"type":"string","description":"一个核心业务词、动作词或精确 methodId；空字符串用于浏览分组"},"group":{"type":"string","description":"可选；使用目录实际返回的分组，例如 settings、command、scheduler、repository"},"offset":{"type":"integer","minimum":0,"description":"仅在响应给出 nextOffset 时继续"},"limit":{"type":"integer","minimum":1,"maximum":50,"default":8,"description":"候选数量；默认 8，只有响应给出 nextOffset 且确有必要时增加"}},"required":["query"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.catalog_page_with_limit(a["query"].as_str().unwrap_or(""),a["group"].as_str(),a["offset"].as_u64().unwrap_or(0),a["limit"].as_u64().unwrap_or(8))) as BridgeToolFn
        }),
        ("bgi.api.describe", "读取接口说明", "读取一个精确 methodId 的用途、参数、前置条件、副作用、结果判定和回退边界。每个候选读一次；callable=false 时以 unavailableReason 为最终结论。", json!({"type":"object","properties":{"methodId":{"type":"string","description":"来自 api.search 的精确 ID"}},"required":["methodId"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.describe(a["methodId"].as_str().unwrap_or(""))) as BridgeToolFn
        }),
        ("bgi.api.read", "读取 BetterGI 状态", "调用刚通过 api.describe 确认的只读接口。用于读取宿主当前设置或诊断；不用于读取 User 文件。arguments 必须满足该接口 inputSchema。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object","description":"无参数接口传空对象"}},"required":["methodId","arguments"],"additionalProperties":false}), {
            Arc::new(move |_: &Value| Err(Error::Tool("该接口必须通过运行时的契约检查调用".into()))) as BridgeToolFn
        }),
        ("bgi.api.invoke", "执行 BetterGI 操作", "调用刚通过 api.describe 确认的写接口。运行时显示审批并跟踪 Job 到终态；返回 outcome/evidence 后直接按契约核验，不重复调用 job.get。不能把 completed 或处理器返回自动当成业务成功。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object","description":"严格满足已读取的 inputSchema"}},"required":["methodId","arguments"],"additionalProperties":false}), {
            Arc::new(move |_: &Value| Err(Error::Tool("该接口必须通过运行时的授权与 Job 跟踪调用".into()))) as BridgeToolFn
        }),
        (
            "bgi.user.list",
            "查看用户资源",
            "列出 BetterGI User 目录下指定位置的一层真实文件和目录。jsonKeys 可在同一次调用中投影每个 JSON 文件的顶层字段，避免逐文件读取；不用于发现宿主接口。",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"相对 User 目录；空字符串表示根目录"},
                "jsonKeys":{"type":"array","maxItems":32,"items":{"type":"string"},"description":"可选；为目录内每个 JSON 返回这些顶层字段"}
            },"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    let path = user_path(&root, a["path"].as_str().unwrap_or(""))?;
                    let json_keys = a["jsonKeys"]
                        .as_array()
                        .map(|keys| {
                            keys.iter()
                                .filter_map(Value::as_str)
                                .take(32)
                                .collect::<Vec<_>>()
                        })
                        .filter(|keys| !keys.is_empty());
                    let read = fs::read_dir(&path)
                        .map_err(|e| Error::Tool(format!("读取目录失败：{e}")))?;
                    let mut entries = Vec::new();
                    for entry in read {
                        let entry = entry.map_err(|e| Error::Tool(format!("读取目录失败：{e}")))?;
                        let file_name = entry.file_name().to_string_lossy().into_owned();
                        if file_name.contains(".sleepy-doll.")
                            && (file_name.ends_with(".bak") || file_name.ends_with(".tmp"))
                        {
                            continue;
                        }
                        let meta = entry.metadata().ok();
                        let mut item = json!({
                            "name": file_name,
                            "directory": meta.as_ref().is_some_and(|m| m.is_dir()),
                            "bytes": meta.as_ref().filter(|m| m.is_file()).map(|m| m.len()),
                        });
                        if let Some(keys) = json_keys.as_ref()
                            && let Some(value) = project_json(&entry.path(), keys)
                        {
                            item["data"] = value;
                        }
                        entries.push(item);
                    }
                    entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                    Ok(json!({"path":path.to_string_lossy(),"count":entries.len(),"entries":entries}))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.read",
            "读取配置文件",
            "读取 BetterGI User 目录中的一个文本文件。JSON 可用 keys 投影所需顶层字段；查询配置组通常读取 name、index、projects，只有修改整个文件时才读取全文。多个独立文件应在同一轮并行读取。",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"由 user.list 或已读文件得到的相对 User 路径"},
                "keys":{"type":"array","items":{"type":"string"},"description":"JSON 顶层字段投影；修改文件时省略以读取全文"}
            },"required":["path"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    let path = user_path(&root, a["path"].as_str().unwrap_or(""))?;
                    let bytes =
                        fs::read(&path).map_err(|e| Error::Tool(format!("读取失败：{e}")))?;
                    let text = String::from_utf8_lossy(&bytes);
                    let selected = a["keys"]
                        .as_array()
                        .map(|keys| keys.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                        .filter(|keys| !keys.is_empty());
                    // 裁剪失败时退回全文。
                    if let Some((out, dropped)) = selected.and_then(|keys| select_keys(&text, &keys))
                    {
                        return Ok(json!({
                            "path": path.to_string_lossy(),
                            "bytes": bytes.len(),
                            "sha256": sha256(&bytes),
                            "droppedKeys": dropped,
                            "chars": out.chars().count(),
                            "truncated": out.chars().count() > USER_READ_LIMIT,
                            "text": out.chars().take(USER_READ_LIMIT).collect::<String>(),
                        }));
                    }
                    Ok(json!({
                        "path": path.to_string_lossy(),
                        "bytes": bytes.len(),
                        "sha256": sha256(&bytes),
                        "chars": text.chars().count(),
                        "truncated": text.chars().count() > USER_READ_LIMIT,
                        "text": text.chars().take(USER_READ_LIMIT).collect::<String>(),
                    }))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.inspect_script",
            "读取脚本说明",
            "一次读取一个已知 JS 脚本包的 manifest、README、settings 参数定义、目录条目和 settings 下的账户配置。folderName 必须来自配置组任务或 User/JsScript 目录；不要再分别 list/read 同一脚本。",
            json!({"type":"object","properties":{"folderName":{"type":"string","minLength":1,"description":"配置组 Javascript 任务中的精确 folderName"}},"required":["folderName"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let folder = a["folderName"].as_str().unwrap_or("").trim();
                    let root = client.user_root()?;
                    let path = user_path(&root, &format!("JsScript/{folder}"))?;
                    if !path.is_dir() {
                        return Err(Error::Tool(format!("JS 脚本目录不存在：{folder}")));
                    }
                    let mut entries = fs::read_dir(&path)
                        .map_err(|e| Error::Tool(format!("读取脚本目录失败：{e}")))?
                        .filter_map(|entry| entry.ok())
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    entries.sort();
                    let readme = entries
                        .iter()
                        .find(|name| name.eq_ignore_ascii_case("README.md"))
                        .and_then(|name| read_small_text(&path.join(name), 64 * 1024));
                    let profile_path = path.join("settings");
                    let mut profiles = Vec::new();
                    if profile_path.is_dir() {
                        for entry in fs::read_dir(&profile_path)
                            .map_err(|e| Error::Tool(format!("读取脚本账户配置失败：{e}")))?
                            .filter_map(|entry| entry.ok())
                        {
                            if let Some(value) = read_small_json(&entry.path(), 64 * 1024) {
                                profiles.push(json!({
                                    "name":entry.file_name().to_string_lossy(),
                                    "value":value
                                }));
                            }
                        }
                        profiles.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
                    }
                    Ok(json!({
                        "folderName":folder,
                        "entries":entries,
                        "manifest":read_small_json(&path.join("manifest.json"), 64 * 1024),
                        "settings":read_small_json(&path.join("settings.json"), 128 * 1024),
                        "readme":readme,
                        "profiles":profiles,
                    }))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.resolve",
            "查找可运行任务",
            "一次判定采集或运行目标：查配置组、核验引用路径是否还在、只按目录名找 AutoPathing 父节点。不要用 list/read 扫路线 JSON。按 verdict 行动：run 直接运行该配置组；repair 只补 missing；create 用 candidates 父节点建组；ambiguous 才提问；notFound 再考虑更新仓库。",
            json!({"type":"object","properties":{"query":{"type":"string","minLength":1,"description":"用户原话或材料/配置组名称"}},"required":["query"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    Ok(resolve::resolve_local(
                        &root,
                        a["query"].as_str().unwrap_or(""),
                    ))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.write",
            "写入配置文件",
            "原子创建或替换 BetterGI User 资源文件。已有文件必须提交 user.read 返回的 sha256，写入前校验 JSON、比较版本并保留独立备份；写后自动核验。不得修改 User/config.json。",
            json!({"type":"object","properties":{"path":{"type":"string","description":"相对 User 路径；不得是 config.json"},"content":{"type":"string","description":"保留未知字段后的完整文件内容"},"expectedSha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"替换已有文件时必填，使用最近一次 user.read 返回的 sha256；新建文件省略"}},"required":["path","content"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    let path = user_path(&root, a["path"].as_str().unwrap_or(""))?;
                    if path
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("config.json"))
                        && path.parent() == Some(root.as_path())
                    {
                        return Err(Error::Tool(
                            "User/config.json 必须通过 bgi.api 设置事务修改".into(),
                        ));
                    }
                    let content = a["content"].as_str().unwrap_or("");
                    validate_resource(&path, content.as_bytes())?;
                    let previous = if path.exists() {
                        let bytes = fs::read(&path)
                            .map_err(|error| Error::Tool(format!("读取写入前版本失败，未写入：{error}")))?;
                        let actual = sha256(&bytes);
                        let expected = a["expectedSha256"].as_str().ok_or_else(|| {
                            Error::Tool("目标已存在；必须先读取并提交 expectedSha256，未写入".into())
                        })?;
                        if actual != expected {
                            return Err(Error::Conflict(
                                "目标文件在读取后发生变化；未覆盖较新的内容，请重新读取".into(),
                            ));
                        }
                        Some(actual)
                    } else {
                        if a.get("expectedSha256").is_some() {
                            return Err(Error::Conflict(
                                "目标文件已不存在；未按替换请求重新创建".into(),
                            ));
                        }
                        None
                    };
                    let backup = replace_resource(&path, content.as_bytes())?;
                    let backup = backup.map(|backup| {
                        backup
                            .strip_prefix(&root)
                            .unwrap_or(&backup)
                            .to_string_lossy()
                            .replace('\\', "/")
                    });
                    Ok(json!({
                        "path": path.to_string_lossy(),
                        "created": previous.is_none(),
                        "previousSha256": previous,
                        "sha256": sha256(content.as_bytes()),
                        "bytes": content.len(),
                        "backup": backup,
                        "verified": true,
                        "note": "文件已持久化；是否立即刷新界面由对应 BetterGI 功能决定。",
                    }))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.restore",
            "恢复备份",
            "把 bgi.user.write 返回的独立备份恢复到原资源。恢复前比较当前 sha256，避免覆盖写入后的其他修改；恢复本身也为当前版本创建新备份并核验。",
            json!({"type":"object","properties":{"path":{"type":"string","description":"原资源的相对 User 路径"},"backup":{"type":"string","description":"user.write 返回的相对 backup 路径"},"expectedSha256":{"type":"string","pattern":"^[0-9a-f]{64}$","description":"当前原资源最近一次 user.read 或 user.write 返回的 sha256"}},"required":["path","backup","expectedSha256"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    let path = user_path(&root, a["path"].as_str().unwrap_or(""))?;
                    let backup = user_path(&root, a["backup"].as_str().unwrap_or(""))?;
                    let expected_name = format!(
                        "{}.sleepy-doll.",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                    let valid_backup = backup.parent() == path.parent()
                        && backup.file_name().is_some_and(|name| {
                            let name = name.to_string_lossy();
                            name.starts_with(&expected_name) && name.ends_with(".bak")
                        });
                    if !valid_backup || !backup.is_file() {
                        return Err(Error::Tool("backup 不是该目标的 Sleepy Doll 恢复文件".into()));
                    }
                    let current = fs::read(&path)
                        .map_err(|error| Error::Tool(format!("读取当前目标失败，未恢复：{error}")))?;
                    if sha256(&current) != a["expectedSha256"].as_str().unwrap_or("") {
                        return Err(Error::Conflict(
                            "目标文件在写入后又发生变化；未覆盖较新的内容".into(),
                        ));
                    }
                    let content = fs::read(&backup)
                        .map_err(|error| Error::Tool(format!("读取备份失败，未恢复：{error}")))?;
                    validate_resource(&path, &content)?;
                    let displaced = replace_resource(&path, &content)?;
                    Ok(json!({
                        "path": path.to_string_lossy(),
                        "restoredFrom": a["backup"],
                        "sha256": sha256(&content),
                        "displacedBackup": displaced.map(|value| value.strip_prefix(&root).unwrap_or(&value).to_string_lossy().replace('\\', "/")),
                        "verified": true,
                    }))
                }) as BridgeToolFn
            },
        ),
    ]);
    for (name, label, description, schema, function) in definitions {
        let execution = if matches!(
            name,
            "bgi.capability.invoke" | "bgi.job.cancel" | "bgi.api.invoke"
        ) {
            ToolExecution {
                effect: ToolEffect::GameWrite,
                concurrency: crate::extension::ConcurrencyMode::GameExclusive,
                idempotency: crate::extension::IdempotencyMode::OriginalKeyOnly,
                cancellation: crate::extension::CancellationMode::Cooperative,
                verification: crate::extension::VerificationMode::Job,
                deferred: false,
                always_load: true,
                search_hint: Some("执行或取消 BGI 游戏动作".into()),
                ..ToolExecution::default()
            }
        } else if matches!(name, "bgi.user.write" | "bgi.user.restore") {
            // 改的是用户文件而不是游戏动作：串行执行，需要授权。
            ToolExecution {
                effect: ToolEffect::LocalWrite,
                risk: crate::extension::RiskLevel::High,
                concurrency_safe: false,
                deferred: false,
                always_load: true,
                search_hint: Some("修改用户的 BGI 配置文件".into()),
                // 影响按真实差异计。
                scope: crate::extension::ScopeKind::Fields,
                scope_target: Some("path".into()),
                scope_reader: Some("bgi.user.read".into()),
                ..ToolExecution::default()
            }
        } else if matches!(
            name,
            "bgi.user.list" | "bgi.user.read" | "bgi.user.inspect_script" | "bgi.user.resolve"
        ) {
            ToolExecution {
                max_result_chars: 96_000,
                deferred: false,
                always_load: true,
                search_hint: Some("读取 BetterGI 用户资源".into()),
                ..ToolExecution::read_only()
            }
        } else {
            ToolExecution {
                deferred: false,
                always_load: true,
                search_hint: Some("检索或观测 BGI 状态与能力".into()),
                ..ToolExecution::read_only()
            }
        };
        registry.register(
            FunctionTool::new(name, description, schema, "core:bgi", move |arguments| {
                function(arguments)
            })
            .with_label(label)
            .with_execution(execution),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_path_resolves_inside_the_user_directory() {
        let root = Path::new(r"C:\BGI\User");
        assert_eq!(
            user_path(root, "ScriptGroup").unwrap(),
            root.join("ScriptGroup")
        );
        assert_eq!(
            user_path(root, r"ScriptGroup\每日.json").unwrap(),
            root.join("ScriptGroup").join("每日.json")
        );
        assert_eq!(user_path(root, "").unwrap(), root.to_path_buf());
        assert_eq!(
            user_path(root, "./config.json").unwrap(),
            root.join("config.json")
        );
    }

    /// 目录穿越必须在解析阶段挡掉。
    #[test]
    fn user_path_rejects_escapes() {
        let root = Path::new(r"C:\BGI\User");
        for bad in [
            r"..\..\Windows\System32",
            "../config.json",
            "a/../../b",
            "..",
            r"C:\Windows\System32",
            "/etc/passwd",
            r"\\server\share",
        ] {
            assert!(user_path(root, bad).is_err(), "未拒绝：{bad}");
        }
    }

    /// 按需取字段。
    #[test]
    fn select_keys_keeps_only_the_requested_top_level_fields() {
        // 按真实比例：config 段占绝大部分。
        let text = format!(
            r#"{{"index":9,"name":"test","config":{{"pathingConfig":{{"a":"{}"}}}},"projects":[{{"name":"p"}}]}}"#,
            "x".repeat(600)
        );
        let (kept, dropped) = select_keys(&text, &["name", "index", "projects"]).unwrap();
        let value: Value = serde_json::from_str(&kept).unwrap();
        assert_eq!(value["name"], "test");
        assert_eq!(value["index"], 9);
        assert!(value.get("config").is_none());
        assert_eq!(dropped, vec!["config".to_string()]);
        assert!(kept.chars().count() < text.chars().count() / 10);

        // 请求了不存在的字段不算错，只是拿不到。
        let (kept, _) = select_keys(&text, &["name", "nope"]).unwrap();
        assert_eq!(kept, r#"{"name":"test"}"#);
    }

    /// 不是 JSON 对象时不裁剪，调用方退回全文。
    #[test]
    fn select_keys_declines_non_json_input() {
        assert!(select_keys("不是 JSON", &["name"]).is_none());
        assert!(select_keys("[1,2,3]", &["name"]).is_none());
    }

    #[test]
    fn directory_projection_reads_requested_json_fields_in_one_pass() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("group.json");
        fs::write(
            &file,
            r#"{"name":"日常","index":3,"config":{"large":"ignored"},"projects":[{"name":"任务"}]}"#,
        )
        .unwrap();
        let projected = project_json(&file, &["name", "index", "projects"]).unwrap();
        assert_eq!(projected["name"], "日常");
        assert_eq!(projected["index"], 3);
        assert_eq!(projected["projects"][0]["name"], "任务");
        assert!(projected.get("config").is_none());
    }

    #[test]
    fn domain_tools_expose_aggregated_reads_and_safe_write_guidance() {
        let client = Arc::new(BgiClient::new(BridgeConfig {
            enabled: true,
            base_url: "http://127.0.0.1:1".into(),
            token: Some("test".into()),
            instance_id: None,
            timeout_ms: 1000,
        }));
        let mut registry = ToolRegistry::default();
        register_tools(&mut registry, client).unwrap();
        let definitions = registry.definitions();
        let search = definitions
            .iter()
            .find(|tool| tool.name == "bgi.api.search")
            .unwrap();
        assert!(
            search.input_schema["properties"]["group"]
                .get("enum")
                .is_none()
        );
        let inspect = definitions
            .iter()
            .find(|tool| tool.name == "bgi.user.inspect_script")
            .unwrap();
        assert!(inspect.description.contains("一次读取"));
        assert_eq!(inspect.input_schema["required"], json!(["folderName"]));
        let resolve = definitions
            .iter()
            .find(|tool| tool.name == "bgi.user.resolve")
            .unwrap();
        assert!(resolve.description.contains("不要用 list/read 扫路线"));
        assert_eq!(resolve.input_schema["required"], json!(["query"]));
        let write = definitions
            .iter()
            .find(|tool| tool.name == "bgi.user.write")
            .unwrap();
        assert!(write.description.contains("不得修改 User/config.json"));
        assert!(
            write.input_schema["properties"]
                .get("expectedSha256")
                .is_some()
        );
        let restore = definitions
            .iter()
            .find(|tool| tool.name == "bgi.user.restore")
            .unwrap();
        assert!(restore.description.contains("恢复"));
    }

    /// 替换是原子的、可核验的，每次修改留下独立备份。
    #[test]
    fn resource_replace_is_verified_and_recoverable() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let target = user_path(root, "ScriptGroup/group.json").unwrap();
        let original = r#"{"Name":"原"}"#.as_bytes();
        let changed = r#"{"Name":"改"}"#.as_bytes();
        assert!(replace_resource(&target, original).unwrap().is_none());
        let backup = replace_resource(&target, changed).unwrap().unwrap();
        assert_eq!(fs::read(&target).unwrap(), changed);
        assert_eq!(fs::read(&backup).unwrap(), original);
        let displaced = replace_resource(&target, &fs::read(&backup).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(fs::read(&target).unwrap(), original);
        assert_eq!(fs::read(displaced).unwrap(), changed);
    }

    #[test]
    fn invalid_json_is_rejected_before_resource_write() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("group.json");
        assert!(validate_resource(&target, br#"{"broken":}"#).is_err());
        assert!(!target.exists());
    }
}
