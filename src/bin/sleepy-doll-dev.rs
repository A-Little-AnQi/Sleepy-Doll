use std::{env, io::Read, path::PathBuf, sync::Arc, thread};

use serde::Deserialize;
use serde_json::{Value, json};
use sleepy_doll::app::AppController;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.sleepy-doll/dev/config.json");
const IPC_PORT_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/.sleepy-doll/dev/ipc.port");
const IPC_PORT: u16 = 47124;
const IPC_SPAN: u16 = 32;

#[derive(Deserialize)]
struct IpcRequest {
    #[serde(default)]
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Sleepy Doll dev backend failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(argument) = env::args_os().nth(1) {
        match argument.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", sleepy_doll::config::USAGE);
                return Ok(());
            }
            "-V" | "--version" => {
                println!("{}", sleepy_doll::config::VERSION);
                return Ok(());
            }
            other => {
                return Err(format!(
                    "sleepy-doll-dev 不接受命令行参数（收到 {other}）；它固定读取 {CONFIG}"
                )
                .into());
            }
        }
    }
    let config = PathBuf::from(CONFIG);
    sleepy_doll::config::seed(&config)?;
    if let Some(directory) = config.parent()
        && let Err(error) = sleepy_doll::logging::init(directory)
    {
        eprintln!("无法写入日志（{error}），本次运行的记录只有标准错误。");
    }
    let controller = Arc::new(AppController::load(&config)?);
    let (server, port) = bind_ipc()?;
    std::fs::write(IPC_PORT_FILE, format!("{port}\n"))?;
    println!("Sleepy Doll dev backend: http://127.0.0.1:{port}/ipc");
    println!("UI preview: Vite 默认 http://127.0.0.1:5173/ ，占用则顺延");
    println!("Configuration: {}", config.display());
    loop {
        let request = server.recv()?;
        let controller = controller.clone();
        thread::spawn(move || handle_request(request, &controller));
    }
}

fn handle_request(mut request: Request, controller: &Arc<AppController>) {
    let origin = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Origin"))
        .map(|header| header.value.as_str().to_owned());
    if let Some(value) = origin.as_deref()
        && !loopback_origin(value)
    {
        let _ = request.respond(Response::empty(403));
        return;
    }
    let method = request.method().clone();
    let url = request.url().to_owned();
    if method == Method::Options {
        let _ = request.respond(cors(Response::empty(204), origin.as_deref()));
        return;
    }
    let mut body = String::new();
    let _ = request
        .as_reader()
        .take(16 * 1024 * 1024)
        .read_to_string(&mut body);
    let path = url.split('?').next().unwrap_or(&url);
    let (status, payload) = if method == Method::Post && path == "/ipc" {
        ipc(&body, controller)
    } else {
        (
            404,
            json!({"ok":false,"error":{"message":format!("No route for {method} {path}")}}),
        )
    };
    let response = cors(
        Response::from_string(payload.to_string())
            .with_status_code(StatusCode(status))
            .with_header(json_header()),
        origin.as_deref(),
    );
    let _ = request.respond(response);
}

fn ipc(body: &str, controller: &Arc<AppController>) -> (u16, Value) {
    let request: IpcRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return (
                400,
                json!({"ok":false,"error":{"message":error.to_string()}}),
            );
        }
    };
    match controller.handle(&request.method, request.params, Arc::new(|_, _| {})) {
        Ok(result) => (200, json!({"id":request.id,"ok":true,"result":result})),
        Err(error) => (
            200,
            json!({"id":request.id,"ok":false,"error":{"message":error.user_message()}}),
        ),
    }
}

fn bind_ipc() -> Result<(Server, u16), Box<dyn std::error::Error>> {
    let mut last = String::new();
    for port in IPC_PORT..IPC_PORT.saturating_add(IPC_SPAN) {
        match Server::http(format!("127.0.0.1:{port}")) {
            Ok(server) => return Ok((server, port)),
            Err(error) => last = error.to_string(),
        }
    }
    Err(format!("无法绑定 127.0.0.1:{IPC_PORT} 起的端口：{last}").into())
}

fn loopback_origin(origin: &str) -> bool {
    let Ok(url) = url::Url::parse(origin) else {
        return false;
    };
    url.scheme() == "http"
        && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"))
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
}

fn json_header() -> Header {
    Header::from_bytes("content-type", "application/json; charset=utf-8").expect("static header")
}

fn cors_header(origin: Option<&str>) -> Header {
    let value = origin
        .filter(|value| loopback_origin(value))
        .unwrap_or("http://127.0.0.1:5173");
    Header::from_bytes("access-control-allow-origin", value).expect("static header")
}

fn cors<R: std::io::Read>(response: Response<R>, origin: Option<&str>) -> Response<R> {
    response
        .with_header(cors_header(origin))
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
        )
}
