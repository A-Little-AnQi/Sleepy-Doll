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
        self.request("GET", &format!("/bridge/v1/catalog?q={query}"), None, None)
    }
    pub fn describe(&self, method_id: &str) -> Result<Value> {
        self.request(
            "GET",
            &format!("/bridge/v1/catalog/{method_id}"),
            None,
            None,
        )
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
        self.request("POST", "/bridge/v1/invoke", Some(&json!({"requestId":key,"instanceId":self.config.instance_id,"methodId":method_id,"arguments":arguments,"execution":{"onDisconnect":"continue"}})), Some(&key))
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
    let definitions: Vec<BridgeToolDefinition<'_>> = vec![
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
    for (name, description, schema, function) in definitions {
        let execution = if matches!(name, "bgi.capability.invoke" | "bgi.job.cancel") {
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
