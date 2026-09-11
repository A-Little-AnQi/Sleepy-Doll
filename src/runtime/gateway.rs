use crate::{
    config::{ModelConfig, ModelProtocol},
    error::{Error, Result},
    model::{Message, ModelResponse, ProtocolModel, ToolCall, Usage, parse_response},
    tools::ToolDefinition,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

pub async fn complete(
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
    // `timeoutMs` bounds connection setup and the silence between stream frames.
    // A whole-response deadline on the client would abort long streamed answers
    // mid-sentence; the run deadline already bounds the total turn.
    let idle = Duration::from_millis(config.options.timeout_ms.max(1000));
    let client = reqwest::Client::builder()
        .connect_timeout(idle.min(Duration::from_secs(30)))
        .read_timeout(idle)
        .build()?;
    let mut req = client.post(endpoint).json(&body);
    for (key, value) in model.headers() {
        req = req.header(key, value);
    }
    let response =
        tokio::select! {_ = cancel.cancelled()=>return Err(Error::Cancelled),r=req.send()=>r?};
    if !response.status().is_success() {
        return Err(Error::Http(format!(
            "model returned HTTP {}",
            response.status().as_u16()
        )));
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

/// Gemini and Ollama may deliver tool arguments either as an object or as an
/// already-serialized JSON string. Both are normalized to JSON text here.
fn arguments_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Accepts the JSON object itself, or one level of string wrapping around it.
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

fn wire_name(name: &str) -> String {
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

pub fn validate(r: &ModelResponse) -> Result<()> {
    if !matches!(
        r.finish_reason.as_deref(),
        Some("stop" | "completed" | "end_turn" | "tool_use" | "tool_calls" | "STOP")
    ) {
        return Err(Error::ModelProtocol(
            "response interrupted, refused, or missing completion marker".into(),
        ));
    }
    if r.text.trim().is_empty() && r.tool_calls.is_empty() {
        return Err(Error::ModelProtocol("empty model response".into()));
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

pub struct Decoder {
    protocol: ModelProtocol,
    text: String,
    calls: BTreeMap<usize, (String, String, String)>,
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
            reason: None,
            usage: Usage::default(),
            completed: false,
            final_response: None,
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
                "response.completed" => {
                    self.final_response = Some(parse_response(self.protocol, &v["response"])?);
                    self.completed = true;
                }
                "response.failed" | "response.incomplete" | "response.refusal.delta" => {
                    return Err(Error::ModelProtocol("response did not complete".into()));
                }
                _ => {}
            },
            ModelProtocol::OpenaiChat => {
                let c = &v["choices"][0];
                delta = c["delta"]["content"].as_str().unwrap_or("").into();
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
                if let Some(n) = v["usage"]["prompt_tokens"].as_u64() {
                    self.usage.input_tokens = Some(n);
                }
                if let Some(n) = v["usage"]["completion_tokens"].as_u64() {
                    self.usage.output_tokens = Some(n);
                }
            }
            ModelProtocol::AnthropicMessages => match v["type"].as_str().unwrap_or("") {
                "message_start" => {
                    self.usage.input_tokens = v["message"]["usage"]["input_tokens"].as_u64()
                }
                "content_block_start" if v["content_block"]["type"] == "tool_use" => {
                    let b = &v["content_block"];
                    self.calls.insert(
                        v["index"].as_u64().unwrap_or(0) as usize,
                        (
                            b["id"].as_str().unwrap_or("").into(),
                            b["name"].as_str().unwrap_or("").into(),
                            String::new(),
                        ),
                    );
                }
                "content_block_delta" => {
                    delta = v["delta"]["text"].as_str().unwrap_or("").into();
                    if let Some(s) = v["delta"]["partial_json"].as_str() {
                        self.calls
                            .entry(v["index"].as_u64().unwrap_or(0) as usize)
                            .or_default()
                            .2
                            .push_str(s);
                    }
                }
                "message_delta" => {
                    self.reason = v["delta"]["stop_reason"].as_str().map(str::to_owned);
                    self.usage.output_tokens = v["usage"]["output_tokens"].as_u64();
                }
                "message_stop" => self.completed = true,
                _ => {}
            },
            ModelProtocol::Gemini => {
                let c = &v["candidates"][0];
                for p in c["content"]["parts"].as_array().into_iter().flatten() {
                    if p["thought"] == true {
                        continue;
                    }
                    if let Some(s) = p["text"].as_str() {
                        delta.push_str(s);
                    }
                    if let Some(f) = p.get("functionCall") {
                        self.calls.insert(
                            self.calls.len(),
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
                    self.usage.input_tokens = v["usageMetadata"]["promptTokenCount"].as_u64();
                    self.usage.output_tokens = v["usageMetadata"]["candidatesTokenCount"].as_u64();
                }
            }
            ModelProtocol::OllamaChat => {
                delta = v["message"]["content"].as_str().unwrap_or("").into();
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
                    self.usage.input_tokens = v["prompt_eval_count"].as_u64();
                    self.usage.output_tokens = v["eval_count"].as_u64();
                }
            }
        }
        self.text.push_str(&delta);
        if self.text.len() > 2 * 1024 * 1024
            || self.calls.values().map(|c| c.2.len()).sum::<usize>() > 4 * 1024 * 1024
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
            ModelResponse {
                text: self.text,
                tool_calls: calls,
                finish_reason: self.reason,
                usage: self.usage,
            }
        };
        validate(&result)?;
        Ok(result)
    }
}
