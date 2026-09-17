#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{sync::Arc, thread};

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleepy_doll::{app::AppController, runtime::types::Event as AgentEvent};
use tao::{
    dpi::{LogicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use wry::{
    WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

#[derive(RustEmbed)]
#[folder = "ui-dist/"]
struct UiAssets;

#[derive(Debug)]
enum UserEvent {
    ToWeb(Value),
    #[cfg(target_os = "windows")]
    ShowWindow,
    #[cfg(target_os = "windows")]
    Quit,
    #[cfg(target_os = "windows")]
    ShutdownFinished,
}

#[derive(Deserialize)]
struct IpcRequest {
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IpcError {
    code: &'static str,
    message: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Sleepy Doll failed to start: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = match sleepy_doll::config::resolve_path()? {
        sleepy_doll::config::Resolved::Config(path) => path,
        sleepy_doll::config::Resolved::Help => {
            println!("{}", sleepy_doll::config::USAGE);
            return Ok(());
        }
        sleepy_doll::config::Resolved::Version => {
            println!("{}", sleepy_doll::config::VERSION);
            return Ok(());
        }
    };
    sleepy_doll::config::seed(&config_path)?;
    let controller = Arc::new(AppController::load(&config_path)?);
    let startup_config = sleepy_doll::AppConfig::load(&config_path)?;
    if startup_config.host_plugin_enabled() && startup_config.bridge.enabled {
        let startup = controller.clone();
        thread::spawn(move || {
            let _ = startup.handle(
                "bridge.setEnabled",
                json!({"enabled":true}),
                Arc::new(|_, _| {}),
            );
        });
    }
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    #[cfg(target_os = "windows")]
    let _tray = create_tray(proxy.clone())?;
    let window = WindowBuilder::new()
        .with_title("Sleepy Doll")
        .with_inner_size(Size::Logical(LogicalSize::new(1360.0, 860.0)))
        .with_min_inner_size(Size::Logical(LogicalSize::new(900.0, 620.0)))
        .build(&event_loop)?;

    let ipc_controller = controller.clone();
    let ipc_proxy = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_asynchronous_custom_protocol(
            "sleepy".into(),
            move |_webview_id, request, responder| {
                responder.respond(asset_response(request));
            },
        )
        .with_ipc_handler(move |request| {
            dispatch_ipc(request, ipc_controller.clone(), ipc_proxy.clone())
        })
        .with_initialization_script("window.__SLEEPY_DOLL_DESKTOP__ = true;")
        .with_navigation_handler(|destination| {
            if is_application_url(&destination) {
                return true;
            }
            // Markdown links open outside the privileged application WebView.
            if url::Url::parse(&destination)
                .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&destination)
                    .spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open")
                    .arg(&destination)
                    .spawn();
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&destination).spawn();
            }
            false
        })
        .with_url("sleepy://localhost/")
        .with_devtools(cfg!(debug_assertions))
        .build(&window)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::ToWeb(payload)) => {
                if let Ok(serialized) = serde_json::to_string(&payload) {
                    let _ = webview
                        .evaluate_script(&format!("window.__sleepyDollReceive?.({serialized});"));
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                #[cfg(target_os = "windows")]
                window.set_visible(false);
                #[cfg(not(target_os = "windows"))]
                {
                    controller.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ShowWindow) => {
                window.set_visible(true);
                window.set_focus();
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::Quit) => {
                let controller = controller.clone();
                let proxy = proxy.clone();
                thread::spawn(move || {
                    controller.shutdown();
                    let _ = proxy.send_event(UserEvent::ShutdownFinished);
                });
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ShutdownFinished) => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

#[cfg(target_os = "windows")]
fn create_tray(
    proxy: EventLoopProxy<UserEvent>,
) -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
    use tray_icon::{
        Icon, TrayIconBuilder,
        menu::{Menu, MenuEvent, MenuItem},
    };
    let menu = Menu::new();
    let open = MenuItem::new("打开 Sleepy Doll", true, None);
    let quit = MenuItem::new("停止任务并退出", true, None);
    menu.append_items(&[&open, &quit])?;
    let open_id = open.id().clone();
    let quit_id = quit.id().clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == open_id {
            let _ = proxy.send_event(UserEvent::ShowWindow);
        }
        if event.id == quit_id {
            let _ = proxy.send_event(UserEvent::Quit);
        }
    }));
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32i32 {
        for x in 0..32i32 {
            let radius = (x - 16).pow(2) + (y - 16).pow(2);
            if radius < 196 && (x - 20).pow(2) + (y - 12).pow(2) > 100 {
                let color = [128, 128, 128, 255];
                rgba[((y * 32 + x) * 4) as usize..((y * 32 + x) * 4 + 4) as usize]
                    .copy_from_slice(&color);
            }
        }
    }
    Ok(TrayIconBuilder::new()
        .with_tooltip("Sleepy Doll · 关闭窗口后继续运行")
        .with_icon(Icon::from_rgba(rgba, 32, 32)?)
        .with_menu(Box::new(menu))
        .build()?)
}

fn dispatch_ipc(
    request: Request<String>,
    controller: Arc<AppController>,
    proxy: EventLoopProxy<UserEvent>,
) {
    if !is_application_url(&request.uri().to_string()) {
        return;
    }
    let parsed = serde_json::from_str::<IpcRequest>(request.body());
    thread::spawn(move || match parsed {
        Ok(request) => {
            let event_proxy = proxy.clone();
            let emit: Arc<dyn Fn(String, AgentEvent) + Send + Sync> =
                Arc::new(move |task_id, event| {
                    let _ = event_proxy.send_event(UserEvent::ToWeb(
                        json!({"kind":"event","taskId":task_id,"event":event}),
                    ));
                });
            let response = match controller.handle(&request.method, request.params, emit) {
                Ok(result) => json!({"kind":"response","id":request.id,"ok":true,"result":result}),
                Err(error) => {
                    json!({"kind":"response","id":request.id,"ok":false,"error":IpcError { code:"NATIVE_ERROR", message:error.user_message() }})
                }
            };
            let _ = proxy.send_event(UserEvent::ToWeb(response));
        }
        Err(error) => {
            let _ = proxy.send_event(UserEvent::ToWeb(json!({"kind":"response","id":"unknown","ok":false,"error":{"code":"INVALID_IPC","message":error.to_string()}})));
        }
    });
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
    let requested = if path.is_empty() { "index.html" } else { path };
    let asset = UiAssets::get(requested).or_else(|| UiAssets::get("index.html"));
    match asset {
        Some(asset) => Response::builder()
            .status(200)
            .header(
                CONTENT_TYPE,
                mime_guess::from_path(requested)
                    .first_or_octet_stream()
                    .as_ref(),
            )
            .header(
                "cache-control",
                if requested == "index.html" {
                    "no-cache"
                } else {
                    "public, max-age=31536000, immutable"
                },
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
