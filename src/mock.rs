use std::{
    collections::{HashMap, VecDeque},
    io::Read,
    net::ToSocketAddrs,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::error::{Error, Result};

type IpcHandler = Arc<dyn Fn(&str, Value) -> Result<Value> + Send + Sync>;

#[derive(Clone)]
struct MockJob {
    created: Instant,
    outcome: String,
    cancelled: bool,
}

#[derive(Default)]
struct MockState {
    jobs: RwLock<HashMap<String, MockJob>>,
    ipc: RwLock<Option<IpcHandler>>,
    responses: Mutex<VecDeque<Value>>,
    keys: Mutex<HashMap<String, (String, String)>>,
    faults: Mutex<MockFaults>,
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
    pub fn job_count(&self) -> usize {
        self.state.jobs.read().unwrap().len()
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
    // Reflect the caller's Origin so both http://localhost:5173 and
    // http://127.0.0.1:5173 (or any other dev origin) pass CORS.
    let origin = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str().to_owned());
    let mut body = String::new();
    let _ = request
        .as_reader()
        .take(16 * 1024 * 1024)
        .read_to_string(&mut body);
    let input = serde_json::from_str(&body).unwrap_or(Value::Null);
    if url.starts_with("/v1/") || url == "/api/chat" {
        let base = state.faults.lock().unwrap().model_delay_ms;
        let mut rng = MockRng::new();
        thread::sleep(first_token_delay(base, &mut rng));
    }
    let (status, payload) = route(&method, &url, &input, state);
    if status == 200
        && let Some((content_type, chunks)) = stream_frames(&url, &input, &payload, state)
    {
        let stream = MockStream {
            chunks,
            current: std::io::Cursor::new(Vec::new()),
        };
        let response = Response::new(
            StatusCode(200),
            vec![
                Header::from_bytes("content-type", content_type).unwrap(),
                cors_header(origin.as_deref()),
            ],
            stream,
            None,
            None,
        );
        let _ = request.respond(response);
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

struct MockStream {
    chunks: VecDeque<(Vec<u8>, u64)>,
    current: std::io::Cursor<Vec<u8>>,
}
impl Read for MockStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.current.position() as usize >= self.current.get_ref().len() {
            let Some((chunk, delay_ms)) = self.chunks.pop_front() else {
                return Ok(0);
            };
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
            self.current = std::io::Cursor::new(chunk);
        }
        self.current.read(buffer)
    }
}

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
        if bound == 0 { 0 } else { self.next_u64() % bound }
    }
    fn between(&mut self, min: u64, max: u64) -> u64 {
        if max <= min { min } else { min + self.below(max - min + 1) }
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

fn stream_frames(
    url: &str,
    input: &Value,
    payload: &Value,
    state: &MockState,
) -> Option<(&'static str, VecDeque<(Vec<u8>, u64)>)> {
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
                .map(|(delta, delay)| (sse(json!({"type":"response.output_text.delta","delta":delta})), delay)),
        );
        if !truncate {
            chunks.push_back((sse(json!({"type":"response.completed","response":payload})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path == "/v1/chat/completions" {
        let text = payload["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default();
        chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(content, delay)| {
            (sse(json!({"choices":[{"index":0,"delta":{"content":content},"finish_reason":null}]})), delay)
        }));
        if !truncate {
            chunks.push_back((sse(json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":0,"completion_tokens":0}})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path == "/v1/messages" {
        let text = payload["content"][0]["text"].as_str().unwrap_or_default();
        chunks.push_back((sse(
            json!({"type":"message_start","message":{"usage":{"input_tokens":0}}}),
        ), 0));
        chunks.push_back((sse(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})), 0));
        chunks.extend(stream_plan(text, base, &mut rng).into_iter().map(|(text, delay)| {
            (sse(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})), delay)
        }));
        if !truncate {
            chunks.push_back((sse(json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":0}})), 0));
            chunks.push_back((sse(json!({"type":"message_stop"})), 0));
        }
        return Some(("text/event-stream", chunks));
    }
    if path.contains(":streamGenerateContent") {
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
    if *method == Method::Post && path == "/v1/responses" {
        if let Some(value) = state.responses.lock().unwrap().pop_front() {
            return (200, value);
        }
        return (200, responses(input));
    }
    if *method == Method::Post && path == "/v1/chat/completions" {
        return (200, chat(input));
    }
    if *method == Method::Post && path == "/v1/messages" {
        return (200, anthropic(input));
    }
    if *method == Method::Post && path.starts_with("/v1/models/") {
        return (200, gemini(input));
    }
    if *method == Method::Post && path == "/api/chat" {
        return (200, ollama(input));
    }
    if *method == Method::Get && path == "/bridge/v1/info" {
        return (
            200,
            json!({"protocolVersion":"1","instanceId":"mock-bgi","features":["catalog","jobs","state","idempotency"]}),
        );
    }
    if *method == Method::Get && path == "/bridge/v1/state" {
        return (
            200,
            json!({"instanceId":"mock-bgi","snapshotId":"mock-snapshot","observedAt":if state.faults.lock().unwrap().stale_state{"2000-01-01T00:00:00Z".into()}else{now()},"runtime":{"captureReady":true,"windowActive":true,"activeJobId":null},"ui":{"value":"main","status":"observed","confidence":null},"position":{"value":null,"status":"unknown","reason":"Mock scenario intentionally has no position"}}),
        );
    }
    if *method == Method::Get && path == "/bridge/v1/catalog" {
        return (200, catalog());
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
        let message = if text.contains("captureReady") {
            "Mock BGI 已连接，截图可用，当前位于主界面；位置在此场景中刻意设为未知。没有执行任何游戏写操作。"
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

fn chat(input: &Value) -> Value {
    let has_tool = input["messages"]
        .as_array()
        .is_some_and(|messages| messages.iter().any(|message| message["role"] == "tool"));
    if has_tool {
        json!({"choices":[{"message":{"role":"assistant","content":"Mock Chat Completions 工具结果已处理。"},"finish_reason":"stop"}],"usage":{"prompt_tokens":0,"completion_tokens":0}})
    } else {
        json!({"choices":[{"message":{"role":"assistant","content":"Mock Chat Completions 文本响应。"},"finish_reason":"stop"}],"usage":{"prompt_tokens":0,"completion_tokens":0}})
    }
}

fn anthropic(_input: &Value) -> Value {
    json!({"id":"mock-anthropic","content":[{"type":"text","text":"Mock Anthropic Messages 文本响应。"}],"stop_reason":"end_turn","usage":{"input_tokens":0,"output_tokens":0}})
}

fn gemini(_input: &Value) -> Value {
    json!({"candidates":[{"content":{"role":"model","parts":[{"text":"Mock Gemini 文本响应。"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":0,"candidatesTokenCount":0}})
}

fn ollama(_input: &Value) -> Value {
    json!({"model":"mock","message":{"role":"assistant","content":"Mock Ollama 文本响应。"},"done":true,"done_reason":"stop","prompt_eval_count":0,"eval_count":0})
}

fn catalog() -> Value {
    json!({"catalogVersion":"mock-v1","items":[capability("mock.success"),capability("mock.unknown"),capability("mock.failure"),capability("mock.busy"),capability("mock.denied")]})
}

fn capability(method_id: &str) -> Value {
    json!({"methodId":method_id,"displayName":method_id,"providerId":"mock","catalogVersion":"mock-v1","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"executionMode":"job","effect":"gameWrite","callable":true})
}

fn invoke(input: &Value, state: &MockState) -> (u16, Value) {
    let method_id = input["methodId"].as_str().unwrap_or("mock.success");
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

fn json_header() -> Header {
    Header::from_bytes("content-type", "application/json; charset=utf-8").expect("static header")
}
fn cors_header(origin: Option<&str>) -> Header {
    let value = origin.filter(|value| value.starts_with("http://127.0.0.1") || value.starts_with("http://localhost")).unwrap_or("http://localhost:5173");
    Header::from_bytes("access-control-allow-origin", value).expect("static header")
}
fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
