#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 安装程序的窗口与 IPC：把界面挂起来，请求转给 `sleepy_doll::setup`，进度推回去。

// 桌面壳的窗口改造，按路径引入。
#[path = "../window_chrome.rs"]
mod window_chrome;

use std::{
    io::Read,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use flate2::read::DeflateDecoder;
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleepy_doll::setup::{self, Archive};
#[cfg(target_os = "windows")]
use tao::platform::windows::WindowExtWindows;
use tao::{
    dpi::{LogicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    WebContext, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

/// 由 `installer/pack-payload.ps1` 在构建期生成，路径相对本文件所在目录。
const MANIFEST: &[u8] = include_bytes!("../../target/setup/payload.json");
const PAYLOAD: &[u8] = include_bytes!("../../target/setup/payload.bin");

const TITLE: &str = "Sleepy Doll 安装程序";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 460.0;

/// 写入期间为 true，此时忽略关闭请求。
static BUSY: AtomicBool = AtomicBool::new(false);

#[derive(RustEmbed)]
#[folder = "target/ui/"]
struct UiAssets;

/// 界面用这两个标记判断自己运行在桌面壳里、标题栏是否自绘。
const INITIALIZATION_SCRIPT: &str = "window.__SLEEPY_DOLL_DESKTOP__ = true;\n\
     window.__SLEEPY_DOLL_FRAMELESS__ = true;";

#[derive(Debug)]
enum Push {
    /// 请求的回执，对应 `window.__setupReceive`。
    Reply(Value),
    /// 进度状态，对应 `window.__setupState`。
    State(State),
}

#[derive(Debug)]
enum UserEvent {
    ToWeb(Push),
    Window(window_chrome::Action),
    /// 文件夹选择框与目录确认要在主线程上弹。
    Browse {
        id: String,
    },
    Confirm {
        directory: PathBuf,
        desktop_shortcut: bool,
    },
}

#[derive(Deserialize)]
struct IpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

/// 推给界面的进度状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct State {
    phase: &'static str,
    /// 0..1，取三位小数。
    progress: f64,
    message: String,
    error: Option<String>,
}

impl State {
    fn new(phase: &'static str, progress: f64, message: impl Into<String>) -> Self {
        Self {
            phase,
            progress: (progress * 1000.0).round() / 1000.0,
            message: message.into(),
            error: None,
        }
    }

    fn failed(error: impl std::fmt::Display) -> Self {
        Self {
            phase: "failed",
            progress: 0.0,
            message: String::new(),
            error: Some(error.to_string()),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        // 窗口还没起来，只能弹系统对话框。
        native_message(&format!("安装程序无法启动：{error}"));
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let window = WindowBuilder::new()
        .with_title(TITLE)
        .with_inner_size(Size::Logical(LogicalSize::new(WIDTH, HEIGHT)))
        // window_chrome::install 会加回 WS_THICKFRAME，尺寸用上下限钉死。
        .with_min_inner_size(Size::Logical(LogicalSize::new(WIDTH, HEIGHT)))
        .with_max_inner_size(Size::Logical(LogicalSize::new(WIDTH, HEIGHT)))
        .with_resizable(false)
        .with_decorations(false)
        .build(&event_loop)?;

    let mut web_context = WebContext::new(Some(webview_cache()));
    let owner = window.hwnd();
    let ipc_proxy = proxy.clone();
    let ipc_hwnd = window_chrome::hwnd_id(&window);
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_asynchronous_custom_protocol(
            "sleepy".into(),
            move |_webview_id, request, responder| {
                responder.respond(asset_response(request));
            },
        )
        .with_ipc_handler(move |request| dispatch_ipc(request, ipc_proxy.clone(), ipc_hwnd))
        .with_initialization_script(INITIALIZATION_SCRIPT)
        .with_navigation_handler(|destination| is_application_url(&destination))
        .with_url("sleepy://localhost/setup.html")
        .with_devtools(cfg!(debug_assertions))
        .build(&window)?;

    window_chrome::install(&window);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::ToWeb(Push::Reply(payload))) => {
                if let Ok(serialized) = serde_json::to_string(&payload) {
                    let _ =
                        webview.evaluate_script(&format!("window.__setupReceive?.({serialized});"));
                }
            }
            Event::UserEvent(UserEvent::ToWeb(Push::State(state))) => {
                if let Ok(serialized) = serde_json::to_string(&state) {
                    let _ =
                        webview.evaluate_script(&format!("window.__setupState?.({serialized});"));
                }
            }
            Event::UserEvent(UserEvent::Window(action)) => {
                match (action, BUSY.load(Ordering::SeqCst)) {
                    // 写入期间忽略关闭。
                    (window_chrome::Action::Close, false) => quit(control_flow),
                    (window_chrome::Action::Close, true) => {}
                    (action, _) => window_chrome::perform(&window, action),
                }
            }
            Event::UserEvent(UserEvent::Browse { id }) => {
                let found = setup::browse_for_directory(owner, "选择 Sleepy Doll 的安装位置")
                    .unwrap_or(None);
                let payload = json!({ "id": id, "result": { "directory": found } });
                let _ = webview.evaluate_script(&format!("window.__setupReceive?.({payload});"));
            }
            Event::UserEvent(UserEvent::Confirm {
                directory,
                desktop_shortcut,
            }) => {
                if confirm_foreign(&window, &directory) {
                    start(proxy.clone(), directory, desktop_shortcut);
                } else {
                    state(&proxy, State::new("idle", 0.0, "已取消安装"));
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } if !BUSY.load(Ordering::SeqCst) => quit(control_flow),
            _ => {}
        }
    });
}

/// 安装程序的 WebView2 缓存目录，放在系统临时目录里。
fn webview_cache() -> PathBuf {
    std::env::temp_dir().join("sleepy-doll-setup-webview2")
}

/// 关窗口，退出前安排好清理。
fn quit(control_flow: &mut ControlFlow) {
    setup::cleanup_after_exit(&[setup::Removal::Tree(webview_cache())]);
    *control_flow = ControlFlow::Exit;
}

/// 处理一条界面请求。窗口动作不回执。
fn dispatch_ipc(request: Request<String>, proxy: EventLoopProxy<UserEvent>, hwnd: isize) {
    if !is_application_url(&request.uri().to_string()) {
        return;
    }
    let Ok(request) = serde_json::from_str::<IpcRequest>(request.body()) else {
        return;
    };
    match request.method.as_str() {
        "window.drag" => {
            // 与主程序一致：拖动在指针按下时进入系统移动循环。
            #[cfg(target_os = "windows")]
            window_chrome::begin_system_drag(hwnd);
            return;
        }
        "window.setDragStrip" => {
            window_chrome::set_drag_strip(
                request.params["height"].as_f64().unwrap_or(0.0),
                request.params["controls"].as_f64().unwrap_or(0.0),
                request.params["maximize"].as_bool().unwrap_or(false),
            );
            return;
        }
        _ => {}
    }
    if let Some(action) = window_chrome::Action::from_method(&request.method) {
        // 窗口尺寸是固定的，不接受最大化。
        if action != window_chrome::Action::ToggleMaximize {
            let _ = proxy.send_event(UserEvent::Window(action));
        }
        return;
    }

    let id = request.id;
    match request.method.as_str() {
        "setup.info" => reply(&proxy, &id, info_payload()),
        // 选择框关掉之后才有答案，回执由事件循环发。
        "setup.browse" => {
            let _ = proxy.send_event(UserEvent::Browse { id });
        }
        "setup.install" => {
            let requested = request
                .params
                .get("directory")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(setup::default_directory);
            let desktop_shortcut = request
                .params
                .get("desktopShortcut")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            // 校验的是追加产品名之后的路径。
            let resolved = setup::resolve_directory(&requested);
            let directory = PathBuf::from(&resolved);
            reply(&proxy, &id, json!({}));
            if let Err(error) = setup::validate_directory(&resolved) {
                state(&proxy, State::failed(error));
            } else if setup::is_foreign_directory(&directory) {
                let _ = proxy.send_event(UserEvent::Confirm {
                    directory,
                    desktop_shortcut,
                });
            } else {
                start(proxy.clone(), directory, desktop_shortcut);
            }
        }
        // 位置以注册表为准，不用界面送来的路径。
        "setup.launch" => {
            let started = setup::installed()
                .map(|installed| installed.directory.join(setup::EXECUTABLE))
                .filter(|executable| executable.is_file())
                .is_some_and(|executable| setup::launch(&executable).is_ok());
            reply(&proxy, &id, json!({"started": started}));
        }
        "setup.uninstall" => {
            let remove_user_data = request
                .params
                .get("removeUserData")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            reply(&proxy, &id, json!({}));
            match setup::uninstall_directory() {
                Some(directory) => {
                    if BUSY.swap(true, Ordering::SeqCst) {
                        return;
                    }
                    thread::spawn(move || {
                        let outcome = open_archive().and_then(|archive| {
                            let mut report = |progress: f64, message: &str| {
                                state(&proxy, State::new("running", progress, message));
                            };
                            setup::uninstall(&archive, &directory, remove_user_data, &mut report)
                        });
                        BUSY.store(false, Ordering::SeqCst);
                        finish(&proxy, outcome);
                    });
                }
                None => state(&proxy, State::failed("找不到安装位置，无法卸载")),
            }
        }
        _ => reply(&proxy, &id, json!({})),
    }
}

/// 回一条请求的结果。界面按 `id` 认领。
fn reply(proxy: &EventLoopProxy<UserEvent>, id: &str, result: Value) {
    let _ = proxy.send_event(UserEvent::ToWeb(Push::Reply(
        json!({ "id": id, "result": result }),
    )));
}

/// 起工作线程执行安装。
fn start(proxy: EventLoopProxy<UserEvent>, directory: PathBuf, desktop_shortcut: bool) {
    if setup::is_uninstall_mode() {
        state(&proxy, State::failed("这是卸载程序，不能用来安装"));
        return;
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        let outcome = open_archive().and_then(|archive| {
            let mut report = |progress: f64, message: &str| {
                state(&proxy, State::new("running", progress, message));
            };
            setup::install(&archive, &directory, desktop_shortcut, &mut report)
        });
        BUSY.store(false, Ordering::SeqCst);
        finish(&proxy, outcome);
    });
}

fn finish(proxy: &EventLoopProxy<UserEvent>, outcome: Result<(), setup::Error>) {
    match outcome {
        Ok(()) => state(proxy, State::new("done", 1.0, "完成")),
        Err(error) => state(proxy, State::failed(error)),
    }
}

/// 解压嵌入的载荷，整段一次性放进内存。
fn open_archive() -> Result<Archive, setup::Error> {
    let mut data = Vec::new();
    DeflateDecoder::new(PAYLOAD)
        .read_to_end(&mut data)
        .map_err(|error| setup::Error::message(format!("载荷无法解压：{error}")))?;
    Archive::new(MANIFEST, data)
}

fn state(proxy: &EventLoopProxy<UserEvent>, value: State) {
    let _ = proxy.send_event(UserEvent::ToWeb(Push::State(value)));
}

fn info_payload() -> Value {
    let uninstall = setup::is_uninstall_mode();
    let installed = setup::installed();
    let directory = if uninstall {
        setup::uninstall_directory()
            .map(|path| path.display().to_string())
            .unwrap_or_else(setup::default_directory)
    } else {
        installed
            .as_ref()
            .map(|installed| installed.directory.display().to_string())
            .unwrap_or_else(setup::default_directory)
    };
    json!({
        "version": setup::version(),
        "directory": directory,
        "defaultDirectory": setup::default_directory(),
        "installed": installed.is_some(),
        "installedVersion": installed.and_then(|installed| installed.version),
        "uninstallMode": uninstall,
    })
}

/// 目录里已有别的东西时问一次，默认按钮是「否」。
fn confirm_foreign(window: &Window, directory: &std::path::Path) -> bool {
    let message = format!(
        "{} 里已经有别的东西，没有 Sleepy Doll 的程序文件。\n\n\
         继续安装不会删除已有文件。确定要装到这里吗？",
        directory.display()
    );
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MessageBoxW,
        };
        let answer = unsafe {
            MessageBoxW(
                window.hwnd() as _,
                wide(&message).as_ptr(),
                wide(TITLE).as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        };
        answer == IDYES
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, message);
        true
    }
}

/// 窗口还没建起来时的错误去处。
fn native_message(message: &str) {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                wide(message).as_ptr(),
                wide(TITLE).as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    eprintln!("{message}");
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn is_application_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        matches!(
            (url.scheme(), url.host_str()),
            ("sleepy", Some("localhost")) | ("http", Some("sleepy.localhost"))
        ) && url.port().is_none()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

fn asset_response(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');
    let requested = if path.is_empty() { "setup.html" } else { path };
    match UiAssets::get(requested) {
        Some(asset) => Response::builder()
            .status(200)
            .header(
                CONTENT_TYPE,
                mime_guess::from_path(requested)
                    .first_or_octet_stream()
                    .as_ref(),
            )
            .body(asset.data.into_owned())
            .unwrap(),
        None => Response::builder()
            .status(404)
            .header(CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"Not found".to_vec())
            .unwrap(),
    }
}
