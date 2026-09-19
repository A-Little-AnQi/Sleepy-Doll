use crate::{
    config::{ModelConfig, ModelProtocol},
    error::{Error, Result},
    extension::ToolDefinition,
    model::{
        Message, ModelResponse, ProtocolModel, Reasoning, ToolCall, Usage, parse_response,
        usage_from_anthropic, usage_from_gemini, usage_from_ollama, usage_from_openai_chat,
    },
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

/// 每一次模型调用都从这里过，请求与结果的记录放在这一层。
pub async fn complete(
    config: ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
    cancel: &CancellationToken,
    emit: impl FnMut(&str) -> Result<()>,
) -> Result<ModelResponse> {
    let started = std::time::Instant::now();
    let logged = format!("{}（{}）", config.model, config.protocol);
    log::info!(
        "请求模型 {logged}：{} 条消息，{} 个工具",
        messages.len(),
        tools.len()
    );
    let result = complete_inner(config, messages, tools, cancel, emit).await;
    match &result {
        Ok(response) => log::info!(
            "模型 {logged} 返回：{} 字，{} 个调用，输入 {} / 输出 {} token，{:.1}s",
            response.text.chars().count(),
            response.tool_calls.len(),
            response.usage.input_tokens.unwrap_or(0),
            response.usage.output_tokens.unwrap_or(0),
            started.elapsed().as_secs_f64()
        ),
        Err(error) => log::warn!(
            "模型 {logged} 失败（{:.1}s）：{error}",
            started.elapsed().as_secs_f64()
        ),
    }
    result
}

async fn complete_inner(
    mut config: ModelConfig,
    messages: &[Message],
    tools: &[ToolDefinition],
    cancel: &CancellationToken,
    mut emit: impl FnMut(&str) -> Result<()>,
) -> Result<ModelResponse> {
    config.options.max_output_tokens.get_or_insert(8192);
    let model = ProtocolModel::new(config.clone());
    let names = tools
        .iter()
        .map(|t| (wire_name(&t.name), t.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let wire_tools = tools
        .iter()
        .map(|t| {
            let mut t = t.clone();
            t.name = wire_name(&t.name);
            t
        })
        .collect::<Vec<_>>();
    let wire_messages = messages
        .iter()
        .map(|m| {
            let mut m = m.clone();
            for c in &mut m.tool_calls {
                c.name = wire_name(&c.name);
            }
            m
        })
        .collect::<Vec<_>>();
    let mut body = model.body(&wire_messages, &wire_tools);
    let mut endpoint = model.endpoint();
    if config.protocol == ModelProtocol::Gemini {
        endpoint = endpoint.replace(":generateContent", ":streamGenerateContent?alt=sse");
    } else {
        body["stream"] = json!(true);
    }
    if config.protocol == ModelProtocol::OpenaiChat {
        body["stream_options"] = json!({"include_usage":true});
    }
    // `timeoutMs` 限制连接建立与流式帧之间的静默时间；整轮时限由运行预算约束。
    let idle = Duration::from_millis(config.options.timeout_ms.max(1000));
    let client = reqwest::Client::builder()
        .connect_timeout(idle.min(Duration::from_secs(30)))
        .read_timeout(idle)
        .build()?;
    let mut req = client.post(endpoint).json(&body);
    for (key, value) in model.headers() {
        req = req.header(key, value);
    }
    // 只在收到响应体之前重试。已经开始的流不重放：它的工具参数与正文可能已被观测。
    let mut attempt = 0;
    let response = loop {
        let request = req
            .try_clone()
            .ok_or_else(|| Error::Config("模型请求无法重试".into()))?;
        let result = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            result = request.send() => result,
        };
        let retry = match &result {
            Ok(response) => matches!(
                response.status().as_u16(),
                429 | 500 | 502 | 503 | 504 | 529
            ),
            Err(error) => error.is_connect() || error.is_timeout(),
        };
        if !retry || attempt >= 2 {
            break result?;
        }
        let delay = result
            .as_ref()
            .ok()
            .and_then(|response| response.headers().get("retry-after"))
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|seconds| Duration::from_secs(seconds.clamp(1, 5)))
            .unwrap_or(Duration::from_millis(500 * (1 << attempt)));
        attempt += 1;
        tokio::select! { _ = cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(delay) => {} }
    };
    if !response.status().is_success() {
        let status = response.status().as_u16();
        let message = match status {
            401 | 403 => "模型服务拒绝鉴权，请检查 API Key 和访问权限",
            429 => "模型服务限流，重试后仍不可用",
            404 => "模型或 API 地址不存在，请检查模型配置",
            _ => "模型服务返回错误",
        };
        let detail = read_error_detail(response, cancel).await;
        return Err(Error::Http(if detail.is_empty() {
            format!("{message}（HTTP {status}）")
        } else {
            format!("{message}（HTTP {status}）：{detail}")
        }));
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !content_type.contains("event-stream") && !content_type.contains("ndjson") {
        let raw = read_json(response, cancel).await?;
        let mut result = parse_response(config.protocol, &raw)?;
        for c in &mut result.tool_calls {
            if let Some(name) = names.get(&c.name) {
                c.name = name.clone();
            }
        }
        validate(&result)?;
        emit(&result.text)?;
        return Ok(result);
    }
    let mut stream = response.bytes_stream();
    let mut pending = Vec::new();
    let mut decoder = Decoder::new(config.protocol);
    loop {
        let chunk = tokio::select! {_ = cancel.cancelled()=>return Err(Error::Cancelled),r=stream.next()=>r};
        let Some(chunk) = chunk else { break };
        pending.extend_from_slice(&chunk?);
        if pending.len() > 4 * 1024 * 1024 {
            return Err(Error::ModelProtocol("stream frame exceeds limit".into()));
        }
        while let Some(end) = pending.iter().position(|b| *b == b'\n') {
            let line = String::from_utf8(pending.drain(..=end).collect())
                .map_err(|_| Error::ModelProtocol("invalid UTF-8 stream".into()))?;
            let line = line.trim();
            let data = if content_type.contains("ndjson") {
                line
            } else if let Some(s) = line.strip_prefix("data:") {
                s.trim()
            } else {
                continue;
            };
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let raw: Value = serde_json::from_str(data)?;
            if let Some(delta) = decoder.push(raw)? {
                emit(&delta)?;
            }
        }
    }
    if !pending.iter().all(u8::is_ascii_whitespace) {
        return Err(Error::ModelProtocol("truncated stream frame".into()));
    }
    let mut result = decoder.finish()?;
    for c in &mut result.tool_calls {
        if let Some(name) = names.get(&c.name) {
            c.name = name.clone();
        }
    }
    Ok(result)
}

async fn read_error_detail(response: reqwest::Response, cancel: &CancellationToken) -> String {
    const LIMIT: usize = 64 * 1024;
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while bytes.len() < LIMIT {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => return String::new(),
            chunk = stream.next() => chunk,
        };
        let Some(Ok(chunk)) = chunk else { break };
        let remaining = LIMIT - bytes.len();
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    summarize_error_body(&bytes)
}

fn summarize_error_body(bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let parsed = serde_json::from_slice::<Value>(bytes).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| {
            ["/error/message", "/message", "/detail", "/error"]
                .iter()
                .find_map(|pointer| value.pointer(pointer).and_then(Value::as_str))
        })
        .unwrap_or(raw.as_ref());
    message
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(800)
        .collect()
}
pub(crate) async fn read_json(
    response: reqwest::Response,
    cancel: &CancellationToken,
) -> Result<Value> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    loop {
        let next = tokio::select! {_ = cancel.cancelled()=>return Err(Error::Cancelled),next=stream.next()=>next};
        let Some(chunk) = next else { break };
        let chunk = chunk?;
        if buffer.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(Error::Http("response exceeds size limit".into()));
        }
        buffer.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&buffer).map_err(|_| Error::Http("response is not valid JSON".into()))
}

/// Gemini 与 Ollama 的 tool 参数可能是对象，也可能是已序列化的 JSON 字符串，
/// 这里统一成 JSON 文本。
fn arguments_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// 接受 JSON 对象本身，或包一层字符串的形态。
fn parse_arguments(text: &str) -> Result<Value> {
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(text)?;
    match value {
        Value::String(inner) if !inner.trim().is_empty() => Ok(serde_json::from_str(&inner)?),
        other => Ok(other),
    }
}

/// 内部工具名 → 发给提供方的名字。
///
/// 库里存的是内部名（`bgi.user.list`），回包必须带 wire 名。
pub(crate) fn wire_name(name: &str) -> String {
    let readable = name
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_{}", readable, &super::types::hash(&json!(name))[..8])
}

pub fn output_truncated(reason: Option<&str>) -> bool {
    matches!(reason, Some("max_tokens" | "max_output_tokens" | "length"))
}

pub fn validate(r: &ModelResponse) -> Result<()> {
    let truncated = output_truncated(r.finish_reason.as_deref());
    if !truncated
        && !matches!(
            r.finish_reason.as_deref(),
            Some("stop" | "completed" | "end_turn" | "tool_use" | "tool_calls" | "STOP")
        )
    {
        return Err(Error::ModelProtocol(
            "response interrupted, refused, or missing completion marker".into(),
        ));
    }
    if r.text.trim().is_empty() && r.tool_calls.is_empty() {
        return Err(Error::ModelProtocol(
            if truncated && r.reasoning.is_some() {
                "模型在思考阶段耗尽输出预算，本轮没有产生可见回复".into()
            } else if r.reasoning.is_some() {
                "模型只返回了思考内容，没有可见回复".into()
            } else {
                "empty model response".into()
            },
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for c in &r.tool_calls {
        if c.id.is_empty() || c.name.is_empty() || !c.arguments.is_object() || !ids.insert(&c.id) {
            return Err(Error::ModelProtocol(
                "invalid or duplicate tool call".into(),
            ));
        }
    }
    Ok(())
}

/// 正在拼装的独立推理块（Anthropic 内容块、Responses 输出项）。
///
/// `block` 保留提供方起始帧的原样载荷，拼装时只补文本与签名；私有键
/// （如 `redacted_thinking` 的 `data`）原样回传。
struct ReasoningBlock {
    block: Value,
    text: String,
    signature: String,
}

impl ReasoningBlock {
    fn new(block: Value) -> Self {
        Self {
            block,
            text: String::new(),
            signature: String::new(),
        }
    }

    /// 提供方跳过起始帧、直接送增量帧时的占位块。
    fn orphan() -> Self {
        Self::new(json!({"type":"thinking","thinking":"","signature":""}))
    }
}

/// 推理载荷的收集状态。
///
/// 收集形态取决于各协议把签名放在哪里：Anthropic 与 Responses 有独立的块，
/// Gemini 的签名附着在 part 上，另外两个协议只有可读文本。
enum Pending {
    /// 按块下标索引。不同下标的帧会交错到达，`BTreeMap` 按提供方的块顺序还原。
    Blocks(BTreeMap<usize, ReasoningBlock>),
    /// 签名按 `functionCall` 在轮内的序号索引 —— 运行时会重写 call id，
    /// 而 part 顺序稳定。`thought` part 的文本单独累计。
    Gemini {
        text: String,
        signatures: BTreeMap<usize, String>,
    },
    /// 只有可读文本，不需要回传。
    Text(String),
}

impl Pending {
    fn for_protocol(protocol: ModelProtocol) -> Self {
        match protocol {
            ModelProtocol::AnthropicMessages | ModelProtocol::OpenaiResponses => {
                Self::Blocks(BTreeMap::new())
            }
            ModelProtocol::Gemini => Self::Gemini {
                text: String::new(),
                signatures: BTreeMap::new(),
            },
            ModelProtocol::OpenaiChat | ModelProtocol::OllamaChat => Self::Text(String::new()),
        }
    }

    /// 已收集内容的字节数，用于流式响应上限。
    fn len(&self) -> usize {
        match self {
            Self::Blocks(blocks) => blocks
                .values()
                .map(|r| r.text.len() + r.signature.len())
                .sum(),
            Self::Gemini { text, signatures } => {
                text.len() + signatures.values().map(String::len).sum::<usize>()
            }
            Self::Text(text) => text.len(),
        }
    }

    /// 组装成回传载荷。没有收集到任何内容时返回 `None`。
    fn into_reasoning(self, protocol: ModelProtocol) -> Option<Reasoning> {
        let mut display = String::new();
        let mut blocks = Vec::new();
        match self {
            Self::Blocks(collected) => {
                for (_, r) in collected {
                    display.push_str(&r.text);
                    // Responses 的 reasoning 项本身就是完整载荷，原样回传。
                    if protocol != ModelProtocol::AnthropicMessages {
                        blocks.push(r.block);
                        continue;
                    }
                    let mut block = r.block;
                    if block["type"] == "thinking"
                        && let Some(map) = block.as_object_mut()
                    {
                        // 无条件写入：提供方可能在起始帧里省略 signature，
                        // 只经 `signature_delta` 给出。
                        map.insert("thinking".into(), json!(r.text));
                        map.insert("signature".into(), json!(r.signature));
                    }
                    // `redacted_thinking` 只有 `data`，原样回传。
                    blocks.push(block);
                }
            }
            Self::Gemini { text, signatures } => {
                display = text;
                blocks = signatures
                    .into_iter()
                    .map(|(call_index, signature)| {
                        json!({"callIndex": call_index, "signature": signature})
                    })
                    .collect();
            }
            Self::Text(text) => display = text,
        }
        (!blocks.is_empty() || !display.is_empty()).then_some(Reasoning {
            protocol,
            blocks,
            text: display,
        })
    }
}

pub struct Decoder {
    protocol: ModelProtocol,
    text: String,
    calls: BTreeMap<usize, (String, String, String)>,
    pending: Pending,
    reason: Option<String>,
    usage: Usage,
    completed: bool,
    final_response: Option<ModelResponse>,
}

impl Decoder {
    pub fn new(protocol: ModelProtocol) -> Self {
        Self {
            protocol,
            text: String::new(),
            calls: BTreeMap::new(),
            pending: Pending::for_protocol(protocol),
            reason: None,
            usage: Usage::default(),
            completed: false,
            final_response: None,
        }
    }

    /// 独立块表。当前协议不是块形态时返回 `None`。
    fn blocks(&mut self) -> Option<&mut BTreeMap<usize, ReasoningBlock>> {
        match &mut self.pending {
            Pending::Blocks(blocks) => Some(blocks),
            _ => None,
        }
    }

    /// 追加仅供展示的推理文本。
    fn push_reasoning_text(&mut self, text: &str) {
        match &mut self.pending {
            Pending::Text(display) => display.push_str(text),
            Pending::Gemini { text: display, .. } => display.push_str(text),
            Pending::Blocks(_) => {}
        }
    }

    pub fn push(&mut self, v: Value) -> Result<Option<String>> {
        if !v["error"].is_null() || v["type"] == "error" {
            return Err(Error::ModelProtocol("provider stream error".into()));
        }
        let mut delta = String::new();
        match self.protocol {
            ModelProtocol::OpenaiResponses => match v["type"].as_str().unwrap_or("") {
                "response.output_text.delta" => delta = v["delta"].as_str().unwrap_or("").into(),
                "response.completed" | "response.incomplete" => {
                    self.final_response = Some(parse_response(self.protocol, &v["response"])?);
                    self.completed = true;
                }
                "response.failed" | "response.refusal.delta" => {
                    return Err(Error::ModelProtocol("response did not complete".into()));
                }
                _ => {}
            },
            ModelProtocol::OpenaiChat => {
                let c = &v["choices"][0];
                delta = c["delta"]["content"].as_str().unwrap_or("").into();
                // 只为界面：多数提供方拒绝回传该字段。
                if let Some(thinking) = c["delta"]["reasoning_content"]
                    .as_str()
                    .or_else(|| c["delta"]["reasoning"].as_str())
                {
                    self.push_reasoning_text(thinking);
                }
                if !c["delta"]["refusal"].is_null() {
                    return Err(Error::ModelProtocol("model refused".into()));
                }
                for call in c["delta"]["tool_calls"].as_array().into_iter().flatten() {
                    let slot = self
                        .calls
                        .entry(call["index"].as_u64().unwrap_or(0) as usize)
                        .or_default();
                    if let Some(s) = call["id"].as_str() {
                        slot.0 = s.into();
                    }
                    if let Some(s) = call["function"]["name"].as_str() {
                        slot.1.push_str(s);
                    }
                    if let Some(s) = call["function"]["arguments"].as_str() {
                        slot.2.push_str(s);
                    }
                }
                if let Some(s) = c["finish_reason"].as_str() {
                    self.reason = Some(s.into());
                    self.completed = true;
                }
                if v.get("usage").is_some() {
                    self.usage.merge(usage_from_openai_chat(&v["usage"]));
                }
            }
            ModelProtocol::AnthropicMessages => match v["type"].as_str().unwrap_or("") {
                "message_start" => {
                    self.usage
                        .merge(usage_from_anthropic(&v["message"]["usage"]));
                }
                // 按块类型分派：用 `if ... type == "tool_use"` 守卫会吞掉思考块的起始帧。
                "content_block_start" => {
                    let index = v["index"].as_u64().unwrap_or(0) as usize;
                    let b = &v["content_block"];
                    match b["type"].as_str().unwrap_or("") {
                        "tool_use" => {
                            self.calls.insert(
                                index,
                                (
                                    b["id"].as_str().unwrap_or("").into(),
                                    b["name"].as_str().unwrap_or("").into(),
                                    String::new(),
                                ),
                            );
                        }
                        // `redacted_thinking` 同样要回传，且只带 `data`。
                        "thinking" | "redacted_thinking" => {
                            if let Some(blocks) = self.blocks() {
                                blocks.insert(index, ReasoningBlock::new(b.clone()));
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_delta" => {
                    let index = v["index"].as_u64().unwrap_or(0) as usize;
                    // 提供方可能不带 `delta.type`，此分支兜底。
                    match v["delta"]["type"].as_str().unwrap_or("") {
                        // 推理分支不写 `delta`，不会流进 assistant.delta。
                        "thinking_delta" => {
                            if let (Some(text), Some(blocks)) =
                                (v["delta"]["thinking"].as_str(), self.blocks())
                            {
                                blocks
                                    .entry(index)
                                    .or_insert_with(ReasoningBlock::orphan)
                                    .text
                                    .push_str(text);
                            }
                        }
                        "signature_delta" => {
                            if let (Some(signature), Some(blocks)) =
                                (v["delta"]["signature"].as_str(), self.blocks())
                            {
                                blocks
                                    .entry(index)
                                    .or_insert_with(ReasoningBlock::orphan)
                                    .signature
                                    .push_str(signature);
                            }
                        }
                        _ => {
                            delta = v["delta"]["text"].as_str().unwrap_or("").into();
                            if let Some(s) = v["delta"]["partial_json"].as_str() {
                                self.calls.entry(index).or_default().2.push_str(s);
                            }
                        }
                    }
                }
                "message_delta" => {
                    self.reason = v["delta"]["stop_reason"].as_str().map(str::to_owned);
                    self.usage.merge(usage_from_anthropic(&v["usage"]));
                }
                "message_stop" => self.completed = true,
                _ => {}
            },
            ModelProtocol::Gemini => {
                let c = &v["candidates"][0];
                for p in c["content"]["parts"].as_array().into_iter().flatten() {
                    // 思考文本不进 `delta`，只留给界面。
                    if p["thought"] == true {
                        if let (Some(s), Pending::Gemini { text, .. }) =
                            (p["text"].as_str(), &mut self.pending)
                        {
                            text.push_str(s);
                        }
                        continue;
                    }
                    if let Some(s) = p["text"].as_str() {
                        delta.push_str(s);
                    }
                    if let Some(f) = p.get("functionCall") {
                        // 签名按 functionCall 序号索引：并行调用时只有第一个
                        // part 带签名，回传时必须附回同一个 part。
                        let call_index = self.calls.len();
                        if let (Some(signature), Pending::Gemini { signatures, .. }) = (
                            p["thoughtSignature"]
                                .as_str()
                                .or_else(|| p["thought_signature"].as_str()),
                            &mut self.pending,
                        ) {
                            signatures.insert(call_index, signature.into());
                        }
                        self.calls.insert(
                            call_index,
                            (
                                uuid::Uuid::new_v4().to_string(),
                                f["name"].as_str().unwrap_or("").into(),
                                arguments_text(&f["args"]),
                            ),
                        );
                    }
                }
                if let Some(s) = c["finishReason"].as_str() {
                    self.reason = Some(s.into());
                    self.completed = true;
                }
                if v.get("usageMetadata").is_some() {
                    self.usage.merge(usage_from_gemini(&v["usageMetadata"]));
                }
            }
            ModelProtocol::OllamaChat => {
                delta = v["message"]["content"].as_str().unwrap_or("").into();
                // 只为界面：Ollama 不要求回传思考内容。
                if let Some(thinking) = v["message"]["thinking"].as_str() {
                    self.push_reasoning_text(thinking);
                }
                for c in v["message"]["tool_calls"].as_array().into_iter().flatten() {
                    self.calls.insert(
                        self.calls.len(),
                        (
                            uuid::Uuid::new_v4().to_string(),
                            c["function"]["name"].as_str().unwrap_or("").into(),
                            c["function"]["arguments"].to_string(),
                        ),
                    );
                }
                if v["done"] == true {
                    self.completed = true;
                    self.reason = Some(v["done_reason"].as_str().unwrap_or("stop").into());
                    self.usage.merge(usage_from_ollama(&v));
                }
            }
        }
        self.text.push_str(&delta);
        if self.text.len() > 2 * 1024 * 1024
            || self.calls.values().map(|c| c.2.len()).sum::<usize>() > 4 * 1024 * 1024
            || self.pending.len() > 4 * 1024 * 1024
        {
            return Err(Error::ModelProtocol("response exceeds limit".into()));
        }
        Ok((!delta.is_empty()).then_some(delta))
    }
    pub fn finish(self) -> Result<ModelResponse> {
        if !self.completed {
            return Err(Error::ModelProtocol(
                "stream ended without completion".into(),
            ));
        }
        let result = if let Some(r) = self.final_response {
            r
        } else {
            let calls = self
                .calls
                .into_values()
                .map(|(id, name, args)| {
                    Ok(ToolCall {
                        id,
                        name,
                        arguments: parse_arguments(&args)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let reasoning = self.pending.into_reasoning(self.protocol);
            ModelResponse {
                text: self.text,
                tool_calls: calls,
                finish_reason: self.reason,
                usage: self.usage,
                reasoning,
            }
        };
        validate(&result)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_error_uses_the_structured_message_and_bounds_it() {
        let detail = summarize_error_body(
            br#"{"error":{"type":"invalid_request_error","message":"tool results must follow tool use"}}"#,
        );
        assert_eq!(detail, "tool results must follow tool use");
        assert_eq!(
            summarize_error_body(&vec![b'x'; 2_000]).chars().count(),
            800
        );
    }
}
