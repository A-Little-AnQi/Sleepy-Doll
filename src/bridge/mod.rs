//! 与 BetterGI 的接触面：桥的客户端、工具，以及桥进程的生命周期。

pub(crate) mod approval;
pub mod control;
pub mod features;
pub(crate) mod origin;
pub mod recovery;
pub(crate) mod resolve;
pub(crate) mod retrieval;

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

fn local_resource_lifecycle(path: &str) -> Value {
    let normalized = path.replace('\\', "/");
    if ![
        "AutoPathing/",
        "JsScript/",
        "KeyMouseScript/",
        "AutoFight/",
        "AutoGeniusInvokation/",
    ]
    .iter()
    .any(|root| normalized.starts_with(root))
    {
        return Value::Null;
    }
    json!({"path":normalized,"inspectMethodId":"bgi.inspect_local_resource","deleteMethodId":"bgi.delete_local_resource","restoreMethodId":"bgi.restore_local_resource","selectionRequired":false,
        "next":"若用户要求删除此资源，直接 describe/read inspect_local_resource，再按返回 path/version describe/invoke delete_local_resource。已经有真实路径，不再构造页面、遍历懒加载树或枚举选中项。"})
}

fn referenced_resource_targets(content: &[u8]) -> Value {
    let Ok(value) = serde_json::from_slice::<Value>(content) else {
        return Value::Null;
    };
    let mut targets = std::collections::BTreeMap::<String, Vec<String>>::new();
    for project in value["projects"].as_array().into_iter().flatten() {
        let root = match project["type"].as_str() {
            Some("Pathing") => "AutoPathing",
            Some("Javascript") => "JsScript",
            Some("KeyMouse") => "KeyMouseScript",
            _ => continue,
        };
        let Some(folder) = project["folderName"].as_str() else {
            continue;
        };
        let path = format!("{root}/{}", folder.replace('\\', "/").trim_matches('/'));
        targets
            .entry(path)
            .or_default()
            .push(project["name"].as_str().unwrap_or("").into());
    }
    json!({"targets":targets.into_iter().map(|(path,projects)|json!({"path":path,"projects":projects,"resourceLifecycle":local_resource_lifecycle(&path)})).collect::<Vec<_>>(),
        "next":"用户同时要求删除组及引用资源时，先删除已定位组，再按上述真实引用目录 inspect_local_resource；不从 AutoPathing 根目录重新遍历。检查范围后按用户目标删除完整目录或精确文件。"})
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
    /// 生产恒为 true：每个非 info 请求前都核对对端进程来源。只有本地
    /// 假服务器的测试经 for_tests 置 false；不构成公开 API。
    origin_preflight: bool,
}

impl BgiClient {
    pub fn new(config: BridgeConfig) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_millis(config.timeout_ms)))
            .build()
            .new_agent();
        Self {
            config,
            agent,
            origin_preflight: true,
        }
    }

    /// 仅供测试：本地假服务器没有官方 BetterGI 进程，跳过来源预检。
    #[cfg(test)]
    pub(crate) fn for_tests(config: BridgeConfig) -> Self {
        Self {
            origin_preflight: false,
            ..Self::new(config)
        }
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

    fn repository_read(&self, method_id: &str, arguments: &Value) -> Result<Value> {
        let contract = self.describe(method_id)?;
        if contract["effect"] != "readOnly" || contract["callable"] != true {
            return Err(Error::Tool(
                "当前桥不支持只读仓库查询，请重新连接更新后的桥".into(),
            ));
        }
        let response = self.invoke(method_id, arguments)?;
        response
            .get("result")
            .cloned()
            .ok_or_else(|| Error::Tool("仓库读取没有返回结果".into()))
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
        if path != "/bridge/v1/info" && self.origin_preflight {
            control::info(&self.config)?;
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
        let value: Value = response.body_mut().read_json()?;
        if path == "/bridge/v1/info" {
            origin::require_info(&value)?;
        }
        Ok(value)
    }
}

pub fn register_tools(registry: &mut ToolRegistry, client: Arc<BgiClient>) -> Result<()> {
    let features = features::FeatureIndex::bundled()?;
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
        ("bgi.feature.search", "查找 BGI 功能链路", "离线检索 BGI 插件的全量功能与流程索引；只返回少量摘要。用户目标或设置/命令不明确时用它，明确运行资源仍先 user.resolve。静态命中不代表现场可调用；用 feature.read 读取单项链路，再 describe 当前接口。", json!({"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":200},"kind":{"type":"string","enum":["workflow","command","setting","page","scriptApi","resourceModel","stable"]},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":12,"default":5}},"required":["query"],"additionalProperties":false}), {
            let features = features.clone();
            Arc::new(move |a: &Value| features.search(a["query"].as_str().unwrap_or(""),a["kind"].as_str(),a["offset"].as_u64().unwrap_or(0) as usize,a["limit"].as_u64().unwrap_or(5) as usize)) as BridgeToolFn
        }),
        ("bgi.feature.read", "读取单项 BGI 链路", "读取 feature.search 返回的一个精确 ID，取得输入来源、步骤、分支、验证方法与相关参考资料。不读取全量手册，不证明当前接口可调用；只按当前目标继续读取引用和 describe。", json!({"type":"object","properties":{"id":{"type":"string","minLength":1}},"required":["id"],"additionalProperties":false}), {
            let features = features.clone();
            Arc::new(move |a: &Value| features.read(a["id"].as_str().unwrap_or(""))) as BridgeToolFn
        }),
        ("bgi.repo.search", "检索脚本仓库", "搜索中央仓库全部资源，默认 all；JS 参数用 js，采集/地图追踪必须用 pathing。地图追踪返回完整目标目录和作者包、requirements，不返回散落叶子供拼接。分类无命中不等于全仓库没有。用精确路径阅读、订阅并准备运行，不要扫描桥程序集。", json!({"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":200},"category":{"type":"string","enum":["all","js","pathing","combat","tcg"],"default":"all"},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":20,"default":8}},"required":["query"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.repository_read("bgi.search_script_repository", a)) as BridgeToolFn
        }),
        ("bgi.repo.read", "读取仓库源码", "直接从中央 Git 仓库读取未订阅的 settings.json、README、manifest、入口 JS 与引用模块。path 使用搜索返回的 js/... 路径再拼文件名，不加 repo/，不猜已安装目录。contains 定位字段或函数及上下文；返回行号、SHA-256、truncated 和 nextLine，截断时继续分页。阅读不会运行脚本或修改订阅。", json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":1024},"startLine":{"type":"integer","minimum":1},"maxLines":{"type":"integer","minimum":1,"maximum":240,"default":160},"contains":{"type":"string","minLength":1,"maxLength":200}},"required":["path"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.repository_read("bgi.read_script_repository_file", a)) as BridgeToolFn
        }),
        ("bgi.api.search", "检索 BetterGI 接口", "在当前 BetterGI 宿主中发现设置或动作。它不搜索配置组、路线、脚本等用户资源。group 必须来自目录实际返回的分组；用一个业务词查询，一次零结果后检查证据源。", json!({"type":"object","properties":{"query":{"type":"string","description":"一个核心业务词、动作词或精确 methodId；空字符串用于浏览分组"},"group":{"type":"string","description":"可选；使用目录实际返回的分组，例如 settings、command、scheduler、repository"},"offset":{"type":"integer","minimum":0,"description":"仅在响应给出 nextOffset 时继续"},"limit":{"type":"integer","minimum":1,"maximum":50,"default":8,"description":"候选数量；默认 8，只有响应给出 nextOffset 且确有必要时增加"}},"required":["query"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.catalog_page_with_limit(a["query"].as_str().unwrap_or(""),a["group"].as_str(),a["offset"].as_u64().unwrap_or(0),a["limit"].as_u64().unwrap_or(8))) as BridgeToolFn
        }),
        ("bgi.api.describe", "读取接口说明", "读取精确 methodId 的当前契约，再 read/invoke。直接资源／设置／领域接口优先，cmd.*、bgi.ui.* 与页面上下文是最低优先级；能直接操作数据时不操作界面。callable=false 说明当前接口限制。", json!({"type":"object","properties":{"methodId":{"type":"string","description":"来自当前证据的精确 ID；资源删除用 bgi.inspect_local_resource / bgi.delete_local_resource，不选地图追踪界面删除命令"}},"required":["methodId"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.describe(a["methodId"].as_str().unwrap_or(""))) as BridgeToolFn
        }),
        ("bgi.api.read", "读取 BetterGI 状态", "调用刚通过 api.describe 确认的只读接口。用于读取宿主当前设置或诊断；不用于读取 User 文件。arguments 必须满足该接口 inputSchema。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object","description":"无参数接口传空对象"}},"required":["methodId","arguments"],"additionalProperties":false}), {
            Arc::new(move |_: &Value| Err(Error::Tool("该接口必须通过运行时的契约检查调用".into()))) as BridgeToolFn
        }),
        ("bgi.api.invoke", "执行 BetterGI 操作", "只调用本次运行已经 api.describe 的写接口。直接改资源／设置／领域数据优先，界面操作优先级最低。删除路线或脚本先 inspect_local_resource，再调用 delete_local_resource；不先选中、右键、导航或展开树。多组运行及关闭游戏收尾先统一计划一次 bgi.run_script_groups，默认 waitForCompletion=false 后台交接；同计划的后续步骤不是用户要求等待。仅明确要求监控、等结果或汇报完成才 true；普通请求直接规划并运行，不额外问是否守护，游戏运行期间不查 Job、日志或状态。游戏启动后的加载用 wait_ready 阻塞等待，不让模型循环查状态。运行时等待期间不调用模型，取得交接证据后给最终总结并结束，不再 job.get、查状态或日志，也不重跑；明确要求等待／后续步骤／排错时才继续。completed 不自动表示业务成功。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object","description":"严格满足本次已读取的 inputSchema"}},"required":["methodId","arguments"],"additionalProperties":false}), {
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
                            "relativePath": entry.path().strip_prefix(&root).unwrap_or(&entry.path()).to_string_lossy().replace('\\', "/"),
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
                    Ok(json!({"path":path.to_string_lossy(),"count":entries.len(),"entries":entries,"resourceLifecycle":local_resource_lifecycle(a["path"].as_str().unwrap_or(""))}))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.read",
            "读取配置文件",
            "读取 BetterGI User 目录中的一个文本文件。JSON 顶层字段按名字用 keys 投影：配置组的 name、index、projects，以及队伍、策略、拾取、恢复等设置的 config，都按名投影所需字段即可，不必读全文。已有确切 path 的独立文件直接读取。droppedKeys 只是本轮未请求的字段，不代表它们不存在。修改文件前必须先读过完整文件内容并在写回时保留其中的未知字段，提交写入前完整 SHA256。多个独立文件应在同一轮并行读取。",
            json!({"type":"object","properties":{
                "path":{"type":"string","description":"由 user.list 或已读文件得到的相对 User 路径"},
                "keys":{"type":"array","items":{"type":"string"},"description":"JSON 顶层字段按名投影；查询或修改队伍/策略等都在 config 字段内按名取，如 config、name、index、projects；写回前须先读过含未知字段的完整内容以取得完整 SHA"}
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
                            "resourceLifecycle": local_resource_lifecycle(a["path"].as_str().unwrap_or("")),
                            "referencedResources": referenced_resource_targets(&bytes),
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
                        "resourceLifecycle": local_resource_lifecycle(a["path"].as_str().unwrap_or("")),
                        "referencedResources": referenced_resource_targets(&bytes),
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
                    let manifest = read_small_json(&path.join("manifest.json"), 64 * 1024);
                    let settings_name = manifest.as_ref().and_then(|value| value["settings_ui"].as_str().or_else(|| value["settingsUi"].as_str())).filter(|name| !name.is_empty());
                    let settings_path = settings_name.map(|name| user_path(&path, name)).transpose()?;
                    Ok(json!({
                        "folderName":folder,
                        "entries":entries,
                        "manifest":manifest,
                        "settingsPath":settings_name,
                        "settings":settings_path.as_ref().and_then(|path| read_small_json(path, 128 * 1024)),
                        "readme":readme,
                        "profiles":profiles,
                    }))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.resolve",
            "查找可运行任务",
            "查找用户要求运行的实际资源：先匹配本机配置组、核验引用和地图追踪父目录；未命中自动搜索当前中央仓库全部分类。不要扫描路线 JSON 或先刷新仓库。run 表示组完整可运行，仍需比对返回的 config 槽位与用户目标，目标含队伍/赶路/参数变更时先写入再运行；repair 补 missing；create 准备本地父目录；resourceFound 按 repository 候选订阅、配置并继续运行；lookupFailed 是查询失败，不能称资源不存在；notFound 才是本机和当前全仓索引均未命中。",
            json!({"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":200,"description":"用户原话或材料/脚本/配置组名称，例如帮我跑下血斛"}},"required":["query"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a: &Value| {
                    let root = client.user_root()?;
                    resolve::resolve_target(resolve::resolve_local(
                        &root,
                        a["query"].as_str().unwrap_or(""),
                    ), |arguments| client.repository_read("bgi.search_script_repository", arguments))
                }) as BridgeToolFn
            },
        ),
        (
            "bgi.user.write",
            "写入配置文件",
            "原子创建或替换 BetterGI User 资源文件。替换已有文件必须先在本轮重新 user.read 并提交它返回的 sha256；历史轮读取的内容与 sha256 不是当前证据。校验不一致时本次不写入，会以 ok:false 返回版本冲突——重新读取最新文件，保留其中新出现的变更，只改用户目标的字段后再次提交；只有确实无法确定改法时才询问用户。写入前校验 JSON、比较版本并保留独立备份；写后自动核验。不得修改 User/config.json。",
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
                            return Err(Error::Stale(
                                "目标文件在读取后发生变化；未覆盖较新的内容，请重新读取".into(),
                            ));
                        }
                        Some(actual)
                    } else {
                        if a.get("expectedSha256").is_some() {
                            return Err(Error::Stale(
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
                        return Err(Error::Stale(
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
            "bgi.user.list"
                | "bgi.user.read"
                | "bgi.user.inspect_script"
                | "bgi.user.resolve"
                | "bgi.repo.search"
                | "bgi.repo.read"
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
mod user_write_tests {
    use super::*;
    use std::io::{Read, Write as IoWrite};
    use std::net::TcpListener;

    /// 只应答 GET /bridge/v1/host 的假桥：user.write 的其余逻辑全在本地文件上。
    /// 返回 RAII 守卫：Drop 时停止接收循环并 join 线程，不遗留端口占用。
    struct FakeHostBridge {
        base_url: String,
        address: std::net::SocketAddr,
        stop: Arc<std::sync::atomic::AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }
    impl FakeHostBridge {
        fn start(user_path: String) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let port = address.port();
            let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flagged = stop.clone();
            let handle = std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let mut buffer = [0u8; 4096];
                    let _ = stream.read(&mut buffer);
                    let body = json!({"userPath": user_path}).to_string();
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(head.as_bytes());
                    if flagged.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                }
            });
            Self {
                base_url: format!("http://127.0.0.1:{port}"),
                address,
                stop,
                handle: Some(handle),
            }
        }
    }
    impl Drop for FakeHostBridge {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
            // 空连接唤醒阻塞的 accept，让线程走到停止检查后退出。
            let _ = std::net::TcpStream::connect(self.address);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn sha_of(bytes: &[u8]) -> String {
        sha256(bytes)
    }

    /// 临时数据统一放 target/.tmp/task-write-recovery（不写系统 Temp，也不
    /// 删除既有内容：固定文件每次覆写复位，一次性文件用唯一名）。
    fn user_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from("target/.tmp/task-write-recovery").join(name);
        std::fs::create_dir_all(dir.join("settings")).unwrap();
        std::fs::write(
            dir.join("settings").join("groups.json"),
            "{\"x\":1,\"note\":\"保留我\"}",
        )
        .unwrap();
        dir
    }

    fn registry(base_url: &str) -> ToolRegistry {
        let client = Arc::new(BgiClient::for_tests(BridgeConfig {
            enabled: true,
            base_url: base_url.into(),
            token: Some("test-token".into()),
            instance_id: None,
            timeout_ms: 5_000,
            host_install_path: None,
            auto_start: true,
            launch_silently: false,
        }));
        let mut registry = ToolRegistry::default();
        register_tools(&mut registry, client).unwrap();
        registry
    }

    async fn read(registry: &ToolRegistry, path: &str) -> Result<Value> {
        registry
            .call_async(
                "bgi.user.read",
                &json!({"path": path}),
                tokio_util::sync::CancellationToken::new(),
            )
            .await
    }

    async fn write(registry: &ToolRegistry, path: &str, content: &str, sha: &str) -> Result<Value> {
        registry
            .call_async(
                "bgi.user.write",
                &json!({"path":path,"content":content,"expectedSha256":sha}),
                tokio_util::sync::CancellationToken::new(),
            )
            .await
    }

    #[tokio::test]
    async fn stale_hash_conflicts_are_recoverable_and_leave_file_untouched() {
        let dir = user_dir("stale-hash");
        let server = FakeHostBridge::start(dir.to_string_lossy().into_owned());
        let registry = &registry(&server.base_url);
        let target = dir.join("settings").join("groups.json");

        // 读取后文件被别处更新：旧 SHA 提交必须得到可恢复的版本冲突（Stale），
        // 而不是终止运行的 Conflict，也绝不覆盖较新内容。
        let outdated = sha_of(b"{\"x\":0}");
        let error = write(&registry, "settings/groups.json", "{\"x\":2}", &outdated)
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Stale(_)),
            "旧校验值应产生版本冲突，实际：{error}"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            "{\"x\":1,\"note\":\"保留我\"}".as_bytes(),
            "冲突时文件必须保持原样"
        );

        // 模型按指引走完整恢复路径：真实调用 bgi.user.read 取回全文与 SHA，
        // 从最新 JSON 只改目标字段 x、保留 note，再提交写入。
        let latest = read(&registry, "settings/groups.json")
            .await
            .expect("重新读取必须成功");
        assert_eq!(latest["truncated"], json!(false), "恢复依据必须是完整读取");
        let mut document: Value = serde_json::from_str(latest["text"].as_str().unwrap()).unwrap();
        document["x"] = json!(2);
        let replaced = serde_json::to_string(&document).unwrap();
        let ok = write(
            &registry,
            "settings/groups.json",
            &replaced,
            latest["sha256"].as_str().unwrap(),
        )
        .await
        .expect("以重读返回的 SHA 提交应成功");
        assert_eq!(ok["verified"], json!(true));
        let written: Value = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        assert_eq!(written["x"], json!(2));
        assert_eq!(
            written["note"],
            json!("保留我"),
            "重读后的写入必须保留原有字段"
        );

        // 成功后旧 SHA 再用立即再次冲突：不得悄悄通过。
        let again = write(&registry, "settings/groups.json", "{\"x\":3}", &outdated).await;
        assert!(matches!(again, Err(Error::Stale(_))));
        let final_state: Value = serde_json::from_slice(&std::fs::read(&target).unwrap()).unwrap();
        assert_eq!(final_state["x"], json!(2), "第二次冲突同样不得覆盖");
    }

    #[tokio::test]
    async fn replace_request_for_missing_file_is_stale_not_recreate() {
        let dir = user_dir("missing-target");
        let server = FakeHostBridge::start(dir.to_string_lossy().into_owned());
        let registry = &registry(&server.base_url);
        // 目标已不存在但按替换语义提交了 SHA：必须反馈冲突，不得悄悄重建。
        // 文件名带唯一 ID，重跑不依赖删除。
        let missing = format!("settings/gone-{}.json", uuid::Uuid::new_v4());
        let error = write(&registry, &missing, "{}", &"0".repeat(64))
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Stale(_)),
            "目标消失应反馈版本冲突：{error}"
        );
        assert!(!dir.join(&missing).exists());
        // 新建省略 SHA 仍然直接成功。
        let created = registry
            .call_async(
                "bgi.user.write",
                &json!({"path":missing,"content":"{}"}),
                tokio_util::sync::CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(created["created"], json!(true));
    }
}
