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
