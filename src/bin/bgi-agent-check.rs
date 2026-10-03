//! 隔离/提权验收驱动器，复用实际 AppController/Supervisor，不启动桌面窗口。
//!
//! stdio 模式（兼容原有用法）：每行输入一个 {"method":...,"params":...} 请求，
//! 输出一行 {"ok":...} 回包，事件以 {"event":...} 行旁路输出。
//!
//! 文件模式（供 root 以管理员身份调度，无 localhost RPC 暴露）：
//!   bgi-agent-check <config> --commands <file> --responses <file>
//! 命令文件 tail 跟随（EOF 等待追加，不假结束）；每条完整 JSON 行
//! {"id":...,"method":...,"params":...} 走同一 controller.handle 通道；
//! 回复行 {"id":...,"ok":...} 附原 id 便于关联；shutdown 停止本程序启动的
//! 活动运行后退出。所有 event/回复输出先经 sanitize()：凭证键删除、字符串
//! 内嵌 JSON 递归脱敏、凭证形态值打码——密钥与请求头绝不落输出。
use serde_json::{Value, json};
use sleepy_doll::app::AppController;
use std::{
    fs::OpenOptions,
    io::{self, BufRead, Read, Seek, SeekFrom, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

/// 疑似凭证的键名（含变体）：命中即整字段丢弃，宁少报不泄漏。
fn credential_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "apikey",
        "api_key",
        "key",
        "secret",
        "authorization",
        "header",
        "cookie",
        "credential",
        "password",
        "bearer",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

/// 字符串内 "key":"value" / key=value / Bearer xxx 形态的凭证值打码。
/// 键与结构保留（可审计），只把值替换为 [REDACTED]。
fn redact_embedded(text: &str) -> String {
    const MARKERS: [&str; 9] = [
        "token",
        "apikey",
        "api_key",
        "api-key",
        "secret",
        "authorization",
        "password",
        "bearer",
        "cookie",
    ];
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_alphabetic() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len()
            && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-')
        {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let is_marker = MARKERS.contains(&word.to_ascii_lowercase().as_str());
        out.push_str(&word);
        if !is_marker {
            continue;
        }
        // 吃掉键值分隔符（可能有引号夹着，如 "token" : "v"）。
        while i < chars.len()
            && (chars[i] == ' '
                || chars[i] == '\t'
                || chars[i] == ':'
                || chars[i] == '='
                || chars[i] == '"'
                || chars[i] == '\'')
        {
            out.push(chars[i]);
            i += 1;
        }
        if i >= chars.len() || chars[i] == ',' || chars[i] == '}' || chars[i] == ']' {
            continue; // 裸词或空值，没有可打码的内容。
        }
        // 值紧跟另一个凭证词（如 authorization=Bearer xxx）时把值的处理交给后者。
        let peek: String = chars[i..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '_' || **c == '-')
            .collect();
        if MARKERS.contains(&peek.to_ascii_lowercase().as_str()) {
            continue;
        }
        // 值：带引号则到闭合引号，否则到首个分隔/空白符。
        let quote = matches!(chars[i], '"' | '\'');
        if quote {
            let fence = chars[i];
            out.push(fence);
            i += 1;
            while i < chars.len() && chars[i] != fence {
                i += 1;
            }
            if i < chars.len() {
                out.push_str("[REDACTED]");
                out.push(fence);
                i += 1;
            }
        } else {
            while i < chars.len() {
                let c = chars[i];
                if c == ',' || c == '}' || c == ']' || c.is_whitespace() || c == '&' {
                    break;
                }
                i += 1;
            }
            out.push_str("[REDACTED]");
        }
    }
    out
}

/// 递归脱敏：凭证键整字段删除；字符串值先尝试 JSON 反序列化递归处理，
/// 再对残余文本内的凭证形态打码。普通工具证据（sha256、状态、参数）保留。
pub fn sanitize(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(key, _)| !credential_key(key))
                .map(|(key, value)| (key, sanitize(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(sanitize).collect()),
        Value::String(text) => {
            let trimmed = text.trim();
            if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                || (trimmed.starts_with('[') && trimmed.ends_with(']'))
            {
                if let Ok(nested) = serde_json::from_str::<Value>(trimmed) {
                    // 内嵌 JSON：脱敏后重新序列化，保持证据可读。
                    return Value::String(sanitize(nested).to_string());
                }
            }
            Value::String(redact_embedded(&text))
        }
        other => other,
    }
}

/// 从累积缓冲里取出完整（以换行结束）的行；残余半行留在缓冲等待后续追加。
fn take_complete_lines(buffer: &mut String) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(position) = buffer.find('\n') {
        let line: String = buffer.drain(..=position).collect();
        let line = line.trim_end_matches(['\n', '\r']);
        if !line.is_empty() {
            lines.push(line.to_string());
        }
    }
    lines
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os();
    let program = arguments.next();
    let path = arguments.next().ok_or("需要配置路径")?;
    let mut commands: Option<std::path::PathBuf> = None;
    let mut responses: Option<std::path::PathBuf> = None;
    let rest: Vec<std::ffi::OsString> = arguments.collect();
    let mut rest = rest.into_iter();
    while let Some(flag) = rest.next() {
        match flag.to_string_lossy().as_ref() {
            "--commands" => commands = Some(rest.next().ok_or("--commands 需要路径")?.into()),
            "--responses" => responses = Some(rest.next().ok_or("--responses 需要路径")?.into()),
            other => return Err(format!("未知参数：{other}").into()),
        }
    }
    match (commands, responses) {
        (None, None) => run_stdio(path),
        (Some(commands), Some(responses)) => run_file_transport(path, &commands, &responses),
        _ => Err("--commands 与 --responses 必须成对提供".into()),
    }
    .map_err(|error| {
        eprintln!(
            "driver 失败：{error}（program={:?}）",
            program.as_ref().and_then(|p| p.to_str())
        );
        error
    })
}

/// 所有输出共用：一次互斥内整行写入并 flush，事件与回复不交错。
trait LineSink {
    fn write_line(&mut self, line: &str) -> io::Result<()>;
}

struct StdoutSink(io::Stdout);

impl LineSink for StdoutSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        let mut handle = self.0.lock();
        writeln!(handle, "{line}")?;
        handle.flush()
    }
}

struct FileSink(std::fs::File);

impl LineSink for FileSink {
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        self.0.seek(SeekFrom::End(0))?;
        writeln!(self.0, "{line}")?;
        self.0.flush()
    }
}

fn run_stdio(path: std::ffi::OsString) -> Result<(), Box<dyn std::error::Error>> {
    let controller = Arc::new(AppController::load(path)?);
    let sink = Arc::new(Mutex::new(StdoutSink(io::stdout())));
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if value["method"] == "shutdown" {
            controller.shutdown();
            break;
        }
        let result = controller.handle(
            value["method"].as_str().unwrap_or(""),
            value["params"].clone(),
            emit_sink(&sink),
        );
        let response = match result {
            Ok(result) => json!({"ok":true,"result":sanitize(result)}),
            Err(error) => json!({"ok":false,"error":redact_embedded(&error.user_message())}),
        };
        sink.lock().unwrap().write_line(&response.to_string())?;
    }
    controller.shutdown();
    Ok(())
}

fn emit_sink<T: LineSink + Send + 'static>(
    sink: &Arc<Mutex<T>>,
) -> Arc<dyn Fn(String, sleepy_doll::runtime::types::Event) + Send + Sync> {
    let sink = sink.clone();
    Arc::new(
        move |kind: String, event: sleepy_doll::runtime::types::Event| {
            let line = json!({"event": kind, "data": sanitize(event.data)}).to_string();
            if let Ok(mut sink) = sink.lock() {
                let _ = sink.write_line(&line);
            }
        },
    )
}

/// 文件传输模式：tail 命令文件等待追加；回复附原 id；EOF 不结束。
fn run_file_transport(
    path: std::ffi::OsString,
    commands: &std::path::Path,
    responses: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let controller = Arc::new(AppController::load(path)?);
    let sink = Arc::new(Mutex::new(FileSink(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(responses)?,
    )));
    let mut tail = std::fs::File::open(commands)?;
    let mut buffer = String::new();
    loop {
        // 增量读取：只消费已到达的完整行，半行留在缓冲。
        let mut chunk = [0u8; 4096];
        let read = tail.read(&mut chunk)?;
        if read > 0 {
            buffer.push_str(&String::from_utf8_lossy(&chunk[..read]));
        }
        for line in take_complete_lines(&mut buffer) {
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(error) => {
                    let response = json!({"id":Value::Null,"ok":false,"error":format!("命令不是合法 JSON：{error}")});
                    sink.lock().unwrap().write_line(&response.to_string())?;
                    continue;
                }
            };
            let id = value.get("id").cloned().unwrap_or(Value::Null);
            if value["method"] == "shutdown" {
                controller.shutdown();
                sink.lock().unwrap().write_line(
                    &json!({"id":id,"ok":true,"result":{"shutdown":true}}).to_string(),
                )?;
                return Ok(());
            }
            let result = controller.handle(
                value["method"].as_str().unwrap_or(""),
                value["params"].clone(),
                emit_sink(&sink),
            );
            let response = match result {
                Ok(result) => json!({"id":id,"ok":true,"result":sanitize(result)}),
                Err(error) => {
                    json!({"id":id,"ok":false,"error":redact_embedded(&error.user_message())})
                }
            };
            sink.lock().unwrap().write_line(&response.to_string())?;
        }
        // EOF：等待命令文件追加，不假结束。
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize;
    use serde_json::json;

    #[test]
    fn nested_json_payload_with_credentials_is_redacted() {
        let payload = json!({
            "method": "config.read",
            "body": "{\"bridgeToken\":\"abcdef1234567890abcdef1234567890\",\"apiKey\":\"sk-live-111222333\"}".to_string()
        });
        let sanitized = sanitize(payload).to_string();
        assert!(
            !sanitized.contains("abcdef1234567890"),
            "token value leaked: {sanitized}"
        );
        assert!(
            !sanitized.contains("sk-live-111222333"),
            "api key leaked: {sanitized}"
        );
    }

    #[test]
    fn credential_keys_dropped_but_normal_evidence_kept() {
        let payload = json!({
            "Authorization": "Bearer abc.def.ghi",
            "headers": {"X-Api-Key": "zzz"},
            "result": {"sha256": "0f2a".repeat(16), "ready": true, "note": "ok"}
        });
        let sanitized = sanitize(payload);
        assert!(
            sanitized["result"]["sha256"].is_string(),
            "hash evidence must stay"
        );
        assert!(sanitized["result"]["ready"] == json!(true));
        assert!(sanitized.get("Authorization").is_none());
        assert!(sanitized.get("headers").is_none());
    }

    #[test]
    fn embedded_bearer_in_plain_text_is_redacted() {
        let payload = json!({"error": "request failed: authorization=Bearer q1w2e3r4t5y6u7i8o9p0 header 'Cookie: sid=deadbeef'"});
        let sanitized = sanitize(payload).to_string();
        assert!(
            !sanitized.contains("q1w2e3r4t5y6u7i8o9p0"),
            "bearer value leaked: {sanitized}"
        );
        assert!(
            !sanitized.contains("deadbeef"),
            "cookie value leaked: {sanitized}"
        );
    }
}

#[cfg(test)]
mod transport_tests {
    use super::take_complete_lines;

    #[test]
    fn partial_line_waits_for_append_and_crlf_is_tolerated() {
        let mut buffer = String::new();
        buffer.push_str("{\"id\":1}\r\n{\"id\":2");
        let lines = take_complete_lines(&mut buffer);
        assert_eq!(lines, vec!["{\"id\":1}".to_string()]);
        assert_eq!(buffer, "{\"id\":2");
        buffer.push_str("}\n");
        let lines = take_complete_lines(&mut buffer);
        assert_eq!(lines, vec!["{\"id\":2}".to_string()]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn blank_lines_are_skipped() {
        let mut buffer = String::from("\n\n{\"id\":3}\n\n");
        let lines = take_complete_lines(&mut buffer);
        assert_eq!(lines, vec!["{\"id\":3}".to_string()]);
    }
}
