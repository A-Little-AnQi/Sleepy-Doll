//! 模型这一层的共享类型：消息、工具调用、响应，以及 `Model` 抽象本身。
//! 各协议的编码与解析见 [`protocol`]。

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{ModelAuth, ModelConfig, ModelProtocol},
    error::{Error, Result},
    extension::ToolDefinition,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// 模型返回的推理载荷，原样回传给产出它的协议。
///
/// Anthropic 的 `thinking` 块连同 `signature`、Gemini 的 `thoughtSignature`、
/// Responses 的 `reasoning` 项连同 `encrypted_content` 都必须带回，否则下一轮
/// 被拒；OpenAI Chat 与 Ollama 只返回可读文本，不需要回传。
///
/// 各协议的块形态与签名语义互不兼容，所以载荷带协议标签。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reasoning {
    pub protocol: ModelProtocol,
    /// 提供方原样载荷，不解析、不改写。
    ///
    /// 用 `Value` 而非定型结构：必须逐字回传，定型结构会丢掉提供方的私有键
    /// （如 `redacted_thinking.data`），丢键会被服务端拒绝。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Value>,
    /// 界面展示用的可读文本。协议不返回可读推理时为空。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

impl Reasoning {
    /// 载荷能否发给目标协议。
    pub fn matches(&self, protocol: ModelProtocol) -> bool {
        self.protocol == protocol
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
}

impl Usage {
    pub fn merge(&mut self, incoming: Usage) {
        if incoming.input_tokens.is_some() {
            self.input_tokens = incoming.input_tokens;
        }
        if incoming.output_tokens.is_some() {
            self.output_tokens = incoming.output_tokens;
        }
        if incoming.cache_read_tokens.is_some() {
            self.cache_read_tokens = incoming.cache_read_tokens;
        }
        if incoming.cache_write_tokens.is_some() {
            self.cache_write_tokens = incoming.cache_write_tokens;
        }
    }

    /// 当前上下文占用。Anthropic 的 `input_tokens` 不含缓存命中，要加回去才能
    /// 和窗口比较；OpenAI / Gemini 的 prompt 计数已经含缓存。
    pub fn context_tokens(&self, protocol: ModelProtocol) -> Option<u64> {
        let input = self.input_tokens?;
        Some(match protocol {
            ModelProtocol::AnthropicMessages => {
                input + self.cache_read_tokens.unwrap_or(0) + self.cache_write_tokens.unwrap_or(0)
            }
            _ => input,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Usage,
    /// 见 [`Reasoning`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
}

pub trait Model: Send + Sync {
    fn complete(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<ModelResponse>;
    fn config(&self) -> &ModelConfig;
}

pub trait JsonTransport: Send + Sync {
    fn post(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &Value,
        timeout: Duration,
    ) -> Result<Value>;
}

pub struct UreqTransport;

impl JsonTransport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body: &Value,
        timeout: Duration,
    ) -> Result<Value> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .build()
            .new_agent();
        let mut request = agent.post(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        let mut response = request.send_json(body)?;
        response.body_mut().read_json().map_err(Error::from)
    }
}

mod catalog;
mod protocol;

pub use catalog::list_remote_models;

pub(crate) use protocol::{
    parse_response, usage_from_anthropic, usage_from_gemini, usage_from_ollama,
    usage_from_openai_chat,
};

pub struct ProtocolModel {
    config: ModelConfig,
    transport: Arc<dyn JsonTransport>,
}

impl ProtocolModel {
    pub fn new(config: ModelConfig) -> Self {
        Self {
            config,
            transport: Arc::new(UreqTransport),
        }
    }
    pub fn with_transport(config: ModelConfig, transport: Arc<dyn JsonTransport>) -> Self {
        Self { config, transport }
    }

    pub(crate) fn endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        match self.config.protocol {
            ModelProtocol::OpenaiResponses => format!("{base}/responses"),
            ModelProtocol::OpenaiChat => format!("{base}/chat/completions"),
            ModelProtocol::AnthropicMessages => format!("{base}/messages"),
            ModelProtocol::Gemini => format!("{base}/models/{}:generateContent", self.config.model),
            ModelProtocol::OllamaChat => format!("{base}/api/chat"),
        }
    }

    pub(crate) fn headers(&self) -> HashMap<String, String> {
        let mut headers = self.config.headers.clone();
        if let Some(key) = &self.config.api_key {
            match self.config.protocol {
                ModelProtocol::AnthropicMessages => {
                    if anthropic_uses_bearer(self.config.auth, key) {
                        headers
                            .entry("authorization".into())
                            .or_insert_with(|| format!("Bearer {key}"));
                    } else {
                        headers
                            .entry("x-api-key".into())
                            .or_insert_with(|| key.clone());
                    }
                    headers
                        .entry("anthropic-version".into())
                        .or_insert_with(|| "2023-06-01".into());
                }
                ModelProtocol::Gemini => {
                    if self.config.auth == ModelAuth::Bearer {
                        headers
                            .entry("authorization".into())
                            .or_insert_with(|| format!("Bearer {key}"));
                    } else {
                        headers
                            .entry("x-goog-api-key".into())
                            .or_insert_with(|| key.clone());
                    }
                }
                ModelProtocol::OpenaiResponses | ModelProtocol::OpenaiChat => {
                    headers
                        .entry("authorization".into())
                        .or_insert_with(|| format!("Bearer {key}"));
                }
                ModelProtocol::OllamaChat => {}
            }
        }
        headers
    }

    pub(crate) fn body(&self, messages: &[Message], tools: &[ToolDefinition]) -> Value {
        match self.config.protocol {
            ModelProtocol::OpenaiChat => protocol::openai_chat_body(&self.config, messages, tools),
            ModelProtocol::OpenaiResponses => {
                protocol::responses_body(&self.config, messages, tools)
            }
            ModelProtocol::AnthropicMessages => {
                protocol::anthropic_body(&self.config, messages, tools)
            }
            ModelProtocol::Gemini => protocol::gemini_body(&self.config, messages, tools),
            ModelProtocol::OllamaChat => protocol::ollama_body(&self.config, messages, tools),
        }
    }
}

impl Model for ProtocolModel {
    fn complete(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<ModelResponse> {
        let raw = self.transport.post(
            &self.endpoint(),
            &self.headers(),
            &self.body(messages, tools),
            Duration::from_millis(self.config.options.timeout_ms),
        )?;
        protocol::parse_response(self.config.protocol, &raw)
    }
    fn config(&self) -> &ModelConfig {
        &self.config
    }
}

fn anthropic_uses_bearer(auth: ModelAuth, key: &str) -> bool {
    match auth {
        ModelAuth::Bearer => true,
        ModelAuth::ApiKey => false,
        // 官方密钥以 `sk-ant-` 开头；中转多为 `sk-` 或任意 token，走 Bearer。
        ModelAuth::Auto => !key.starts_with("sk-ant-"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anthropic(auth: ModelAuth, key: &str) -> ProtocolModel {
        ProtocolModel::new(ModelConfig {
            id: "t".into(),
            name: "t".into(),
            protocol: ModelProtocol::AnthropicMessages,
            model: "claude".into(),
            base_url: "http://localhost".into(),
            api_key: Some(key.into()),
            auth,
            headers: HashMap::new(),
            options: Default::default(),
        })
    }

    #[test]
    fn anthropic_official_key_uses_x_api_key() {
        let headers = anthropic(ModelAuth::Auto, "sk-ant-official").headers();
        assert_eq!(
            headers.get("x-api-key").map(String::as_str),
            Some("sk-ant-official")
        );
        assert!(!headers.contains_key("authorization"));
    }

    #[test]
    fn anthropic_gateway_token_uses_bearer() {
        let headers = anthropic(ModelAuth::Auto, "sk-gateway").headers();
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer sk-gateway")
        );
        assert!(!headers.contains_key("x-api-key"));
    }

    #[test]
    fn anthropic_auth_can_be_forced() {
        let headers = anthropic(ModelAuth::Bearer, "sk-ant-official").headers();
        assert_eq!(
            headers.get("authorization").map(String::as_str),
            Some("Bearer sk-ant-official")
        );
        let headers = anthropic(ModelAuth::ApiKey, "sk-gateway").headers();
        assert_eq!(
            headers.get("x-api-key").map(String::as_str),
            Some("sk-gateway")
        );
    }
}
