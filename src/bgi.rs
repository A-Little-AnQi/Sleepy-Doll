use std::{sync::Arc, time::Duration};

use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    config::BridgeConfig,
    error::{Error, Result},
    tools::{FunctionTool, ToolEffect, ToolExecution, ToolRegistry},
};

type BridgeToolFn = Arc<dyn Fn(&Value) -> Result<Value> + Send + Sync>;
type BridgeToolDefinition<'a> = (&'a str, &'a str, Value, BridgeToolFn);

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
    pub fn catalog(&self, query: &str) -> Result<Value> {
        self.catalog_page(query, None, 0)
    }
    pub fn catalog_page(&self, query: &str, group: Option<&str>, offset: u64) -> Result<Value> {
        let mut params = url::form_urlencoded::Serializer::new(String::new());
        params
            .append_pair("q", query)
            .append_pair("limit", "50")
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
            return Err(Error::Tool("BGI Bridge is disabled".into()));
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
            "读取 BGI 当前状态快照；只观测，不执行游戏动作。",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |_| client.state())
            },
        ),
        (
            "bgi.capability.search",
            "按关键词搜索 BGI Bridge 当前身份可见的真实能力。",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.catalog(a["query"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.capability.describe",
            "读取 BGI 能力的参数、执行模式和验证约定。",
            json!({"type":"object","properties":{"methodId":{"type":"string"}},"required":["methodId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.describe(a["methodId"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.capability.invoke",
            "调用已确认的 BGI 能力；Job 接纳不代表业务已成功。",
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
            "读取 BGI Job 状态、结果和业务验证状态。",
            json!({"type":"object","properties":{"jobId":{"type":"string"}},"required":["jobId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.job(a["jobId"].as_str().unwrap_or_default()))
            },
        ),
        (
            "bgi.job.cancel",
            "请求协作式取消指定 BGI Job。",
            json!({"type":"object","properties":{"jobId":{"type":"string"}},"required":["jobId"],"additionalProperties":false}),
            {
                let client = client.clone();
                Arc::new(move |a| client.cancel(a["jobId"].as_str().unwrap_or_default()))
            },
        ),
    ];
    definitions.extend([
        ("bgi.api.search", "检索当前 BetterGI 全量接口的用途摘要。按任务关键词发现配置、状态与命令接口；结果包含可调用状态。选中接口后必须用 bgi.api.describe 阅读完整契约。", json!({"type":"object","properties":{"query":{"type":"string"},"group":{"type":"string"},"offset":{"type":"integer","minimum":0}},"required":["query"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.catalog_page(a["query"].as_str().unwrap_or(""),a["group"].as_str(),a["offset"].as_u64().unwrap_or(0))) as BridgeToolFn
        }),
        ("bgi.api.describe", "读取某个已发现接口的完整 Agent 调用说明：用途、何时调用、前置条件、参数和返回结构、副作用、结果判定、回退方法及示例。不可调用项也有明确原因。", json!({"type":"object","properties":{"methodId":{"type":"string"}},"required":["methodId"],"additionalProperties":false}), {
            let client = client.clone();
            Arc::new(move |a: &Value| client.describe(a["methodId"].as_str().unwrap_or(""))) as BridgeToolFn
        }),
        ("bgi.api.read", "调用已阅读契约的只读接口，例如读取设置、检索命令或预览配置差异。拒绝写接口；参数必须符合接口 inputSchema，返回接口 result 对象。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object"}},"required":["methodId","arguments"],"additionalProperties":false}), {
            Arc::new(move |_: &Value| Err(Error::Tool("该接口必须通过运行时的契约检查调用".into()))) as BridgeToolFn
        }),
        ("bgi.api.invoke", "调用已阅读契约的写接口，执行前绑定用户授权，使用幂等 Job 跟踪结果。配置修改应先 get_setting 和 preview_settings，再提交 planId；保存 changeId 用于回退。不得把命令返回视为游戏目标成功。", json!({"type":"object","properties":{"methodId":{"type":"string"},"arguments":{"type":"object"}},"required":["methodId","arguments"],"additionalProperties":false}), {
            Arc::new(move |_: &Value| Err(Error::Tool("该接口必须通过运行时的授权与 Job 跟踪调用".into()))) as BridgeToolFn
        }),
    ]);
    for (name, description, schema, function) in definitions {
        let execution = if matches!(
            name,
            "bgi.capability.invoke" | "bgi.job.cancel" | "bgi.api.invoke"
        ) {
            ToolExecution {
                effect: ToolEffect::GameWrite,
                concurrency: crate::tools::ConcurrencyMode::GameExclusive,
                idempotency: crate::tools::IdempotencyMode::OriginalKeyOnly,
                cancellation: crate::tools::CancellationMode::Cooperative,
                verification: crate::tools::VerificationMode::Job,
                deferred: false,
                always_load: true,
                search_hint: Some("执行或取消 BGI 游戏动作".into()),
                ..ToolExecution::default()
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
            .with_execution(execution),
        )?;
    }
    Ok(())
}
