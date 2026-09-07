use std::{collections::HashMap, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    config::{ModelConfig, ModelProtocol},
    error::{Error, Result},
    tools::ToolDefinition,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
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
            ModelProtocol::OpenaiChat => openai_chat_body(&self.config, messages, tools),
            ModelProtocol::OpenaiResponses => responses_body(&self.config, messages, tools),
            ModelProtocol::AnthropicMessages => anthropic_body(&self.config, messages, tools),
            ModelProtocol::Gemini => gemini_body(&self.config, messages, tools),
            ModelProtocol::OllamaChat => ollama_body(&self.config, messages, tools),
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
        parse_response(self.config.protocol, &raw)
    }
    fn config(&self) -> &ModelConfig {
        &self.config
    }
}

fn openai_message(message: &Message) -> Value {
    match message.role {
        Role::Tool => {
            json!({"role":"tool","tool_call_id":message.tool_call_id,"content":message.content})
        }
        Role::Assistant if !message.tool_calls.is_empty() => json!({
            "role":"assistant", "content": if message.content.is_empty() { Value::Null } else { json!(message.content) },
            "tool_calls": message.tool_calls.iter().map(|call| json!({"id":call.id,"type":"function","function":{"name":call.name,"arguments":call.arguments.to_string()}})).collect::<Vec<_>>()
        }),
        _ => json!({"role":message.role,"content":message.content}),
    }
}

fn tool_schema(tool: &ToolDefinition) -> Value {
    json!({"type":"function","function":{"name":tool.name,"description":tool.description,"parameters":tool.input_schema}})
}

fn openai_chat_body(config: &ModelConfig, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut body = json!({"model":config.model,"messages":messages.iter().map(openai_message).collect::<Vec<_>>(),"tools":tools.iter().map(tool_schema).collect::<Vec<_>>()});
    if let Some(value) = config.options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = config.options.max_output_tokens {
        body["max_tokens"] = json!(value);
    }
    body
}

fn responses_body(config: &ModelConfig, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let mut input = Vec::new();
    for message in messages {
        match message.role {
            Role::Tool => input.push(json!({"type":"function_call_output","call_id":message.tool_call_id,"output":message.content})),
            Role::Assistant if !message.tool_calls.is_empty() => {
                if !message.content.is_empty() { input.push(json!({"role":"assistant","content":message.content})); }
                input.extend(message.tool_calls.iter().map(|call| json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.arguments.to_string()})));
            }
            _ => input.push(json!({"role":message.role,"content":message.content})),
        }
    }
    let response_tools = tools.iter().map(|tool| json!({"type":"function","name":tool.name,"description":tool.description,"parameters":tool.input_schema})).collect::<Vec<_>>();
    let mut body = json!({"model":config.model,"input":input,"tools":response_tools});
    if let Some(value) = config.options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = config.options.max_output_tokens {
        body["max_output_tokens"] = json!(value);
    }
    if let Some(value) = &config.options.reasoning_effort {
        body["reasoning"] = json!({"effort":value});
    }
    body
}

fn anthropic_body(config: &ModelConfig, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let system = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let converted = messages.iter().filter(|message| message.role != Role::System).map(|message| {
        if message.role == Role::Tool { json!({"role":"user","content":[{"type":"tool_result","tool_use_id":message.tool_call_id,"content":message.content}]}) }
        else {
            let mut content = if message.content.is_empty() { vec![] } else { vec![json!({"type":"text","text":message.content})] };
            content.extend(message.tool_calls.iter().map(|call| json!({"type":"tool_use","id":call.id,"name":call.name,"input":call.arguments})));
            json!({"role":message.role,"content":content})
        }
    }).collect::<Vec<_>>();
    json!({"model":config.model,"system":system,"max_tokens":config.options.max_output_tokens.unwrap_or(8192),"messages":converted,"tools":tools.iter().map(|tool| json!({"name":tool.name,"description":tool.description,"input_schema":tool.input_schema})).collect::<Vec<_>>()})
}

fn gemini_body(config: &ModelConfig, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    let system = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let contents = messages.iter().filter(|message| message.role != Role::System).map(|message| {
        let mut parts = if message.content.is_empty() { vec![] } else { vec![json!({"text":message.content})] };
        if message.role == Role::Tool {
            let name=messages.iter().flat_map(|m|m.tool_calls.iter()).find(|c|Some(&c.id)==message.tool_call_id.as_ref()).map(|c|c.name.as_str()).unwrap_or("unknown_tool");
            parts = vec![json!({"functionResponse":{"name":name,"response":{"output":message.content}}})];
        }
        else { parts.extend(message.tool_calls.iter().map(|call| json!({"functionCall":{"name":call.name,"args":call.arguments,"id":call.id}}))); }
        json!({"role":if message.role == Role::Assistant {"model"} else {"user"},"parts":parts})
    }).collect::<Vec<_>>();
    json!({"systemInstruction":{"parts":[{"text":system}]},"contents":contents,"tools":[{"functionDeclarations":tools.iter().map(|tool| json!({"name":tool.name,"description":tool.description,"parameters":tool.input_schema})).collect::<Vec<_>>()}],"generationConfig":{"temperature":config.options.temperature,"maxOutputTokens":config.options.max_output_tokens}})
}

fn ollama_body(config: &ModelConfig, messages: &[Message], tools: &[ToolDefinition]) -> Value {
    json!({"model":config.model,"stream":false,"messages":messages.iter().map(openai_message).collect::<Vec<_>>(),"tools":tools.iter().map(tool_schema).collect::<Vec<_>>(),"options":{"temperature":config.options.temperature,"num_predict":config.options.max_output_tokens}})
}

fn parse_arguments(value: &Value) -> Result<Value> {
    if value.is_object() {
        return Ok(value.clone());
    }
    if let Some(text) = value.as_str() {
        return serde_json::from_str(text)
            .map_err(|_| Error::ModelProtocol("model returned invalid tool arguments".into()));
    }
    Ok(json!({}))
}

pub(crate) fn parse_response(protocol: ModelProtocol, raw: &Value) -> Result<ModelResponse> {
    match protocol {
        ModelProtocol::OpenaiChat => {
            let choice = raw["choices"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or_else(|| Error::ModelProtocol("OpenAI response has no choices".into()))?;
            let message = &choice["message"];
            let calls = message["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|call| {
                    Ok(ToolCall {
                        id: string(&call["id"]),
                        name: string(&call["function"]["name"]),
                        arguments: parse_arguments(&call["function"]["arguments"])?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ModelResponse {
                text: string(&message["content"]),
                tool_calls: calls,
                finish_reason: choice["finish_reason"].as_str().map(str::to_owned),
                usage: Usage {
                    input_tokens: raw["usage"]["prompt_tokens"].as_u64(),
                    output_tokens: raw["usage"]["completion_tokens"].as_u64(),
                },
            })
        }
        ModelProtocol::OpenaiResponses => {
            let output = raw["output"].as_array().cloned().unwrap_or_default();
            let text = output
                .iter()
                .filter(|item| item["type"] == "message")
                .flat_map(|item| item["content"].as_array().into_iter().flatten())
                .filter(|item| item["type"] == "output_text")
                .map(|item| string(&item["text"]))
                .collect::<String>();
            let calls = output
                .iter()
                .filter(|item| item["type"] == "function_call")
                .map(|call| {
                    Ok(ToolCall {
                        id: string(&call["call_id"]),
                        name: string(&call["name"]),
                        arguments: parse_arguments(&call["arguments"])?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(ModelResponse {
                text,
                tool_calls: calls,
                finish_reason: raw["status"].as_str().map(str::to_owned),
                usage: Usage {
                    input_tokens: raw["usage"]["input_tokens"].as_u64(),
                    output_tokens: raw["usage"]["output_tokens"].as_u64(),
                },
            })
        }
        ModelProtocol::AnthropicMessages => {
            let blocks = raw["content"].as_array().cloned().unwrap_or_default();
            let text = blocks
                .iter()
                .filter(|item| item["type"] == "text")
                .map(|item| string(&item["text"]))
                .collect::<String>();
            let calls = blocks
                .iter()
                .filter(|item| item["type"] == "tool_use")
                .map(|call| ToolCall {
                    id: string(&call["id"]),
                    name: string(&call["name"]),
                    arguments: call["input"].clone(),
                })
                .collect();
            Ok(ModelResponse {
                text,
                tool_calls: calls,
                finish_reason: raw["stop_reason"].as_str().map(str::to_owned),
                usage: Usage {
                    input_tokens: raw["usage"]["input_tokens"].as_u64(),
                    output_tokens: raw["usage"]["output_tokens"].as_u64(),
                },
            })
        }
        ModelProtocol::Gemini => {
            let candidate = raw["candidates"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or_else(|| Error::ModelProtocol("Gemini response has no candidates".into()))?;
            let parts = candidate["content"]["parts"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let text = parts
                .iter()
                .filter(|item| item["thought"] != true)
                .map(|item| string(&item["text"]))
                .collect::<String>();
            let calls = parts
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    item.get("functionCall").map(|call| ToolCall {
                        id: call["id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("gemini-call-{index}")),
                        name: string(&call["name"]),
                        arguments: call["args"].clone(),
                    })
                })
                .collect();
            Ok(ModelResponse {
                text,
                tool_calls: calls,
                finish_reason: candidate["finishReason"].as_str().map(str::to_owned),
                usage: Usage {
                    input_tokens: raw["usageMetadata"]["promptTokenCount"].as_u64(),
                    output_tokens: raw["usageMetadata"]["candidatesTokenCount"].as_u64(),
                },
            })
        }
        ModelProtocol::OllamaChat => {
            let message = &raw["message"];
            let calls = message["tool_calls"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(index, call)| ToolCall {
                    id: format!("ollama-call-{index}"),
                    name: string(&call["function"]["name"]),
                    arguments: call["function"]["arguments"].clone(),
                })
                .collect();
            Ok(ModelResponse {
                text: string(&message["content"]),
                tool_calls: calls,
                finish_reason: raw["done_reason"].as_str().map(str::to_owned),
                usage: Usage {
                    input_tokens: raw["prompt_eval_count"].as_u64(),
                    output_tokens: raw["eval_count"].as_u64(),
                },
            })
        }
    }
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_tool_call() {
        let raw = json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"c1","function":{"name":"bgi.state.get","arguments":"{}"}}]},"finish_reason":"tool_calls"}]});
        let response = parse_response(ModelProtocol::OpenaiChat, &raw).unwrap();
        assert_eq!(response.tool_calls[0].name, "bgi.state.get");
    }

    #[test]
    fn rejects_malformed_tool_arguments() {
        let raw = json!({"choices":[{"message":{"tool_calls":[{"id":"c1","function":{"name":"x","arguments":"{"}}]}}]});
        assert!(parse_response(ModelProtocol::OpenaiChat, &raw).is_err());
    }
}
