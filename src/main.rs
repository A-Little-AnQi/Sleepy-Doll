#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod window_chrome;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleepy_doll::{app::AppController, logging, runtime::types::Event as AgentEvent};
#[cfg(target_os = "windows")]
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::{
    dpi::{LogicalSize, PhysicalSize, Size},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
use wry::{
    WebContext, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

/// `assets/sleepy-doll.rc` 里的图标编号。
#[cfg(target_os = "windows")]
const APP_ICON: u16 = 1;

/// 窗口图标与任务栏图标取 exe 资源里的同一份；取不到只影响外观，不该拦住启动。
#[cfg(target_os = "windows")]
fn shell_icon(size: u32) -> Option<tao::window::Icon> {
    match tao::window::Icon::from_resource(APP_ICON, Some(PhysicalSize::new(size, size))) {
        Ok(icon) => Some(icon),
        Err(error) => {
            log::warn!("窗口图标不可用: {error}");
            None
        }
    }
}

#[derive(RustEmbed)]
#[folder = "target/ui/"]
struct UiAssets;

/// 界面靠这两个标记判断自己跑在桌面壳里、以及标题栏要不要自绘。
#[cfg(target_os = "windows")]
const INITIALIZATION_SCRIPT: &str = "window.__SLEEPY_DOLL_DESKTOP__ = true;\n\
     window.__SLEEPY_DOLL_FRAMELESS__ = true;";
#[cfg(not(target_os = "windows"))]
const INITIALIZATION_SCRIPT: &str = "window.__SLEEPY_DOLL_DESKTOP__ = true;";

#[derive(Debug)]
enum UserEvent {
    ToWeb(Value),
    Window(window_chrome::Action),
    #[cfg(target_os = "windows")]
    SyncChrome,
    #[cfg(target_os = "windows")]
    ShowWindow,
    /// 显隐托盘图标。开关的持久化在发起侧完成，这里只负责主线程上的界面变更。
    #[cfg(target_os = "windows")]
    TrayVisible(bool),
    /// 同步托盘菜单里桥开关的勾选态；菜单与界面两个入口都会走到这里。
    #[cfg(target_os = "windows")]
    BridgeState(bool),
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
    // 用户目录是配置文件的所在目录：日志和 WebView2 的缓存都放进去，安装目录里
    // 就只剩程序本身，升级覆盖时不会连带清掉它们。
    let user_directory = config_path
        .parent()
        .ok_or("配置路径没有所在目录，无法定位用户目录")?
        .to_path_buf();
    if let Err(error) = logging::init(&user_directory) {
        // 日志是诊断手段，不是运行前提。
        eprintln!("无法写入日志（{error}），本次运行的记录只有标准错误。");
    }
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
    // 桥开关的当前值。托盘菜单勾选态、延迟建图标和启动自动连接都从这里取。
    #[cfg(target_os = "windows")]
    let bridge_enabled = Arc::new(AtomicBool::new(startup_config.bridge.enabled));
    #[cfg(target_os = "windows")]
    let mut tray = if startup_config.tray.enabled {
        Some(create_tray(
            proxy.clone(),
            user_directory.clone(),
            bridge_enabled.clone(),
        )?)
    } else {
        None
    };
    // 窗口保持不透明：圆角由 DWM 切（Win11），Win10 上就是方角，与系统上其它
    // 浏览器形态一致。透明管线会被拖动等 DWM 状态变化打破，露白角，不能用。
    let builder = WindowBuilder::new()
        .with_title("Sleepy Doll")
        .with_inner_size(Size::Logical(LogicalSize::new(1360.0, 860.0)))
        .with_min_inner_size(Size::Logical(LogicalSize::new(900.0, 620.0)));
    // 两个图标要在窗口创建时就设上：tao 建窗口的过程中会把它们置空，
    // 而 window_chrome::install() 要等 WebView2 启动完。
    #[cfg(target_os = "windows")]
    let builder = builder
        .with_window_icon(shell_icon(16))
        .with_taskbar_icon(shell_icon(32));
    // 标题栏由界面自绘（见 window_chrome），系统那一条要先摘掉。
    #[cfg(target_os = "windows")]
    let builder = builder.with_decorations(false);
    let window = builder.build(&event_loop)?;

    // WebView2 默认把用户数据放在 exe 旁边，那是安装目录里会被升级覆盖的位置。
    let mut web_context = WebContext::new(Some(webview_data_directory(&user_directory)));
    let ipc_controller = controller.clone();
    let ipc_proxy = proxy.clone();
    let ipc_config_path = config_path.clone();
    let ipc_hwnd = window_chrome::hwnd_id(&window);
    let webview = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_asynchronous_custom_protocol(
            "sleepy".into(),
            move |_webview_id, request, responder| {
                responder.respond(asset_response(request));
            },
        )
        .with_ipc_handler(move |request| {
            dispatch_ipc(
                request,
                ipc_controller.clone(),
                ipc_proxy.clone(),
                ipc_config_path.clone(),
                ipc_hwnd,
            )
        })
        .with_initialization_script(INITIALIZATION_SCRIPT)
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

    // 子窗口这时才存在：WebView2 的边缘命中测试要交给它让出来。
    window_chrome::install(&window);
    let mut maximized = window.is_maximized();

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
                close_window(tray.as_ref(), &window, &controller, &proxy);
                #[cfg(not(target_os = "windows"))]
                {
                    controller.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => {
                // 只有真正变了才通知：缩放过程里这个事件很密集。
                let current = window.is_maximized();
                if current != maximized {
                    maximized = current;
                    let _ = webview.evaluate_script(&format!(
                        "window.__sleepyDollWindow?.({});",
                        json!({ "maximized": current })
                    ));
                }
            }
            Event::UserEvent(UserEvent::Window(action)) => {
                #[cfg(target_os = "windows")]
                if action == window_chrome::Action::Close {
                    close_window(tray.as_ref(), &window, &controller, &proxy);
                } else {
                    window_chrome::perform(&window, action);
                }
                #[cfg(not(target_os = "windows"))]
                window_chrome::perform(&window, action);
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::SyncChrome) => {
                let _ = webview.evaluate_script(&format!(
                    "window.__sleepyDollWindow?.({});",
                    json!({ "maximized": window.is_maximized() })
                ));
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ShowWindow) => {
                window.set_visible(true);
                window.set_focus();
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::TrayVisible(visible)) => {
                match (tray.as_mut(), visible) {
                    (Some(tray), true) => {
                        let _ = tray.icon.set_visible(true);
                    }
                    // 图标还没建过（启动时配置为关）且要显示时才建，勾选态取当前桥状态。
                    (None, true) => match create_tray(
                        proxy.clone(),
                        user_directory.clone(),
                        bridge_enabled.clone(),
                    ) {
                        Ok(handles) => tray = Some(handles),
                        Err(error) => log::warn!("托盘图标不可用: {error}"),
                    },
                    // 隐藏要连句柄一起丢弃：close_window 按句柄是否存在选择
                    // 收进托盘还是退出，留着一个隐藏的句柄会让窗口找不到归处。
                    // 丢弃 TrayIcon 的同时图标也会从通知区移除。
                    (_, false) => tray = None,
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::BridgeState(enabled)) => {
                bridge_enabled.store(enabled, Ordering::Relaxed);
                if let Some(tray) = tray.as_ref() {
                    tray.bridge.set_checked(enabled);
                }
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

/// WebView2 的用户数据目录。放进用户目录，与配置、日志同级。
fn webview_data_directory(user_directory: &Path) -> std::path::PathBuf {
    user_directory.join(".sleepy-doll").join("webview2")
}

/// 关闭窗口只把界面收起来：任务可能还在跑，退出的入口在托盘菜单里。
#[cfg(target_os = "windows")]
fn fold_into_tray(window: &Window) {
    window.set_visible(false);
}

/// 托盘可用时关闭窗口等于收进托盘；托盘被禁用就无处可回，直接走退出流程。
#[cfg(target_os = "windows")]
fn close_window(
    tray: Option<&TrayHandles>,
    window: &Window,
    controller: &Arc<AppController>,
    proxy: &EventLoopProxy<UserEvent>,
) {
    if tray.is_some() {
        fold_into_tray(window);
        return;
    }
    let controller = controller.clone();
    let proxy = proxy.clone();
    thread::spawn(move || {
        controller.shutdown();
        let _ = proxy.send_event(UserEvent::ShutdownFinished);
    });
}

#[cfg(target_os = "windows")]
fn create_tray(
    proxy: EventLoopProxy<UserEvent>,
    user_directory: PathBuf,
    bridge_enabled: Arc<AtomicBool>,
) -> Result<TrayHandles, Box<dyn std::error::Error>> {
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
        menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };

    let menu = Menu::new();
    let bridge = CheckMenuItem::new(
        "BetterGI 桥",
        true,
        bridge_enabled.load(Ordering::Relaxed),
        None,
    );
    let open_user = MenuItem::new("打开配置目录", true, None);
    let quit = MenuItem::new("停止任务并退出", true, None);
    menu.append_items(&[&bridge, &open_user, &PredefinedMenuItem::separator(), &quit])?;
    let bridge_id = bridge.id().clone();
    let open_user_id = open_user.id().clone();
    let quit_id = quit.id().clone();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == bridge_id {
            // 界面入口触发的桥开关也写这同一个原子值，两边永远一致。
            let next = !bridge_enabled.load(Ordering::Relaxed);
            bridge_enabled.store(next, Ordering::Relaxed);
            let _ = menu_proxy.send_event(UserEvent::BridgeState(next));
        }
        if event.id == open_user_id {
            let _ = std::process::Command::new("explorer.exe")
                .arg(&user_directory)
                .spawn();
        }
        if event.id == quit_id {
            let _ = menu_proxy.send_event(UserEvent::Quit);
        }
    }));
    // 左键（松开）直接呼出主窗口；菜单只挂在右键上。
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = proxy.send_event(UserEvent::ShowWindow);
        }
    }));
    let mut builder = TrayIconBuilder::new()
        .with_tooltip("Sleepy Doll · 关闭窗口后继续运行")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false);
    // 图标编在 exe 的资源里，界面、窗口和托盘用的是同一份。取不到只影响外观，
    // 不该拦住启动。
    match Icon::from_resource(APP_ICON, Some((32, 32))) {
        Ok(icon) => builder = builder.with_icon(icon),
        Err(error) => log::warn!("托盘图标不可用: {error}"),
    }
    Ok(TrayHandles {
        icon: builder.build()?,
        bridge,
    })
}

/// 事件循环持有的托盘句柄：显隐图标、同步勾选态都要用到菜单项本身。
#[cfg(target_os = "windows")]
struct TrayHandles {
    icon: tray_icon::TrayIcon,
    bridge: tray_icon::menu::CheckMenuItem,
}
fn dispatch_ipc(
    request: Request<String>,
    controller: Arc<AppController>,
    proxy: EventLoopProxy<UserEvent>,
    config_path: PathBuf,
    hwnd: isize,
) {
    if !is_application_url(&request.uri().to_string()) {
        return;
    }
    let parsed = serde_json::from_str::<IpcRequest>(request.body());
    if let Ok(request) = &parsed {
        match request.method.as_str() {
            "window.state" => {
                #[cfg(target_os = "windows")]
                let _ = proxy.send_event(UserEvent::SyncChrome);
                return;
            }
            "window.drag" => {
                // 拖动必须抢在指针还按着的时候进入系统移动循环；绕行事件循环会
                // 慢半拍，感知上就是窗口跟不上手。
                #[cfg(target_os = "windows")]
                window_chrome::begin_system_drag(hwnd);
                return;
            }
            "window.setDragStrip" => {
                window_chrome::set_drag_strip(
                    request.params["height"].as_f64().unwrap_or(0.0),
                    request.params["controls"].as_f64().unwrap_or(0.0),
                    request.params["maximize"].as_bool().unwrap_or(true),
                );
                return;
            }
            "tray.state" => {
                let response = json!({
                    "kind":"response",
                    "id":request.id,
                    "ok":true,
                    "result":{"enabled":tray_enabled_from_config(&config_path)}
                });
                let _ = proxy.send_event(UserEvent::ToWeb(response));
                return;
            }
            "tray.setEnabled" => {
                let response = match request.params["enabled"].as_bool() {
                    Some(enabled) => {
                        match sleepy_doll::config::AppConfig::set_tray_enabled(
                            &config_path,
                            enabled,
                        ) {
                            Ok(()) => {
                                #[cfg(target_os = "windows")]
                                let _ = proxy.send_event(UserEvent::TrayVisible(enabled));
                                json!({"kind":"response","id":request.id,"ok":true,"result":{"saved":true}})
                            }
                            Err(error) => {
                                json!({"kind":"response","id":request.id,"ok":false,"error":{"code":"NATIVE_ERROR","message":error.user_message()}})
                            }
                        }
                    }
                    None => {
                        json!({"kind":"response","id":request.id,"ok":false,"error":{"code":"INVALID_IPC","message":"enabled 必须是布尔值"}})
                    }
                };
                let _ = proxy.send_event(UserEvent::ToWeb(response));
                return;
            }
            _ => {}
        }
        if request.method == "bridge.setEnabled" {
            // 托盘菜单的勾选态要跟上界面里的开关；真正的执行仍在下面的线程里。
            if let Some(enabled) = request.params["enabled"].as_bool() {
                #[cfg(target_os = "windows")]
                let _ = proxy.send_event(UserEvent::BridgeState(enabled));
                let _ = enabled;
            }
        }
        if let Some(action) = window_chrome::Action::from_method(&request.method) {
            let _ = proxy.send_event(UserEvent::Window(action));
            return;
        }
    }
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

/// 从配置文件读托盘开关的当前值。读不到按默认开启处理，与 serde 默认一致。
fn tray_enabled_from_config(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value["tray"]["enabled"].as_bool())
        .unwrap_or(true)
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
