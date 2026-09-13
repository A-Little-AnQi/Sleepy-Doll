//! 模型这一层的共享类型：消息、工具调用、响应，以及 `Model` 抽象本身。
//! 各协议怎么把它编成请求体、怎么解析回来，见 [`protocol`]。

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    config::{ModelConfig, ModelProtocol},
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
/// 三个协议要求把它带回去，否则下一轮请求被拒：Anthropic 的 `thinking`
/// 块连同 `signature`、Gemini 的 `thoughtSignature`、Responses 的
/// `reasoning` 项连同 `encrypted_content`。另外两个协议（OpenAI Chat、
/// Ollama）只返回可读文本，不需要回传。
///
/// 各协议的块形态与签名语义互不兼容，所以载荷带协议标签；与目标协议不一致
/// 时丢弃整份载荷 —— 丢失推理上下文优于协议错误。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Reasoning {
    pub protocol: ModelProtocol,
    /// 提供方原样载荷，不解析、不改写。
    ///
    /// 用 `Value` 而非定型结构：必须逐字回传，定型结构会丢掉提供方的私有键
    /// （如 `redacted_thinking.data`），丢键即被拒。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Value>,
    /// 界面展示用的可读文本。协议不返回可读推理时为空。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

impl Reasoning {
    /// 载荷能否发给目标协议。协议不符即为否，调用方据此丢弃。
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: Option<String>,
    pub usage: Usage,
    /// 见 [`Reasoning`]。`Deserialize` 需要默认值：调用方可能从裸 JSON 构造。
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

mod protocol;

pub(crate) use protocol::parse_response;

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
                    headers.insert("x-api-key".into(), key.clone());
                    headers
                        .entry("anthropic-version".into())
                        .or_insert_with(|| "2023-06-01".into());
                }
                ModelProtocol::Gemini => {
                    headers.insert("x-goog-api-key".into(), key.clone());
                }
                ModelProtocol::OpenaiResponses | ModelProtocol::OpenaiChat => {
                    headers.insert("authorization".into(), format!("Bearer {key}"));
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

#[cfg(feature = "mock")]
pub mod mock;
