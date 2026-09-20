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
    let stem = openai_model_stem(&config.model);
    if let Some(value) = config.options.temperature
        && !is_openai_o_series(stem)
    {
        body["temperature"] = json!(value);
    }
    if let Some(value) = config.options.max_output_tokens {
        let field = if uses_max_completion_tokens(stem) {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        body[field] = json!(value);
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
            // Anthropic 要求思考块位于同一轮的 text / tool_use 之前，顺序与模型
            // 生成时一致。思考块只能挂在 assistant 轮，协议标签不符的载荷丢弃。
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
        // Anthropic 要求角色交替：并行的工具结果必须并进紧跟在 assistant
        // tool_use 之后的一条 user 消息。
        if converted
            .last()
            .is_some_and(|last| last["role"].as_str() == Some(role))
        {
            let content = converted
                .last_mut()
                .and_then(|last| last["content"].as_array_mut())
                .expect("Anthropic 消息的 content 恒为数组");
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
    let mut body = json!({"model":config.model,"system":system,"max_tokens":config.options.max_output_tokens.unwrap_or(8192),"messages":converted,"tools":tools.iter().map(|tool| json!({"name":tool.name,"description":tool.description,"input_schema":tool.input_schema})).collect::<Vec<_>>()});
    if config.options.prompt_cache {
        inject_anthropic_cache(&mut body);
    }
    body
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
            // Gemini 3 起强制校验 thoughtSignature：收到就必须附回同一个 part，
            // 不能合并也不能与 part 分离，否则 400。
            let signatures = message
                .reasoning
                .as_ref()
                .filter(|r| r.matches(ModelProtocol::Gemini))
                .map(|r| {
                    r.blocks
                        .iter()
                        .filter_map(|b| {
                            let signature = b["signature"]
                                .as_str()
                                .or_else(|| b["thoughtSignature"].as_str())
                                .or_else(|| b["thought_signature"].as_str())?;
                            Some((b["callIndex"].as_u64()? as usize, signature.to_owned()))
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
        // Gemini 按前一条 functionCall 计数 functionResponse：并行的工具结果
        // 必须落在同一条 user content 里。
        if contents
            .last()
            .is_some_and(|last| last["role"].as_str() == Some(role))
        {
            contents
                .last_mut()
                .and_then(|last| last["parts"].as_array_mut())
                .expect("Gemini content 的 parts 恒为数组")
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

fn openai_model_stem(model: &str) -> &str {
    model.rsplit('/').next().unwrap_or(model)
}

/// o1 / o3 / o4：官方 Chat Completions 拒绝 `max_tokens` 和 `temperature`。
fn is_openai_o_series(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.len() > 1
        && model.starts_with('o')
        && model.as_bytes().get(1).is_some_and(|b| b.is_ascii_digit())
}

fn uses_max_completion_tokens(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    is_openai_o_series(&model) || model.starts_with("gpt-5")
}

/// Anthropic 提示缓存：最多 4 个断点，标在 tools 末、system 末、最新非 thinking
/// 块，长对话再标上一条更早的 user。`cache_control` 不能进 openai-chat：
/// Kimi / NIM / Qwen 会因它直接 400。
fn inject_anthropic_cache(body: &mut Value) {
    let existing = count_cache_breakpoints(body);
    let mut budget = 4usize.saturating_sub(existing);
    if budget == 0 {
        return;
    }
    if let Some(last) = body
        .get_mut("tools")
        .and_then(Value::as_array_mut)
        .and_then(|tools| tools.last_mut())
        && set_ephemeral_cache(last)
    {
        budget -= 1;
    }
    if budget > 0 {
        if let Some(text) = body
            .get("system")
            .and_then(Value::as_str)
            .map(str::to_owned)
            && !text.is_empty()
        {
            body["system"] = json!([{"type": "text", "text": text}]);
        }
        if let Some(last) = body
            .get_mut("system")
            .and_then(Value::as_array_mut)
            .and_then(|system| system.last_mut())
            && set_ephemeral_cache(last)
        {
            budget -= 1;
        }
    }
    if budget == 0 {
        return;
    }
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages.iter_mut().rev() {
        if inject_message_breakpoint(message) {
            budget -= 1;
            break;
        }
    }
    if budget == 0 || messages.len() < 4 {
        return;
    }
    let mut user_count = 0;
    for message in messages.iter_mut().rev() {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        user_count += 1;
        if user_count == 2 {
            inject_message_breakpoint(message);
            break;
        }
    }
}

fn set_ephemeral_cache(value: &mut Value) -> bool {
    if value.get("cache_control").is_some() {
        return false;
    }
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    object.insert("cache_control".into(), json!({"type": "ephemeral"}));
    true
}

fn inject_message_breakpoint(message: &mut Value) -> bool {
    let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) else {
        return false;
    };
    let Some(block) = content.iter_mut().rev().find(|block| {
        !matches!(
            block.get("type").and_then(Value::as_str),
            Some("thinking" | "redacted_thinking")
        )
    }) else {
        return false;
    };
    set_ephemeral_cache(block)
}

fn count_cache_breakpoints(body: &Value) -> usize {
    let mut count = 0;
    if let Some(tools) = body.get("tools").and_then(Value::as_array) {
        count += tools
            .iter()
            .filter(|t| t.get("cache_control").is_some())
            .count();
    }
    if let Some(system) = body.get("system").and_then(Value::as_array) {
        count += system
            .iter()
            .filter(|b| b.get("cache_control").is_some())
            .count();
    }
    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        for message in messages {
            if let Some(content) = message.get("content").and_then(Value::as_array) {
                count += content
                    .iter()
                    .filter(|b| b.get("cache_control").is_some())
                    .count();
            }
        }
    }
    count
}

pub(crate) fn usage_from_openai_chat(node: &Value) -> Usage {
    Usage {
        input_tokens: node["prompt_tokens"].as_u64(),
        output_tokens: node["completion_tokens"].as_u64(),
        cache_read_tokens: node["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .or_else(|| node["prompt_tokens_details"]["cachedTokens"].as_u64()),
        cache_write_tokens: None,
    }
}

pub(crate) fn usage_from_openai_responses(node: &Value) -> Usage {
    Usage {
        input_tokens: node["input_tokens"].as_u64(),
        output_tokens: node["output_tokens"].as_u64(),
        cache_read_tokens: node["input_tokens_details"]["cached_tokens"]
            .as_u64()
            .or_else(|| node["cache_read_input_tokens"].as_u64()),
        cache_write_tokens: node["cache_creation_input_tokens"].as_u64(),
    }
}

pub(crate) fn usage_from_anthropic(node: &Value) -> Usage {
    Usage {
        input_tokens: node["input_tokens"].as_u64(),
        output_tokens: node["output_tokens"].as_u64(),
        cache_read_tokens: node["cache_read_input_tokens"].as_u64(),
        cache_write_tokens: node["cache_creation_input_tokens"].as_u64(),
    }
}

pub(crate) fn usage_from_gemini(node: &Value) -> Usage {
    Usage {
        input_tokens: node["promptTokenCount"].as_u64(),
        output_tokens: node["candidatesTokenCount"].as_u64(),
        cache_read_tokens: node["cachedContentTokenCount"].as_u64(),
        cache_write_tokens: None,
    }
}

pub(crate) fn usage_from_ollama(raw: &Value) -> Usage {
    Usage {
        input_tokens: raw["prompt_eval_count"].as_u64(),
        output_tokens: raw["eval_count"].as_u64(),
        cache_read_tokens: None,
        cache_write_tokens: None,
    }
}

fn parse_arguments(value: &Value) -> Result<Value> {
    if value.is_object() {
        return Ok(value.clone());
    }
    if let Some(text) = value.as_str() {
        return serde_json::from_str(text)
            .map_err(|_| Error::ModelProtocol("模型返回的工具参数不合法".into()));
    }
    Ok(json!({}))
}

/// 整理一个 reasoning 输出项以便回传。
///
/// 原样保留 `id` 与 `encrypted_content`，只去掉值为 `null` 的键：显式送
/// `"encrypted_content": null` 会被严格的服务端拒绝，正确做法是省略该字段。
fn replay_reasoning_item(item: &Value) -> Option<Value> {
    let mut item = item.as_object()?.clone();
    item.retain(|_, value| !value.is_null());
    Some(Value::Object(item))
}

/// 收集 Gemini 的 thought 文本与 `functionCall` 的 `thoughtSignature`。
///
/// 签名按 `functionCall` 在轮内的序号索引：运行时会重写 call id，part 顺序稳定。
/// 并行调用时只有第一个 `functionCall` 带签名。
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

/// 只收集可读文本，用于界面展示。这两个协议不要求回传推理内容，回传会被多数
/// 提供方拒绝。
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
/// 整项克隆：`encrypted_content` 必须逐字回传，在 `store=false` 下它是推理上下文
/// 唯一的载体。
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
/// 整块克隆，不提取文本：`signature` 与 `redacted_thinking` 的 `data` 都必须逐字
/// 回传，少一个键、改一个字符，下一轮就是 400。
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
                .ok_or_else(|| Error::ModelProtocol("OpenAI 响应里没有 choices".into()))?;
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
                usage: usage_from_openai_chat(&raw["usage"]),
                reasoning: display_reasoning(
                    ModelProtocol::OpenaiChat,
                    message["reasoning_content"]
                        .as_str()
                        .or_else(|| message["reasoning"].as_str()),
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
                finish_reason: if raw["status"] == "incomplete" {
                    Some(
                        raw["incomplete_details"]["reason"]
                            .as_str()
                            .unwrap_or("max_output_tokens")
                            .to_owned(),
                    )
                } else {
                    raw["status"].as_str().map(str::to_owned)
                },
                usage: usage_from_openai_responses(&raw["usage"]),
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
                usage: usage_from_anthropic(&raw["usage"]),
                reasoning: anthropic_reasoning(&blocks),
            })
        }
        ModelProtocol::Gemini => {
            let candidate = raw["candidates"]
                .as_array()
                .and_then(|items| items.first())
                .ok_or_else(|| Error::ModelProtocol("Gemini 响应里没有 candidates".into()))?;
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
                usage: usage_from_gemini(&raw["usageMetadata"]),
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
                usage: usage_from_ollama(raw),
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

    /// Gemini 与 Anthropic 一样，并行的工具结果要落进同一条 user content。
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

    /// 协议标签不符时丢弃。
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

    /// 全链路：解码真实 SSE 帧 → 组装 → 落库 → 读回 → 组装请求体
    /// 签名必须逐字节保留，否则下一轮 400。
    #[test]
    fn anthropic_reasoning_survives_decode_store_and_replay() {
        use crate::runtime::{gateway::Decoder, store::journal::Journal, types::RunState};

        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open(&directory.path().join("test.db")).unwrap();
        let mut run = journal.create("go", "c", "round-trip", 1800, None).unwrap();
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
            .find(|entry| entry.message.role == Role::Assistant)
            .expect("落库后缺少 assistant 轮次");
        assert_eq!(
            stored.message.reasoning.as_ref(),
            Some(&reasoning),
            "落库往返丢了推理载荷"
        );
        assert!(!stored.created_at.is_empty(), "对话读取丢了消息时间");

        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = history
            .iter()
            .map(|entry| entry.message.clone())
            .collect::<Vec<_>>();
        let body = anthropic_body(&config, &messages, &[]);
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

    fn tool(name: &str) -> crate::extension::ToolDefinition {
        crate::extension::ToolDefinition {
            name: name.into(),
            label: String::new(),
            description: name.into(),
            input_schema: json!({"type":"object","properties":{}}),
            output_schema: None,
            source: "test".into(),
            provider_version: None,
            execution: Default::default(),
        }
    }

    #[test]
    fn anthropic_injects_prompt_cache_breakpoints() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![
            Message {
                role: Role::System,
                content: "sys".into(),
                tool_call_id: None,
                tool_calls: vec![],
                reasoning: None,
            },
            Message {
                role: Role::User,
                content: "hi".into(),
                tool_call_id: None,
                tool_calls: vec![],
                reasoning: None,
            },
        ];
        let body = anthropic_body(&config, &messages, &[tool("a"), tool("b")]);
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral");
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(
            body["messages"][0]["content"][0]["cache_control"]["type"],
            "ephemeral"
        );
        assert_eq!(count_cache_breakpoints(&body), 3);
    }

    #[test]
    fn anthropic_cache_skips_thinking_blocks() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost"
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::Assistant,
            content: "result".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: Some(thinking("sig")),
        }];
        let body = anthropic_body(&config, &messages, &[]);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert!(content[0].get("cache_control").is_none());
        assert_eq!(content[1]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn prompt_cache_can_be_disabled() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"anthropic-messages",
            "model":"test","baseUrl":"http://localhost",
            "options":{"promptCache":false}
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::User,
            content: "hi".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: None,
        }];
        let body = anthropic_body(&config, &messages, &[tool("a")]);
        assert!(body["system"].is_string());
        assert!(body["tools"][0].get("cache_control").is_none());
        assert!(
            body["messages"][0]["content"][0]
                .get("cache_control")
                .is_none()
        );
    }

    #[test]
    fn openai_chat_never_emits_cache_control() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"openai-chat",
            "model":"kimi-k2","baseUrl":"http://localhost",
            "options":{"maxOutputTokens":1024,"temperature":0.2}
        }))
        .unwrap();
        let messages = vec![Message {
            role: Role::User,
            content: "hi".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: None,
        }];
        let body = openai_chat_body(&config, &messages, &[tool("a")]);
        let encoded = body.to_string();
        assert!(!encoded.contains("cache_control"), "{encoded}");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["temperature"], 0.2);
    }

    #[test]
    fn openai_o_series_uses_max_completion_tokens_and_drops_temperature() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"openai-chat",
            "model":"o3-mini","baseUrl":"http://localhost",
            "options":{"maxOutputTokens":2048,"temperature":0.7}
        }))
        .unwrap();
        let body = openai_chat_body(&config, &[], &[]);
        assert_eq!(body["max_completion_tokens"], 2048);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn openai_gpt5_uses_max_completion_tokens() {
        let config: ModelConfig = serde_json::from_value(json!({
            "id":"test","name":"test","protocol":"openai-chat",
            "model":"gpt-5.2","baseUrl":"http://localhost",
            "options":{"maxOutputTokens":1024}
        }))
        .unwrap();
        let body = openai_chat_body(&config, &[], &[]);
        assert_eq!(body["max_completion_tokens"], 1024);
    }

    #[test]
    fn parses_cache_tokens_from_each_provider() {
        let openai = parse_response(
            ModelProtocol::OpenaiChat,
            &json!({"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
                "usage":{"prompt_tokens":100,"completion_tokens":10,
                    "prompt_tokens_details":{"cached_tokens":80}}}),
        )
        .unwrap();
        assert_eq!(openai.usage.cache_read_tokens, Some(80));
        assert_eq!(
            openai.usage.context_tokens(ModelProtocol::OpenaiChat),
            Some(100)
        );

        let anthropic = parse_response(
            ModelProtocol::AnthropicMessages,
            &json!({"content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn",
                "usage":{"input_tokens":20,"output_tokens":5,
                    "cache_read_input_tokens":80,"cache_creation_input_tokens":10}}),
        )
        .unwrap();
        assert_eq!(anthropic.usage.cache_read_tokens, Some(80));
        assert_eq!(
            anthropic
                .usage
                .context_tokens(ModelProtocol::AnthropicMessages),
            Some(110)
        );

        let gemini = parse_response(
            ModelProtocol::Gemini,
            &json!({"candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}],
                "usageMetadata":{"promptTokenCount":90,"candidatesTokenCount":4,
                    "cachedContentTokenCount":70}}),
        )
        .unwrap();
        assert_eq!(gemini.usage.cache_read_tokens, Some(70));
    }
}
