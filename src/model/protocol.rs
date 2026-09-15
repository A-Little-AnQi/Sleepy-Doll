//! 五种模型协议的请求体构造与响应解析。

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{Message, ModelResponse, Reasoning, Role, ToolCall, Usage};
use crate::{
    config::{ModelConfig, ModelProtocol},
    error::{Error, Result},
    extension::ToolDefinition,
};
pub(super) fn openai_message(message: &Message) -> Value {
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

pub(super) fn openai_chat_body(
    config: &ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Value {
    let mut body = json!({"model":config.model,"messages":messages.iter().map(openai_message).collect::<Vec<_>>(),"tools":tools.iter().map(tool_schema).collect::<Vec<_>>()});
    if let Some(value) = config.options.temperature {
        body["temperature"] = json!(value);
    }
    if let Some(value) = config.options.max_output_tokens {
        body["max_tokens"] = json!(value);
    }
    body
}

pub(super) fn responses_body(
    config: &ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Value {
    let mut input = Vec::new();
    for message in messages {
        match message.role {
            Role::Tool => input.push(json!({"type":"function_call_output","call_id":message.tool_call_id,"output":message.content})),
            Role::Assistant => {
                // 推理项必须排在同一个 function_call 之前。
                if let Some(reasoning) = message
                    .reasoning
                    .as_ref()
                    .filter(|r| r.matches(ModelProtocol::OpenaiResponses))
                {
                    input.extend(reasoning.blocks.iter().filter_map(replay_reasoning_item));
                }
                if message.tool_calls.is_empty() {
                    input.push(json!({"role":message.role,"content":message.content}));
                } else {
                    if !message.content.is_empty() { input.push(json!({"role":"assistant","content":message.content})); }
                    input.extend(message.tool_calls.iter().map(|call| json!({"type":"function_call","call_id":call.id,"name":call.name,"arguments":call.arguments.to_string()})));
                }
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

pub(super) fn anthropic_body(
    config: &ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Value {
    let system = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut converted: Vec<Value> = Vec::new();
    for message in messages
        .iter()
        .filter(|message| message.role != Role::System)
    {
        let (role, blocks) = if message.role == Role::Tool {
            let is_error = serde_json::from_str::<Value>(&message.content)
                .ok()
                .is_some_and(|value| value["ok"] == false);
            (
                "user",
                vec![json!({
                    "type":"tool_result",
                    "tool_use_id":message.tool_call_id,
                    "content":message.content,
                    "is_error":is_error
                })],
            )
        } else {
            // Anthropic 要求思考块位于同一轮的 text / tool_use 之前，且顺序
            // 必须与模型生成时一致。只回传、不重排。
            //
            // 仅限 assistant 轮：这个分支也服务 user 轮，用户轮出现思考块是
            // 协议错误。协议标签不符的载荷同样丢弃。
            let mut content = match &message.reasoning {
                Some(reasoning)
                    if message.role == Role::Assistant
                        && reasoning.matches(ModelProtocol::AnthropicMessages) =>
                {
                    reasoning.blocks.clone()
                }
                _ => vec![],
            };
            if !message.content.is_empty() {
                content.push(json!({"type":"text","text":message.content}));
            }
            content.extend(message.tool_calls.iter().map(
                |call| json!({"type":"tool_use","id":call.id,"name":call.name,"input":call.arguments}),
            ));
            (
                if message.role == Role::Assistant {
                    "assistant"
                } else {
                    "user"
                },
                content,
            )
        };
        if blocks.is_empty() {
            continue;
        }
        // Anthropic requires alternating roles. Parallel tool results belong to
        // one user message immediately following the assistant tool_use message.
        // Keeping one DB row per tool is useful internally, so normalize only on
        // the provider wire boundary.
        if converted
            .last()
            .is_some_and(|last| last["role"].as_str() == Some(role))
        {
            let content = converted
                .last_mut()
                .and_then(|last| last["content"].as_array_mut())
                .expect("Anthropic message content is always an array");
            if role == "user" && blocks.iter().all(|block| block["type"] == "tool_result") {
                let insert_at = content
                    .iter()
                    .position(|block| block["type"] != "tool_result")
                    .unwrap_or(content.len());
                content.splice(insert_at..insert_at, blocks);
            } else {
                content.extend(blocks);
            }
        } else {
            converted.push(json!({"role":role,"content":blocks}));
        }
    }
    json!({"model":config.model,"system":system,"max_tokens":config.options.max_output_tokens.unwrap_or(8192),"messages":converted,"tools":tools.iter().map(|tool| json!({"name":tool.name,"description":tool.description,"input_schema":tool.input_schema})).collect::<Vec<_>>()})
}

pub(super) fn gemini_body(
    config: &ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Value {
    let system = messages
        .iter()
        .filter(|message| message.role == Role::System)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut contents: Vec<Value> = Vec::new();
    for message in messages
        .iter()
        .filter(|message| message.role != Role::System)
    {
        let role = if message.role == Role::Assistant {
            "model"
        } else {
            "user"
        };
        let parts = if message.role == Role::Tool {
            let name = messages
                .iter()
                .flat_map(|m| m.tool_calls.iter())
                .find(|c| Some(&c.id) == message.tool_call_id.as_ref())
                .map(|c| c.name.as_str())
                .unwrap_or("unknown_tool");
            vec![json!({"functionResponse":{"name":name,"response":{"output":message.content}}})]
        } else {
            let mut parts = if message.content.is_empty() {
                vec![]
            } else {
                vec![json!({"text":message.content})]
            };
            // Gemini 3 起强制校验 thoughtSignature：收到就必须附回**同一个**
            // part，不能合并也不能与 part 分离，否则 400。
            let signatures = message
                .reasoning
                .as_ref()
                .filter(|r| r.matches(ModelProtocol::Gemini))
                .map(|r| {
                    r.blocks
                        .iter()
                        .filter_map(|b| {
                            Some((
                                b["callIndex"].as_u64()? as usize,
                                b["signature"].as_str()?.to_owned(),
                            ))
                        })
                        .collect::<BTreeMap<usize, String>>()
                })
                .unwrap_or_default();
            parts.extend(message.tool_calls.iter().enumerate().map(|(index, call)| {
                let mut part =
                    json!({"functionCall":{"name":call.name,"args":call.arguments,"id":call.id}});
                if let Some(signature) = signatures.get(&index) {
                    part["thoughtSignature"] = json!(signature);
                }
                part
            }));
            parts
        };
        if parts.is_empty() {
            continue;
        }
        // Gemini counts functionResponse parts against the preceding functionCall parts,
        // and they have to sit in ONE user content. One DB row per tool is useful
        // internally, so merge only here on the provider wire boundary - same reason
        // as anthropic_body above.
        if contents
            .last()
            .is_some_and(|last| last["role"].as_str() == Some(role))
        {
            contents
                .last_mut()
                .and_then(|last| last["parts"].as_array_mut())
                .expect("Gemini content parts are always an array")
                .extend(parts);
        } else {
            contents.push(json!({"role":role,"parts":parts}));
        }
    }
    json!({"systemInstruction":{"parts":[{"text":system}]},"contents":contents,"tools":[{"functionDeclarations":tools.iter().map(|tool| json!({"name":tool.name,"description":tool.description,"parameters":tool.input_schema})).collect::<Vec<_>>()}],"generationConfig":{"temperature":config.options.temperature,"maxOutputTokens":config.options.max_output_tokens}})
}

pub(super) fn ollama_body(
    config: &ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
) -> Value {
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

/// 整理一个 reasoning 输出项以便回传。
///
/// 原样保留 `id` 与 `encrypted_content`，只去掉值为 `null` 的键：显式送
/// `"encrypted_content": null` 会被严格的服务端拒绝，正确做法是省略该字段。
///
/// 注意：若将来在请求体里设 `store: false`，服务端不持久化输出项，这里必须
/// 一并剥掉裸 `rs_*` id，否则会以 `Item with id ... not found` 失败。
fn replay_reasoning_item(item: &Value) -> Option<Value> {
    let mut item = item.as_object()?.clone();
    item.retain(|_, value| !value.is_null());
    Some(Value::Object(item))
}

/// 收集 Gemini 的 thought 文本与 `functionCall` 的 `thoughtSignature`。
///
/// 签名按 `functionCall` 在轮内的序号索引，不按 call id：运行时会重写 call id，
/// 而 part 顺序稳定。并行调用时只有第一个 `functionCall` 带签名。
fn gemini_reasoning(parts: &[Value]) -> Option<Reasoning> {
    let mut text = String::new();
    let mut signatures = Vec::new();
    let mut call_index = 0usize;
    for part in parts {
        if part["thought"] == true {
            text.push_str(&string(&part["text"]));
        }
        if part.get("functionCall").is_some() {
            if let Some(signature) = part["thoughtSignature"].as_str() {
                signatures.push(json!({"callIndex": call_index, "signature": signature}));
            }
            call_index += 1;
        }
    }
    if signatures.is_empty() && text.is_empty() {
        return None;
    }
    Some(Reasoning {
        protocol: ModelProtocol::Gemini,
        blocks: signatures,
        text,
    })
}

/// 只收集可读文本，用于界面展示。这两个协议不要求回传推理内容，回传多数
/// 提供方会拒绝。
fn display_reasoning(protocol: ModelProtocol, text: Option<&str>) -> Option<Reasoning> {
    let text = text.unwrap_or_default();
    (!text.is_empty()).then(|| Reasoning {
        protocol,
        blocks: vec![],
        text: text.to_owned(),
    })
}

/// 收集 Responses 的 reasoning 输出项。
///
/// 整项克隆：`encrypted_content` 必须逐字回传，在 `store=false` 下它是推理
/// 上下文唯一的载体。
fn responses_reasoning(output: &[Value]) -> Option<Reasoning> {
    let kept = output
        .iter()
        .filter(|item| item["type"] == "reasoning")
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return None;
    }
    let text = kept
        .iter()
        .flat_map(|item| item["summary"].as_array().into_iter().flatten())
        .map(|part| string(&part["text"]))
        .collect::<String>();
    Some(Reasoning {
        protocol: ModelProtocol::OpenaiResponses,
        blocks: kept,
        text,
    })
}

/// 收集 Anthropic 的思考块。
///
/// 整块克隆，不提取文本：`signature` 必须逐字回传，`redacted_thinking` 的
/// `data` 也是。少一个键、改一个字符，下一轮就是 400。
fn anthropic_reasoning(blocks: &[Value]) -> Option<Reasoning> {
    let kept = blocks
        .iter()
        .filter(|item| {
            matches!(
                item["type"].as_str(),
                Some("thinking" | "redacted_thinking")
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() {
        return None;
    }
    let text = kept
        .iter()
        .filter(|item| item["type"] == "thinking")
        .map(|item| string(&item["thinking"]))
        .collect::<String>();
    Some(Reasoning {
        protocol: ModelProtocol::AnthropicMessages,
        blocks: kept,
        text,
    })
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
                reasoning: display_reasoning(
                    ModelProtocol::OpenaiChat,
                    message["reasoning_content"].as_str(),
                ),
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
                reasoning: responses_reasoning(&output),
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
                reasoning: anthropic_reasoning(&blocks),
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
                reasoning: gemini_reasoning(&parts),
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
                reasoning: display_reasoning(
                    ModelProtocol::OllamaChat,
                    message["thinking"].as_str(),
                ),
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

    #[test]
    fn anthropic_combines_parallel_tool_results_into_one_user_turn() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![
            Message {
                role: Role::User,
                content: "inspect".into(),
                tool_call_id: None,
                tool_calls: vec![],
                reasoning: None,
            },
            Message {
                role: Role::Assistant,
                content: "checking".into(),
                tool_call_id: None,
                tool_calls: vec![
                    ToolCall {
                        id: "a".into(),
                        name: "first".into(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "b".into(),
                        name: "second".into(),
                        arguments: json!({}),
                    },
                ],
                reasoning: None,
            },
            Message {
                role: Role::Tool,
                content: json!({"ok":true}).to_string(),
                tool_call_id: Some("a".into()),
                tool_calls: vec![],
                reasoning: None,
            },
            Message {
                role: Role::Tool,
                content: json!({"ok":false,"error":"failed"}).to_string(),
                tool_call_id: Some("b".into()),
                tool_calls: vec![],
                reasoning: None,
            },
        ];
        let body = anthropic_body(&config, &messages, &[]);
        let wire = body["messages"].as_array().unwrap();
        assert_eq!(wire.len(), 3);
        assert_eq!(wire[2]["role"], "user");
        assert_eq!(wire[2]["content"].as_array().unwrap().len(), 2);
        assert_eq!(wire[2]["content"][0]["tool_use_id"], "a");
        assert_eq!(wire[2]["content"][1]["tool_use_id"], "b");
        assert_eq!(wire[2]["content"][1]["is_error"], true);
    }

    /// Gemini counts functionResponse parts against the preceding functionCall parts,
    /// so parallel tool results have to land in one user content just like Anthropic.
    #[test]
    fn gemini_combines_parallel_tool_results_into_one_user_turn() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"gemini",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: String::new(),
                tool_call_id: None,
                tool_calls: vec![
                    ToolCall {
                        id: "a".into(),
                        name: "first".into(),
                        arguments: json!({}),
                    },
                    ToolCall {
                        id: "b".into(),
                        name: "second".into(),
                        arguments: json!({}),
                    },
                ],
                reasoning: None,
            },
            Message {
                role: Role::Tool,
                content: json!({"ok":true}).to_string(),
                tool_call_id: Some("a".into()),
                tool_calls: vec![],
                reasoning: None,
            },
            Message {
                role: Role::Tool,
                content: json!({"ok":true}).to_string(),
                tool_call_id: Some("b".into()),
                tool_calls: vec![],
                reasoning: None,
            },
        ];
        let body = gemini_body(&config, &messages, &[]);
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "model");
        assert_eq!(contents[0]["parts"].as_array().unwrap().len(), 2);
        assert_eq!(contents[1]["role"], "user");
        let responses = contents[1]["parts"].as_array().unwrap();
        assert_eq!(responses.len(), 2);
        assert_eq!(responses[0]["functionResponse"]["name"], "first");
        assert_eq!(responses[1]["functionResponse"]["name"], "second");
    }

    fn thinking(signature: &str) -> Reasoning {
        Reasoning {
            protocol: ModelProtocol::AnthropicMessages,
            blocks: vec![json!({"type":"thinking","thinking":"先读状态","signature":signature})],
            text: "先读状态".into(),
        }
    }

    /// Anthropic 要求思考块位于同一轮的 text / tool_use 之前。
    #[test]
    fn anthropic_replays_thinking_before_text_and_tools() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: "查一下".into(),
            tool_call_id: None,
            tool_calls: vec![ToolCall {
                id: "a".into(),
                name: "first".into(),
                arguments: json!({}),
            }],
            reasoning: Some(thinking("sig-1")),
        }];
        let body = anthropic_body(&config, &messages, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        let kinds = content
            .iter()
            .map(|block| block["type"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(kinds, ["thinking", "text", "tool_use"]);
        assert_eq!(content[0]["signature"], "sig-1");
    }

    /// `redacted_thinking` 只有不透明的 `data`，必须整块原样回传。
    #[test]
    fn anthropic_replays_redacted_thinking_unchanged() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: Some(Reasoning {
                protocol: ModelProtocol::AnthropicMessages,
                blocks: vec![json!({"type":"redacted_thinking","data":"opaque-payload"})],
                text: String::new(),
            }),
        }];
        let body = anthropic_body(&config, &messages, &[]);
        let block = &body["messages"][0]["content"][0];
        assert_eq!(block["type"], "redacted_thinking");
        assert_eq!(block["data"], "opaque-payload");
    }

    /// 协议标签不符时丢弃：把 Anthropic 的思考块发给别的协议是协议错误。
    #[test]
    fn anthropic_drops_reasoning_from_another_protocol() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: "查一下".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: Some(Reasoning {
                protocol: ModelProtocol::Gemini,
                blocks: vec![json!({"callIndex":0,"signature":"g"})],
                text: String::new(),
            }),
        }];
        let body = anthropic_body(&config, &messages, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
    }

    /// Gemini 的签名必须附回同一个 part，不能合并也不能分离。
    #[test]
    fn gemini_reattaches_thought_signature_to_its_call() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"gemini",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![
                ToolCall {
                    id: "a".into(),
                    name: "first".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "b".into(),
                    name: "second".into(),
                    arguments: json!({}),
                },
            ],
            // 并行调用时只有第一个 functionCall 带签名。
            reasoning: Some(Reasoning {
                protocol: ModelProtocol::Gemini,
                blocks: vec![json!({"callIndex":0,"signature":"gemini-sig"})],
                text: String::new(),
            }),
        }];
        let body = gemini_body(&config, &messages, &[]);
        let parts = body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts[0]["thoughtSignature"], "gemini-sig");
        assert_eq!(parts[0]["functionCall"]["name"], "first");
        assert!(parts[1].get("thoughtSignature").is_none());
    }

    /// Responses 的 reasoning 项原样回传，但不允许出现 `null` 值键。
    #[test]
    fn responses_replays_reasoning_items_without_nulls() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"openai-responses",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![ToolCall {
                id: "a".into(),
                name: "first".into(),
                arguments: json!({}),
            }],
            reasoning: Some(Reasoning {
                protocol: ModelProtocol::OpenaiResponses,
                blocks: vec![json!({
                    "type":"reasoning",
                    "id":"rs_1",
                    "encrypted_content": Value::Null,
                    "summary":[{"type":"summary_text","text":"先读状态"}]
                })],
                text: "先读状态".into(),
            }),
        }];
        let body = responses_body(&config, &messages, &[]);
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["type"], "reasoning");
        assert!(
            input[0].get("encrypted_content").is_none(),
            "不得序列化 null 字段"
        );
        // 推理项必须排在同一个 function_call 之前。
        assert_eq!(input[1]["type"], "function_call");
    }

    /// 全链路：解码真实 SSE 帧 → 组装 → 落库 → 读回 → 组装请求体。
    /// 签名必须逐字节保留：少一个字符、改一个键都会让下一轮 400。
    #[test]
    fn anthropic_reasoning_survives_decode_store_and_replay() {
        use crate::runtime::{gateway::Decoder, store::journal::Journal, types::RunState};

        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(&directory.path().join("test.db")).unwrap();
        let mut run = journal
            .create("go", "c", "round-trip", 1800, None)
            .unwrap();
        journal.save(&mut run, RunState::Deciding).unwrap();

        let mut decoder = Decoder::new(ModelProtocol::AnthropicMessages);
        for frame in [
            json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"先读状态"}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-abc"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"text":"状态正常"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
            json!({"type":"message_stop"}),
        ] {
            decoder.push(frame).unwrap();
        }
        let response = decoder.finish().unwrap();
        let reasoning = response.reasoning.clone().expect("解码未收集推理");
        assert_eq!(reasoning.blocks[0]["signature"], "sig-abc");

        // 与 runtime/mod.rs 落库的路径一致。
        journal
            .append_message(
                &run,
                &Message {
                    role: Role::Assistant,
                    content: response.text.clone(),
                    tool_call_id: None,
                    tool_calls: response.tool_calls.clone(),
                    reasoning: Some(reasoning.clone()),
                },
            )
            .unwrap();

        let history = journal.conversation_messages("c").unwrap();
        let stored = history
            .iter()
            .find(|m| m.role == Role::Assistant)
            .expect("落库后缺少 assistant 轮次");
        assert_eq!(
            stored.reasoning.as_ref(),
            Some(&reasoning),
            "落库往返丢了推理载荷"
        );

        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let body = anthropic_body(&config, &history, &[]);
        let assistant = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "assistant")
            .expect("请求体里缺少 assistant 轮次");
        let content = assistant["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["signature"], "sig-abc");
        assert_eq!(content[0]["thinking"], "先读状态");
        assert_eq!(content[1]["type"], "text");
    }
}
