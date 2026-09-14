use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::ToSocketAddrs,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::error::{Error, Result};

type IpcHandler = Arc<dyn Fn(&str, Value) -> Result<Value> + Send + Sync>;
/// 演示会话的重置。只有 mock 有这条：让「模拟对话」每次开演前把上一条清掉，
/// 不然每演一次侧栏就多一条。
type DemoReset = Arc<dyn Fn(&str) -> Result<()> + Send + Sync>;

#[derive(Clone)]
struct MockJob {
    created: Instant,
    outcome: String,
    cancelled: bool,
}

/// 录制下来的一段对话，用来复现真实会话。
///
/// 首轮（报文里没有任何工具结果）比对开场白：一致就把进度归零，从头回放；不一致就
/// 不回放，退回按关键词选场景的默认行为。
///
/// 进度按「轮」而不是消耗队列：队列被取空一次就再也演不出来了，而录制是要反复看的。
#[derive(Default)]
struct Replay {
    prompt: String,
    turns: Vec<Value>,
    cursor: usize,
    active: bool,
}

impl Replay {
    fn next_turn(&mut self, input: &Value) -> Option<Value> {
        if self.turns.is_empty() {
            return None;
        }
        if is_first_turn(input) {
            if latest_user_text(input).as_deref() != Some(self.prompt.as_str()) {
                self.active = false;
                return None;
            }
            self.cursor = 0;
            self.active = true;
        }
        if !self.active {
            return None;
        }
        let turn = self.turns.get(self.cursor)?.clone();
        self.cursor += 1;
        Some(turn)
    }
}

#[derive(Default)]
struct MockState {
    jobs: RwLock<HashMap<String, MockJob>>,
    ipc: RwLock<Option<IpcHandler>>,
    responses: Mutex<VecDeque<Value>>,
    replay: Mutex<Replay>,
    demo_reset: RwLock<Option<DemoReset>>,
    keys: Mutex<HashMap<String, (String, String)>>,
    faults: Mutex<MockFaults>,
    bgi_user_path: RwLock<Option<String>>,
    /// 模型网关请求计数。证明零 token 只能看真实请求数，不能只看界面标签。
    model_requests: AtomicUsize,
}

#[derive(Clone, Default)]
pub struct MockFaults {
    pub model_delay_ms: u64,
    pub stream_chunk_delay_ms: u64,
    pub job_delay_ms: u64,
    pub lose_next_acceptance: bool,
    pub stale_state: bool,
    pub refuse_cancellation: bool,
    pub truncate_model_stream: bool,
}

pub struct MockBackend {
    address: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    state: Arc<MockState>,
}

impl MockBackend {
    pub fn set_faults(&self, faults: MockFaults) {
        *self.state.faults.lock().unwrap() = faults;
    }
    pub fn set_responses(&self, responses: Vec<Value>) {
        *self.state.responses.lock().unwrap() = responses.into();
    }
    /// 注册演示会话的重置：收到 `/dev/reset-demo` 时调用。
    pub fn set_demo_reset<F>(&self, handler: F)
    where
        F: Fn(&str) -> Result<()> + Send + Sync + 'static,
    {
        *self
            .state
            .demo_reset
            .write()
            .expect("demo reset lock poisoned") = Some(Arc::new(handler));
    }
    /// 载入一段录制对话：开场白一致时按序回放。
    pub fn set_replay(&self, prompt: impl Into<String>, turns: Vec<Value>) {
        *self.state.replay.lock().unwrap() = Replay {
            prompt: prompt.into(),
            turns,
            cursor: 0,
            active: false,
        };
    }
    pub fn job_count(&self) -> usize {
        self.state.jobs.read().unwrap().len()
    }
    /// 到模型网关的请求次数。零 token 任务必须让这个数字保持不动。
    pub fn model_requests(&self) -> usize {
        self.state.model_requests.load(Ordering::SeqCst)
    }
    pub fn reset_model_requests(&self) {
        self.state.model_requests.store(0, Ordering::SeqCst);
    }
    pub fn set_bgi_user_path(&self, path: impl Into<String>) {
        *self.state.bgi_user_path.write().unwrap() = Some(path.into());
    }
    pub fn start(address: impl ToSocketAddrs) -> Result<Self> {
        let server = Server::http(address).map_err(|error| Error::Http(error.to_string()))?;
        let address = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| Error::Http("mock backend did not bind an IP socket".into()))?
            .to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let state = Arc::new(MockState::default());
        let worker_stop = stop.clone();
        let worker_state = state.clone();
        let thread = thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                match server.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(request)) => {
                        let state = worker_state.clone();
                        thread::spawn(move || handle_request(request, &state));
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
            state,
        })
    }

    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn set_ipc_handler<F>(&self, handler: F)
    where
        F: Fn(&str, Value) -> Result<Value> + Send + Sync + 'static,
    {
        *self.state.ipc.write().expect("mock IPC lock poisoned") = Some(Arc::new(handler));
    }
}

impl Drop for MockBackend {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Deserialize)]
struct IpcRequest {
    #[serde(default)]
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

fn handle_request(mut request: Request, state: &MockState) {
    let method = request.method().clone();
    let url = request.url().to_owned();
    let origin = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str().to_owned());
    // The IPC route is bound to the whole `AppController`, so a page that can
    // reach it can install and enable plugins. Browsers always send `Origin` on
    // cross-origin requests, so refusing every origin outside the dev servers
    // closes that path; a prefix test would not, because
    // `http://localhost.attacker.com` shares the prefix.
    if let Some(value) = origin.as_deref()
        && !DEV_ORIGINS.contains(&value)
    {
        let _ = request.respond(Response::empty(403));
        return;
    }
    let mut body = String::new();
    let _ = request
        .as_reader()
        .take(16 * 1024 * 1024)
        .read_to_string(&mut body);
    let input = serde_json::from_str(&body).unwrap_or(Value::Null);
    if url.starts_with("/v1/") || url == "/api/chat" {
        state.model_requests.fetch_add(1, Ordering::SeqCst);
        let base = state.faults.lock().unwrap().model_delay_ms;
        let mut rng = MockRng::new();
        thread::sleep(first_token_delay(base, &mut rng));
    }
    let (status, payload) = route(&method, &url, &input, state);
    if status == 200
        && let Some((content_type, chunks)) = stream_frames(&url, &input, &payload, state)
    {
        let _ = write_stream(request, content_type, chunks, origin.as_deref());
        return;
    }
    if url == "/bridge/v1/invoke" && status == 202 {
        let mut faults = state.faults.lock().unwrap();
        if faults.lose_next_acceptance {
            faults.lose_next_acceptance = false;
            let _ = request.respond(Response::empty(200));
            return;
        }
    }
    let response = Response::from_string(payload.to_string())
        .with_status_code(StatusCode(status))
        .with_header(json_header())
        .with_header(cors_header(origin.as_deref()))
        .with_header(
            Header::from_bytes("access-control-allow-methods", "POST, GET, OPTIONS")
                .expect("static header"),
        )
        .with_header(
            Header::from_bytes(
                "access-control-allow-headers",
                "content-type, authorization, idempotency-key",
            )
            .expect("static header"),
        );
    let _ = request.respond(response);
}

/// tiny_http's normal response path buffers the chunked body. Flush each SSE
/// frame explicitly; a Read implementation returning small buffers is not enough.
fn write_stream(
    request: Request,
    content_type: &str,
    chunks: StreamFramesChunks,
    origin: Option<&str>,
) -> std::io::Result<()> {
    let mut writer = request.into_writer();
    write!(
        writer,
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nTransfer-Encoding: chunked\r\nCache-Control: no-cache\r\nConnection: close\r\n{}\r\n\r\n",
        cors_header(origin)
    )?;
    writer.flush()?;
    for (chunk, delay_ms) in chunks {
        if delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }
        write!(writer, "{:X}\r\n", chunk.len())?;
        writer.write_all(&chunk)?;
        writer.write_all(b"\r\n")?;
        writer.flush()?;
    }
    writer.write_all(b"0\r\n\r\n")?;
    writer.flush()
}

type StreamFramesChunks = VecDeque<(Vec<u8>, u64)>;

/// Tiny xorshift PRNG so jitter works without adding a rand dependency.
struct MockRng(u64);
impl MockRng {
    fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let tid = format!("{:?}", thread::current().id())
            .bytes()
            .fold(0u64, |acc, byte| acc.rotate_left(3) ^ byte as u64);
        MockRng(nanos ^ tid.rotate_left(17) ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            0
        } else {
            self.next_u64() % bound
        }
    }
    fn between(&mut self, min: u64, max: u64) -> u64 {
        if max <= min {
            min
        } else {
            min + self.below(max - min + 1)
        }
    }
}

/// Time before the first response byte. A base of 0 disables jitter entirely
/// so offline tests stay deterministic.
fn first_token_delay(base: u64, rng: &mut MockRng) -> Duration {
    if base == 0 {
        return Duration::ZERO;
    }
    Duration::from_millis(rng.between(base * 7 / 10, base * 8 / 5))
}

/// Small chance of a mid-stream stall, like a slow upstream provider.
const STALL_PROBABILITY_PER_MILLE: u64 = 40;
const STALL_MIN_MS: u64 = 260;
const STALL_MAX_MS: u64 = 640;

/// Splits text into token-sized pieces, each carrying the delay to apply
/// before emitting it: jittered base delay, a longer pause at sentence
/// punctuation, and an occasional stall. A base of 0 produces fixed pieces
/// with no delay.
fn stream_plan(text: &str, base: u64, rng: &mut MockRng) -> Vec<(String, u64)> {
    let chars: Vec<char> = text.chars().collect();
    let mut plan = Vec::new();
    let mut index = 0;
    let mut first = true;
    while index < chars.len() {
        let target = if base == 0 {
            6
        } else {
            rng.between(1, 5) as usize
        };
        let size = target.min(chars.len() - index);
        let piece: String = chars[index..index + size].iter().collect();
        index += size;
        // The first token's latency is already covered by the first-token
        // delay applied before the response starts streaming.
        let mut delay = if base == 0 || first {
            0
        } else {
            rng.between(base * 3 / 5, base * 7 / 5)
        };
        if base > 0
            && !first
            && piece
                .chars()
                .last()
                .is_some_and(|c| "。\n！？；：，、.!?;:".contains(c))
        {
            delay += rng.between(base * 2, base * 4);
        }
        if base > 0 && !first && rng.below(1000) < STALL_PROBABILITY_PER_MILLE {
            delay += rng.between(STALL_MIN_MS, STALL_MAX_MS);
        }
        first = false;
        plan.push((piece, delay));
    }
    plan
}

fn sse(value: Value) -> Vec<u8> {
    format!("data: {value}\n\n").into_bytes()
}

fn ndjson(value: Value) -> Vec<u8> {
    format!("{value}\n").into_bytes()
}

/// One streamed chunk together with the delay to serve it after.
type StreamChunk = (Vec<u8>, u64);
/// The content type of a streamed response and the chunks to send.
type StreamFrames = (&'static str, VecDeque<StreamChunk>);

fn stream_frames(
    url: &str,
    input: &Value,
    payload: &Value,
    state: &MockState,
) -> Option<StreamFrames> {
    let path = url.split('?').next().unwrap_or(url);
    let streaming = input["stream"] == true || path.contains(":streamGenerateContent");
    if !streaming {
        return None;
    }
    let truncate = state.faults.lock().unwrap().truncate_model_stream;
    let base = state.faults.lock().unwrap().stream_chunk_delay_ms;
    let mut rng = MockRng::new();
    let mut chunks: VecDeque<(Vec<u8>, u64)> = VecDeque::new();
    if path == "/v1/responses" {
        let text = payload["output"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["type"] == "message")
            .flat_map(|item| item["content"].as_array().into_iter().flatten())
            .filter_map(|item| item["text"].as_str())
            .collect::<String>();
        chunks.extend(
            stream_plan(&text, base, &mut rng)
                .into_iter()
                .map(|(delta, delay)| {
                    (
                        sse(json!({"type":"response.output_text.delta","delta":delta})),
                        delay,
                    )
                }),
        );
        if !truncate {
            chunks.push_back((
                sse(json!({"type":"response.completed","response":payload})),
                0,
            ));
        }
        return Some(("text/event-stream", chunks));
    }
    if path == "/v1/chat/completions" {
        if let Some(calls) = payload["choices"][0]["message"]["tool_calls"].as_array() {
            let calls = calls
                .iter()
                .enumerate()
                .map(|(index, call)| {
                    let mut call = call.clone();
                    call["index"] = json!(index);
                    call
                })
                .collect::<Vec<_>>();
            chunks.push_back((sse(json!({"choices":[{"index":0,"delta":{"tool_calls":calls},"finish_reason":null}]})),0));
        }
        let text = payload["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default();
        chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(content, delay)| {
            (sse(json!({"choices":[{"index":0,"delta":{"content":content},"finish_reason":null}]})), delay)
        }));
        if !truncate {
            chunks.push_back((sse(json!({"choices":[{"index":0,"delta":{},"finish_reason":payload["choices"][0]["finish_reason"]}],"usage":{"prompt_tokens":0,"completion_tokens":0}})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path == "/v1/messages" {
        // 逐块发出，不能只看第一块：一轮里既有正文又有工具调用，只取首块会把
        // 工具调用整段丢掉。
        let blocks = payload["content"].as_array().cloned().unwrap_or_default();
        let calls = blocks.iter().any(|block| block["type"] == "tool_use");
        chunks.push_back((
            sse(json!({"type":"message_start","message":{"usage":{"input_tokens":0}}})),
            0,
        ));
        for (index, block) in blocks.iter().enumerate() {
            if block["type"] == "thinking" {
                chunks.push_back((sse(json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":"","signature":""}})),0));
                chunks.extend(stream_plan(block["thinking"].as_str().unwrap_or_default(), base, &mut rng).into_iter().map(|(text, delay)| {
                    (sse(json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":text}})), delay)
                }));
                chunks.push_back((sse(json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":block["signature"].as_str().unwrap_or_default()}})),0));
            } else if block["type"] == "tool_use" {
                chunks.push_back((sse(json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":block["id"],"name":block["name"],"input":{}}})),0));
                chunks.push_back((sse(json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":block["input"].to_string()}})),0));
            } else {
                let text = block["text"].as_str().unwrap_or_default();
                chunks.push_back((sse(json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}})), 0));
                chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(text, delay)| {
                    (sse(json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":text}})), delay)
                }));
            }
            chunks.push_back((sse(json!({"type":"content_block_stop","index":index})), 0));
        }
        if !truncate {
            let reason = if calls { "tool_use" } else { "end_turn" };
            chunks.push_back((sse(json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":0}})), 0));
            chunks.push_back((sse(json!({"type":"message_stop"})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path.contains(":streamGenerateContent") {
        if !payload["candidates"][0]["content"]["parts"][0]["functionCall"].is_null() {
            let mut frame = payload.clone();
            if truncate {
                frame["candidates"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("finishReason");
            }
            chunks.push_back((sse(frame), 0));
            return Some(("text/event-stream", chunks));
        }
        let text = payload["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or_default();
        chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(text, delay)| {
            (sse(json!({"candidates":[{"content":{"role":"model","parts":[{"text":text}]}}]})), delay)
        }));
        if !truncate {
            chunks.push_back((sse(json!({"candidates":[{"content":{"role":"model","parts":[]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0}})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path == "/api/chat" {
        if payload["message"]["tool_calls"].is_array() {
            let mut frame = payload.clone();
            frame["done"] = json!(!truncate);
            chunks.push_back((ndjson(frame), 0));
            return Some(("application/x-ndjson", chunks));
        }
        let text = payload["message"]["content"].as_str().unwrap_or_default();
        chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(content, delay)| {
            (ndjson(json!({"model":"mock","message":{"role":"assistant","content":content},"done":false})), delay)
        }));
        if !truncate {
            chunks.push_back((ndjson(json!({"model":"mock","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":0,"eval_count":0})), 0));
        }
        return Some(("application/x-ndjson", chunks));
    }
    None
}

fn route(method: &Method, url: &str, input: &Value, state: &MockState) -> (u16, Value) {
    if *method == Method::Options {
        return (204, Value::Null);
    }
    let path = url.split('?').next().unwrap_or(url);
    if *method == Method::Post && path == "/ipc" {
        return ipc(input, state);
    }
    // 开发专用：开演前清掉上一次的演示会话。只在这里提供，不进应用 API。
    if *method == Method::Post && path == "/dev/reset-demo" {
        let id = input["conversationId"].as_str().unwrap_or_default();
        let handler = state.demo_reset.read().unwrap().clone();
        return match handler {
            Some(handler) if !id.is_empty() => match handler(id) {
                Ok(()) => (200, json!({"ok":true})),
                Err(error) => (500, json!({"error":error.to_string()})),
            },
            _ => (400, json!({"error":"conversationId 必填，或未注册重置"})),
        };
    }
    if *method == Method::Post && path == "/v1/responses" {
        if let Some(turn) = state.replay.lock().unwrap().next_turn(input) {
            return (200, responses_turn(&turn));
        }
        if let Some(value) = state.responses.lock().unwrap().pop_front() {
            return (200, value);
        }
        return (200, responses(input));
    }
    if *method == Method::Post && path == "/v1/chat/completions" {
        if let Some(turn) = state.replay.lock().unwrap().next_turn(input) {
            return (200, chat_turn(&turn));
        }
        return (200, chat(input));
    }
    if *method == Method::Post && path == "/v1/messages" {
        if let Some(turn) = state.replay.lock().unwrap().next_turn(input) {
            return (200, anthropic_turn(&turn));
        }
        return (200, anthropic(input));
    }
    if *method == Method::Post && path.starts_with("/v1/models/") {
        if let Some(turn) = state.replay.lock().unwrap().next_turn(input) {
            return (200, gemini_turn(&turn));
        }
        return (200, gemini(input));
    }
    if *method == Method::Post && path == "/api/chat" {
        if let Some(turn) = state.replay.lock().unwrap().next_turn(input) {
            return (200, ollama_turn(&turn));
        }
        return (200, ollama(input));
    }
    if *method == Method::Get && path == "/bridge/v1/info" {
        return (
            200,
            json!({"protocolVersion":"1","instanceId":"mock-bgi","catalogVersion":"mock-v1","simulated":true,"features":["catalog","jobs","state","idempotency","agentGuides"]}),
        );
    }
    if *method == Method::Get && path == "/bridge/v1/state" {
        return (
            200,
            json!({"instanceId":"mock-bgi","snapshotId":"mock-snapshot","observedAt":if state.faults.lock().unwrap().stale_state{"2000-01-01T00:00:00Z".into()}else{now()},"runtime":{"captureReady":true,"windowActive":true,"activeJobId":null},"ui":{"value":"main","status":"observed","confidence":null},"position":{"value":null,"status":"unknown","reason":"Mock scenario intentionally has no position"}}),
        );
    }
    if *method == Method::Get && path == "/bridge/v1/host" {
        return (
            200,
            json!({"installPath":"mock","userPath":state.bgi_user_path.read().unwrap().clone().unwrap_or_else(|| "mock/User".into())}),
        );
    }
    if *method == Method::Get && path == "/bridge/v1/catalog" {
        return (200, catalog(url));
    }
    if *method == Method::Get && path.starts_with("/bridge/v1/catalog/") {
        return (
            200,
            capability(path.trim_start_matches("/bridge/v1/catalog/")),
        );
    }
    if *method == Method::Post && path == "/bridge/v1/invoke" {
        return invoke(input, state);
    }
    if path.starts_with("/bridge/v1/jobs/") {
        let suffix = path.trim_start_matches("/bridge/v1/jobs/");
        if *method == Method::Post && suffix.ends_with("/cancel") {
            return cancel(suffix.trim_end_matches("/cancel"), state);
        }
        if *method == Method::Get {
            return job(suffix, state);
        }
    }
    (
        404,
        json!({"error":{"code":"NOT_FOUND","message":format!("No mock route for {method} {path}")}}),
    )
}

fn ipc(input: &Value, state: &MockState) -> (u16, Value) {
    let request: IpcRequest = match serde_json::from_value(input.clone()) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                json!({"ok":false,"error":{"message":error.to_string()}}),
            );
        }
    };
    let handler = state.ipc.read().expect("mock IPC lock poisoned").clone();
    let Some(handler) = handler else {
        return (
            503,
            json!({"id":request.id,"ok":false,"error":{"message":"Mock AppController is not ready"}}),
        );
    };
    match handler(&request.method, request.params) {
        Ok(result) => (200, json!({"id":request.id,"ok":true,"result":result})),
        Err(error) => (
            200,
            json!({"id":request.id,"ok":false,"error":{"message":error.user_message()}}),
        ),
    }
}

fn responses(input: &Value) -> Value {
    let items = input["input"].as_array().cloned().unwrap_or_default();
    let user_start = items.iter().rposition(|i| i["role"] == "user").unwrap_or(0);
    let latest_user = items
        .get(user_start)
        .and_then(|i| i["content"].as_str())
        .unwrap_or("");
    let recent = &items[user_start..];
    let recent_output = recent
        .iter()
        .rev()
        .find(|i| i["type"] == "function_call_output")
        .and_then(|i| i["output"].as_str());
    if latest_user.contains("测试澄清") {
        return if recent_output.is_some() {
            response_text("已收到你的选择，本次澄清测试完成。")
        } else {
            response_call(
                "question",
                "user.ask",
                json!({"question":"这次要选择哪一条路线？"}),
            )
        };
    }
    if latest_user.contains("测试策略") {
        return if recent_output.is_some_and(|output| output.contains("verifiedSucceeded")) {
            response_text("测试策略已完成，并取得成功验证证据。")
        } else {
            response_call(
                "strategy-plan",
                "plan.update",
                json!({"goal":"每日测试路线","steps":[{"id":"route","title":"运行测试路线","capabilityId":"mock.success","arguments":{}}]}),
            )
        };
    }
    if latest_user.contains("测试授权")
        || latest_user.contains("测试未知")
        || latest_user.contains("测试失败")
    {
        let method = if latest_user.contains("测试未知") {
            "mock.unknown"
        } else if latest_user.contains("测试失败") {
            "mock.failure"
        } else {
            "mock.success"
        };
        return match recent_output {
            None => response_call(
                "describe",
                "bgi.capability.describe",
                json!({"methodId":method}),
            ),
            Some(output) if output.contains("contract") => response_call(
                "invoke",
                "bgi.capability.invoke",
                json!({"methodId":method,"arguments":{}}),
            ),
            Some(output) if output.contains("verifiedSucceeded") => {
                response_text("模拟操作已完成，并取得成功验证证据。")
            }
            Some(_) => response_text("本次模拟没有得到目标成功的验证证据，请查看执行结果。"),
        };
    }
    let tool_output = recent
        .iter()
        .rev()
        .find(|item| item["type"] == "function_call_output");
    if let Some(output) = tool_output {
        let text = output["output"].as_str().unwrap_or_default();
        let parsed: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        if parsed["ok"] == false {
            return response_text("Mock 工具调用失败，未取得可用状态。请查看工具返回的错误。");
        }
        let result = parsed
            .get("value")
            .or_else(|| parsed.get("result"))
            .unwrap_or(&parsed);
        if let Some(runtime) = result.get("runtime") {
            let capture = match runtime["captureReady"].as_bool() {
                Some(true) => "截图可用",
                Some(false) => "截图未就绪",
                None => "截图状态未知",
            };
            let window = match runtime["windowActive"].as_bool() {
                Some(true) => "游戏窗口在前台",
                Some(false) => "游戏窗口不在前台",
                None => "窗口状态未知",
            };
            return response_text(&format!(
                "Mock BGI 状态已读取：{capture}，{window}。没有执行任何游戏写操作。"
            ));
        }
        let message = if text.contains("captureReady") {
            "Mock 工具返回了状态数据，请展开工具结果查看。没有执行任何游戏写操作。"
        } else if text.contains("methodId") {
            "Mock 返回了可用能力目录。"
        } else {
            "Mock 工具调用已完成。"
        };
        return response_text(message);
    }
    let user = items
        .iter()
        .rev()
        .find(|item| item["role"] == "user")
        .and_then(|item| item["content"].as_str())
        .unwrap_or_default();
    if user.contains("短回复") {
        return response_text("可以。Mock 场景已就绪。");
    }
    if user.contains("长回复") {
        return response_text(LONG_RESPONSE);
    }
    if user.contains("路线") {
        return response_call(
            "mock-call-catalog",
            "bgi.capability.search",
            json!({"query":"路线"}),
        );
    }
    if user.contains("状态") || user.contains("连接") {
        return response_call("mock-call-state", "bgi.state.get", json!({}));
    }
    response_text("这是 Mock 模型的文本响应；未访问外网，也没有消耗 Token。")
}

const LONG_RESPONSE: &str = "这是一个不消耗 Token 的长回复布局测试。它用于确认真实对话中出现多段内容时，正文不会无限拉宽，输入区不会被内容推离窗口，侧栏也不会因为长标题产生横向滚动。\n\n第一部分：当前环境\nMock 模型与 Mock BGI Bridge 都在本机运行。此回复没有访问外部服务，也没有执行游戏写操作。模型用量字段中的输入和输出 Token 都是 0。\n\n第二部分：需要观察的界面状态\n1. 每段正文应保持舒适行宽，并按中文语义自然换行。\n2. 连续编号不应挤压正文，段落之间需要明显但不过量的间距。\n3. 内容超过可视高度后只滚动对话区域，顶部模型选择和底部输入框保持稳定。\n4. 左侧历史会话标题需要截断显示，但打开后标题和完整消息仍然可读。\n5. 如果后续出现工具调用，工具卡片应嵌在对应回复位置，而不是漂移到独立任务页面。\n\n第三部分：结果边界\n这段文字只验证排版、滚动和会话恢复，不代表真实 BGI 已完成任何任务。真实执行结果仍必须来自 Bridge Job、后置状态观测和验证字段。\n\n结论：长回复场景已经返回。请继续检查末段是否完整可见、滚动是否平稳，以及回到短回复会话后是否恢复到正确的位置。";

fn response_call(id: &str, name: &str, arguments: Value) -> Value {
    json!({"id":"mock-response","status":"completed","output":[{"type":"function_call","id":format!("fc-{id}"),"call_id":id,"name":name,"arguments":arguments.to_string()}],"usage":{"input_tokens":0,"output_tokens":0}})
}

fn response_text(text: &str) -> Value {
    json!({"id":"mock-response","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":0,"output_tokens":0}})
}

/// Adapt all model protocols to the same deterministic scenario engine.
/// 把各协议的报文归一成 Responses 形状的条目，供场景匹配与回放判定共用。
fn scenario_items(input: &Value) -> Vec<Value> {
    let mut items = Vec::new();
    for message in input["messages"].as_array().into_iter().flatten() {
        if message["role"] == "tool" {
            items.push(json!({"type":"function_call_output","output":message["content"]}));
        } else if let Some(text) = message["content"].as_str() {
            items.push(json!({"role":message["role"],"content":text}));
        } else {
            for block in message["content"].as_array().into_iter().flatten() {
                match block["type"].as_str() {
                    Some("tool_result") => {
                        items.push(json!({"type":"function_call_output","output":block["content"]}))
                    }
                    Some("text") => {
                        items.push(json!({"role":message["role"],"content":block["text"]}))
                    }
                    _ => {}
                }
            }
        }
    }
    for message in input["contents"].as_array().into_iter().flatten() {
        for part in message["parts"].as_array().into_iter().flatten() {
            if let Some(reply) = part.get("functionResponse") {
                items.push(
                    json!({"type":"function_call_output","output":reply["response"]["output"]}),
                );
            } else if let Some(text) = part.get("text") {
                items.push(json!({"role":if message["role"] == "model" {"assistant"} else {"user"},"content":text}));
            }
        }
    }
    items
}

fn scenario(input: &Value) -> Value {
    responses(&json!({"input":scenario_items(input)}))["output"][0].clone()
}

/// 一次报文里没有任何工具结果 —— 即一段对话的第一轮。
fn is_first_turn(input: &Value) -> bool {
    !scenario_items(input)
        .iter()
        .any(|item| item["type"] == "function_call_output")
}

/// 报文里最后一条用户文本。第一轮时它就是用户的开场白。
fn latest_user_text(input: &Value) -> Option<String> {
    scenario_items(input)
        .iter()
        .rev()
        .find(|item| item["role"] == "user")
        .and_then(|item| item["content"].as_str())
        .map(str::to_owned)
}

/// 把录制的一轮渲染成各协议的响应体。
///
/// 工具名在这里换成 wire 名：库里存的是内部名，运行时按 wire 名映射回来。
fn turn_calls(turn: &Value) -> Vec<Value> {
    turn["calls"].as_array().cloned().unwrap_or_default()
}

fn named(name: &Value) -> String {
    crate::runtime::gateway::wire_name(name.as_str().unwrap_or(""))
}

fn anthropic_turn(turn: &Value) -> Value {
    let mut content = Vec::new();
    // 思考块必须排在正文与工具调用之前，签名原样带上。
    if let Some(thinking) = turn["thinking"].as_str().filter(|text| !text.is_empty()) {
        content.push(
            json!({"type":"thinking","thinking":thinking,"signature":"mock-replay-signature"}),
        );
    }
    if let Some(text) = turn["text"].as_str().filter(|text| !text.is_empty()) {
        content.push(json!({"type":"text","text":text}));
    }
    for call in turn_calls(turn) {
        content.push(json!({"type":"tool_use",
            "id":call["id"].as_str().unwrap_or("call"),
            "name":named(&call["name"]),
            "input":call["arguments"]}));
    }
    if content.is_empty() {
        content.push(json!({"type":"text","text":""}));
    }
    json!({"id":"mock-replay","content":content,
        "stop_reason":if turn_calls(turn).is_empty() {"end_turn"} else {"tool_use"},
        "usage":{"input_tokens":0,"output_tokens":0}})
}

fn responses_turn(turn: &Value) -> Value {
    let mut output = Vec::new();
    if let Some(text) = turn["text"].as_str().filter(|text| !text.is_empty()) {
        output.push(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}));
    }
    for call in turn_calls(turn) {
        output.push(json!({"type":"function_call",
            "call_id":call["id"].as_str().unwrap_or("call"),
            "name":named(&call["name"]),
            "arguments":call["arguments"].to_string()}));
    }
    json!({"id":"mock-replay","status":"completed","output":output,
        "usage":{"input_tokens":0,"output_tokens":0}})
}

fn chat_turn(turn: &Value) -> Value {
    let calls = turn_calls(turn);
    let message = if calls.is_empty() {
        json!({"role":"assistant","content":turn["text"]})
    } else {
        json!({"role":"assistant","content":turn["text"].as_str().filter(|t| !t.is_empty()),
            "tool_calls":calls.iter().map(|call| json!({
                "id":call["id"].as_str().unwrap_or("call"),"type":"function",
                "function":{"name":named(&call["name"]),"arguments":call["arguments"].to_string()}})).collect::<Vec<_>>()})
    };
    json!({"choices":[{"message":message,"finish_reason":if calls.is_empty() {"stop"} else {"tool_calls"}}],
        "usage":{"prompt_tokens":0,"completion_tokens":0}})
}

fn gemini_turn(turn: &Value) -> Value {
    let calls = turn_calls(turn);
    let mut parts = Vec::new();
    if let Some(text) = turn["text"].as_str().filter(|text| !text.is_empty()) {
        parts.push(json!({"text":text}));
    }
    for call in &calls {
        parts.push(json!({"functionCall":{"name":named(&call["name"]),"args":call["arguments"]}}));
    }
    json!({"candidates":[{"content":{"role":"model","parts":parts},"finishReason":"STOP"}],
        "usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0}})
}

fn ollama_turn(turn: &Value) -> Value {
    let calls = turn_calls(turn);
    let message = if calls.is_empty() {
        json!({"role":"assistant","content":turn["text"]})
    } else {
        json!({"role":"assistant","content":turn["text"].as_str().filter(|t| !t.is_empty()),
            "tool_calls":calls.iter().map(|call| json!({"function":{
                "name":named(&call["name"]),"arguments":call["arguments"]}})).collect::<Vec<_>>()})
    };
    json!({"model":"mock","message":message,"done":true,"done_reason":"stop","prompt_eval_count":0,"eval_count":0})
}

fn chat(input: &Value) -> Value {
    let item = scenario(input);
    let message = if item["type"] == "function_call" {
        json!({"role":"assistant","content":null,"tool_calls":[{"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}}]})
    } else {
        json!({"role":"assistant","content":item["content"][0]["text"]})
    };
    json!({"choices":[{"message":message,"finish_reason":if item["type"] == "function_call" {"tool_calls"} else {"stop"}}],"usage":{"prompt_tokens":0,"completion_tokens":0}})
}

fn anthropic(input: &Value) -> Value {
    let item = scenario(input);
    let block = if item["type"] == "function_call" {
        json!({"type":"tool_use","id":item["call_id"],"name":item["name"],"input":serde_json::from_str::<Value>(item["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}))})
    } else {
        json!({"type":"text","text":item["content"][0]["text"]})
    };
    json!({"id":"mock-anthropic","content":[block],"stop_reason":if item["type"] == "function_call" {"tool_use"} else {"end_turn"},"usage":{"input_tokens":0,"output_tokens":0}})
}

fn gemini(input: &Value) -> Value {
    let item = scenario(input);
    let part = if item["type"] == "function_call" {
        json!({"functionCall":{"name":item["name"],"args":serde_json::from_str::<Value>(item["arguments"].as_str().unwrap_or("{}")).unwrap_or(json!({}))}})
    } else {
        json!({"text":item["content"][0]["text"]})
    };
    json!({"candidates":[{"content":{"role":"model","parts":[part]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0}})
}

fn ollama(input: &Value) -> Value {
    let mut message = chat(input)["choices"][0]["message"].clone();
    if let Some(calls) = message["tool_calls"].as_array_mut() {
        for call in calls {
            call["function"]["arguments"] = serde_json::from_str::<Value>(
                call["function"]["arguments"].as_str().unwrap_or("{}"),
            )
            .unwrap_or(json!({}));
        }
    }
    json!({"model":"mock","message":message,"done":true,"done_reason":"stop","prompt_eval_count":0,"eval_count":0})
}

fn catalog(url: &str) -> Value {
    let parameters: HashMap<_, _> = url::form_urlencoded::parse(
        url.split_once('?')
            .map(|(_, query)| query)
            .unwrap_or("")
            .as_bytes(),
    )
    .into_owned()
    .collect();
    let query = parameters
        .get("q")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    let offset = parameters
        .get("offset")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = parameters
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50)
        .clamp(1, 100);
    let group = parameters.get("group").map(String::as_str).unwrap_or("");
    let items: Vec<_> = [
        "mock.success",
        "mock.unknown",
        "mock.failure",
        "mock.busy",
        "mock.denied",
        "bgi.ping",
        "bgi.update_subscribed_scripts",
    ]
    .into_iter()
    .map(capability)
    .filter(|item| {
        (group.is_empty() || group == item["group"].as_str().unwrap_or("mock"))
            && format!("{} {}", item["methodId"], item["summary"])
                .to_lowercase()
                .contains(&query)
    })
    .collect();
    let total = items.len();
    let page: Vec<_> = items.into_iter().skip(offset).take(limit).collect();
    let next = offset.saturating_add(page.len());
    json!({"catalogVersion":"mock-v1","total":total,"offset":offset,"nextOffset":if next<total {Some(next)}else{None},"groups":[{"id":"mock","count":6}],"items":page})
}

fn capability(method_id: &str) -> Value {
    let readonly = method_id == "bgi.ping";
    let updating = method_id == "bgi.update_subscribed_scripts";
    let summary = if readonly {
        "读取模拟桥连通状态；不连接真实 BetterGI。"
    } else if updating {
        "刷新当前脚本仓库，并更新全部或指定的真实订阅路径；不需要打开仓库窗口。"
    } else {
        "提交模拟任务以验证审批、任务状态和结果处理；不操作真实游戏。"
    };
    let input_schema = if updating {
        json!({"type":"object","properties":{"mode":{"type":"string","enum":["repositoryOnly","selected","all"]},"paths":{"type":"array","minItems":1,"items":{"type":"string"}}},"required":["mode"],"additionalProperties":false})
    } else {
        json!({"type":"object","properties":{},"additionalProperties":false})
    };
    json!({"methodId":method_id,"displayName":method_id,"providerId":"mock","group":if updating {"repository"} else {"mock"},"instanceId":"mock-bgi","catalogVersion":"mock-v1",
        "summary":summary,"whenToUse":["仅用于开发环境的接口调用验证。"],"parameters":[],"sideEffects":["仅改变内存中的模拟任务记录。"],
        "guide":{"title":method_id,"purpose":summary,"whenToUse":["测试接口契约时"],"preconditions":["当前为 Mock 环境"],"sideEffects":["不操作真实配置或游戏"],
            "resultMeaning":"只证明模拟场景结果，不是真实游戏状态。","verification":"核对 simulated 标志及模拟 Job。","rollback":"模拟记录只存在于当前测试进程。",
            "examples":[{}],"documentationSource":"mock-fixture"},
        "inputSchema":input_schema,
        "executionMode":if readonly {"inline"} else {"job"},"effect":if readonly {"readOnly"} else if updating {"hostCommand"} else if method_id=="mock.config" {"configurationWrite"} else {"gameWrite"},"callable":true,"requiresGameReady":!readonly&&!updating})
}

fn invoke(input: &Value, state: &MockState) -> (u16, Value) {
    let method_id = input["methodId"].as_str().unwrap_or("mock.success");
    if method_id == "bgi.ping" {
        return (
            200,
            json!({"methodId":method_id,"result":{"ok":true,"simulated":true}}),
        );
    }
    if method_id == "mock.busy" {
        return (
            503,
            json!({"error":{"code":"GAME_BUSY","message":"Mock game is busy"}}),
        );
    }
    if method_id == "mock.denied" {
        return (
            403,
            json!({"error":{"code":"PERMISSION_DENIED","message":"Mock permission denied"}}),
        );
    }
    let key = input["requestId"].as_str().unwrap_or("").to_owned();
    let fingerprint = crate::runtime::types::hash(input);
    let mut keys = state.keys.lock().unwrap();
    if let Some((old_hash, id)) = keys.get(&key) {
        if old_hash != &fingerprint {
            return (409, json!({"error":{"code":"IDEMPOTENCY_CONFLICT"}}));
        }
        return (
            202,
            json!({"requestId":key,"jobId":id,"state":"queued","acceptedCatalogVersion":"mock-v1"}),
        );
    }
    let job_id = format!("mock-job-{}", uuid::Uuid::new_v4());
    keys.insert(key, (fingerprint, job_id.clone()));
    let outcome = method_id.trim_start_matches("mock.").to_owned();
    state.jobs.write().expect("mock jobs lock poisoned").insert(
        job_id.clone(),
        MockJob {
            created: Instant::now(),
            outcome,
            cancelled: false,
        },
    );
    (
        202,
        json!({"requestId":input["requestId"],"jobId":job_id,"state":"queued","acceptedCatalogVersion":"mock-v1"}),
    )
}

fn job(job_id: &str, state: &MockState) -> (u16, Value) {
    let jobs = state.jobs.read().expect("mock jobs lock poisoned");
    let Some(job) = jobs.get(job_id) else {
        return (
            404,
            json!({"error":{"code":"JOB_NOT_FOUND","message":"Mock job not found"}}),
        );
    };
    if job.cancelled {
        return (
            200,
            json!({"jobId":job_id,"state":"cancelled","verification":{"status":"unknown","reason":"Cancelled in mock"}}),
        );
    }
    if job.created.elapsed()
        < Duration::from_millis(state.faults.lock().unwrap().job_delay_ms.max(250))
    {
        return (
            200,
            json!({"jobId":job_id,"state":"running","progress":{"message":"Mock execution is running"}}),
        );
    }
    match job.outcome.as_str() {
        "failure" => (
            200,
            json!({"jobId":job_id,"state":"failed","error":{"code":"EXECUTION_FAILED","message":"Mock execution failed"}}),
        ),
        "unknown" => (
            200,
            json!({"jobId":job_id,"state":"completed","result":null,"verification":{"status":"unknown","reason":"Mock method completed without authoritative evidence"}}),
        ),
        _ => (
            200,
            json!({"jobId":job_id,"state":"completed","result":{"mock":true},"verification":{"status":"succeeded","reason":"Mock evidence confirmed"}}),
        ),
    }
}

fn cancel(job_id: &str, state: &MockState) -> (u16, Value) {
    if state.faults.lock().unwrap().refuse_cancellation {
        return (200, json!({"jobId":job_id,"state":"stoppingUnconfirmed"}));
    }
    if let Some(job) = state
        .jobs
        .write()
        .expect("mock jobs lock poisoned")
        .get_mut(job_id)
    {
        job.cancelled = true;
        return (200, json!({"jobId":job_id,"state":"cancelling"}));
    }
    (
        404,
        json!({"error":{"code":"JOB_NOT_FOUND","message":"Mock job not found"}}),
    )
}

/// Browser origins served by the local Vite dev server. Matched exactly, never
/// by prefix.
const DEV_ORIGINS: [&str; 4] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "http://[::1]:5173",
    "http://localhost:4173",
];

fn json_header() -> Header {
    Header::from_bytes("content-type", "application/json; charset=utf-8").expect("static header")
}
fn cors_header(origin: Option<&str>) -> Header {
    // `handle_request` has already rejected any origin outside this list, so an
    // exact match is safe to echo back.
    let value = origin
        .filter(|value| DEV_ORIGINS.contains(value))
        .unwrap_or(DEV_ORIGINS[0]);
    Header::from_bytes("access-control-allow-origin", value).expect("static header")
}
fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
