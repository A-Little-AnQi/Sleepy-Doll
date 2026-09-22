#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod window_chrome;

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sleepy_doll::{app::AppController, logging, runtime::types::Event as AgentEvent};
#[cfg(target_os = "windows")]
use tao::platform::windows::{IconExtWindows, WindowBuilderExtWindows};
use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize, Position, Size},
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

/// 窗口图标与任务栏图标取 exe 资源里的同一份；取不到只影响外观。
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

/// 界面靠这两个标记判断自己运行在桌面壳里、以及标题栏要不要自绘。
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
    #[cfg(target_os = "windows")]
    ToggleBridge,
    #[cfg(target_os = "windows")]
    OpenUserDirectory,
    /// 显隐托盘图标。开关的持久化在发起侧完成，这里只在主线程上改界面。
    #[cfg(target_os = "windows")]
    TrayVisible(bool),
    /// 同步托盘菜单里桥开关的勾选态。
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowPlacement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Sleepy Doll 启动失败：{error}");
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
    // 用户目录是配置文件的所在目录，日志与 WebView2 的缓存都放进去。
    let user_directory = config_path
        .parent()
        .ok_or("配置路径没有所在目录，无法定位用户目录")?
        .to_path_buf();
    if let Err(error) = logging::init(&user_directory) {
        eprintln!("无法写入日志（{error}），本次运行的记录只有标准错误。");
    }
    log::info!(
        "Sleepy Doll {} 启动，配置 {}",
        env!("CARGO_PKG_VERSION"),
        config_path.display()
    );
    let controller = Arc::new(AppController::load(&config_path)?);
    let startup_config = sleepy_doll::AppConfig::load(&config_path)?;
    if startup_config.host_plugin_enabled() && startup_config.bridge.enabled {
        let startup = controller.clone();
        thread::spawn(move || {
            // 后台自动连接，失败原因只记日志。
            if let Err(error) = startup.handle(
                "bridge.setEnabled",
                json!({"enabled":true}),
                Arc::new(|_, _| {}),
            ) {
                log::warn!("启动时自动连接 BetterGI 失败：{error}");
            }
        });
    }
    {
        // 宿主生命周期跟随：桥开关开着、宿主在运行而连接断着时自动接回。
        // 只重连不拉起：拉起 BetterGI 只发生在用户主动连接时。
        let watcher = controller.clone();
        thread::spawn(move || {
            let cooldown = Duration::from_secs(30);
            let mut next_attempt = std::time::Instant::now();
            let mut last_failure = String::new();
            loop {
                thread::sleep(Duration::from_secs(5));
                if !watcher.bridge_enabled() || watcher.bridge_connected() {
                    continue;
                }
                if !sleepy_doll::bridge::control::is_host_running()
                    || std::time::Instant::now() < next_attempt
                {
                    continue;
                }
                next_attempt = std::time::Instant::now() + cooldown;
                match watcher.connect_bridge() {
                    Ok(()) => last_failure.clear(),
                    Err(error) => {
                        // 同一个失败原因只记一次，避免日志被重试刷屏。
                        if last_failure != error.to_string() {
                            last_failure = error.to_string();
                            log::warn!("自动重连 BetterGI 失败：{error}");
                        }
                    }
                }
            }
        });
    }
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let placement_path = window_placement_path(&user_directory);
    let placement = load_window_placement(&placement_path)
        .filter(|placement| placement_is_visible(placement, &event_loop));
    // 桥开关的当前值。
    #[cfg(target_os = "windows")]
    let bridge_enabled = Arc::new(AtomicBool::new(startup_config.bridge.enabled));
    #[cfg(target_os = "windows")]
    let mut tray = if startup_config.tray.enabled {
        Some(create_tray(
            proxy.clone(),
            bridge_enabled.load(Ordering::Relaxed),
        )?)
    } else {
        None
    };
    // 窗口保持不透明：圆角由 DWM 切（Win11），Win10 上是方角。透明管线会被拖动等
    // DWM 状态变化打破，不能用。
    let builder = WindowBuilder::new()
        .with_title("Sleepy Doll")
        .with_inner_size(placement.map_or(
            Size::Logical(LogicalSize::new(1360.0, 860.0)),
            |placement| Size::Physical(PhysicalSize::new(placement.width, placement.height)),
        ))
        .with_min_inner_size(Size::Logical(LogicalSize::new(900.0, 620.0)));
    let builder = if let Some(placement) = placement {
        builder
            .with_position(Position::Physical(PhysicalPosition::new(
                placement.x,
                placement.y,
            )))
            .with_maximized(placement.maximized)
    } else {
        builder
    };
    // tao 建窗口的过程中会把图标置空，两个图标要在窗口创建时就设上；
    // window_chrome::install() 要等 WebView2 启动完。
    #[cfg(target_os = "windows")]
    let builder = builder
        .with_window_icon(shell_icon(16))
        .with_taskbar_icon(shell_icon(32));
    // 标题栏由界面自绘（见 window_chrome），系统那一条要先摘掉。
    #[cfg(target_os = "windows")]
    let builder = builder.with_decorations(false);
    let window = builder.build(&event_loop)?;

    // WebView2 默认把用户数据放在 exe 旁边，那是升级会覆盖的位置。
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
            // Markdown 链接在系统浏览器里打开。
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

    // 子窗口在 WebView2 建好后才存在，边缘命中测试要交给它。
    window_chrome::install(&window);
    let mut maximized = window.is_maximized();
    let mut normal_position = placement
        .map(|placement| PhysicalPosition::new(placement.x, placement.y))
        .or_else(|| window.outer_position().ok())
        .unwrap_or_default();
    let mut normal_size = placement
        .map(|placement| PhysicalSize::new(placement.width, placement.height))
        .unwrap_or_else(|| window.inner_size());
    let mut placement_deadline = None;

    event_loop.run(move |event, _, control_flow| {
        if placement_deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            save_window_placement(
                &placement_path,
                normal_position,
                normal_size,
                window.is_maximized(),
            );
            placement_deadline = None;
        }
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(UserEvent::ToWeb(payload)) => {
                if let Ok(serialized) = serde_json::to_string(&payload) {
                    let _ = webview
                        .evaluate_script(&format!("window.__sleepyDollReceive?.({serialized});"));
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } if window_id == window.id() => {
                #[cfg(target_os = "windows")]
                {
                    save_window_placement(
                        &placement_path,
                        normal_position,
                        normal_size,
                        window.is_maximized(),
                    );
                    close_window(tray.as_ref(), &window, &controller, &proxy);
                }
                #[cfg(not(target_os = "windows"))]
                {
                    controller.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Resized(size),
                ..
            } if window_id == window.id() => {
                if !window.is_maximized() && !window.is_minimized() {
                    normal_size = size;
                }
                placement_deadline = Some(std::time::Instant::now() + Duration::from_millis(250));
                // 只有真正变了才通知。
                let current = window.is_maximized();
                if current != maximized {
                    maximized = current;
                    let _ = webview.evaluate_script(&format!(
                        "window.__sleepyDollWindow?.({});",
                        json!({ "maximized": current })
                    ));
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Moved(position),
                ..
            } if window_id == window.id() => {
                if !window.is_maximized() && !window.is_minimized() {
                    normal_position = position;
                }
                placement_deadline = Some(std::time::Instant::now() + Duration::from_millis(250));
            }
            Event::UserEvent(UserEvent::Window(action)) => {
                #[cfg(target_os = "windows")]
                if action == window_chrome::Action::Close {
                    save_window_placement(
                        &placement_path,
                        normal_position,
                        normal_size,
                        window.is_maximized(),
                    );
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
            Event::UserEvent(UserEvent::ToggleBridge) => {
                let enabled = !bridge_enabled.load(Ordering::Relaxed);
                let controller = controller.clone();
                let proxy = proxy.clone();
                thread::spawn(move || {
                    match controller.handle(
                        "bridge.setEnabled",
                        json!({"enabled":enabled}),
                        Arc::new(|_, _| {}),
                    ) {
                        Ok(_) => {
                            let _ = proxy.send_event(UserEvent::BridgeState(enabled));
                        }
                        Err(error) => log::warn!("托盘切换 BetterGI 失败：{error}"),
                    }
                });
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::OpenUserDirectory) => {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&user_directory)
                    .spawn();
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::TrayVisible(visible)) => {
                match (tray.as_mut(), visible) {
                    (Some(tray), true) => {
                        let _ = tray.icon.set_visible(true);
                    }
                    // 图标还没建过且要显示时才建，勾选态取当前桥状态。
                    (None, true) => {
                        match create_tray(proxy.clone(), bridge_enabled.load(Ordering::Relaxed)) {
                            Ok(handles) => tray = Some(handles),
                            Err(error) => log::warn!("托盘图标不可用: {error}"),
                        }
                    }
                    // 隐藏要连句柄一起丢弃：close_window 按句柄是否存在决定收进托盘
                    // 还是退出。丢弃 TrayIcon 时图标也从通知区移除。
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
        if !matches!(*control_flow, ControlFlow::Exit) {
            if let Some(deadline) = placement_deadline {
                *control_flow = ControlFlow::WaitUntil(deadline);
            }
        }
    });
}

/// WebView2 的用户数据目录，与配置、日志同级。
fn webview_data_directory(user_directory: &Path) -> std::path::PathBuf {
    user_directory.join(".sleepy-doll").join("webview2")
}

fn window_placement_path(user_directory: &Path) -> PathBuf {
    user_directory
        .join(".sleepy-doll")
        .join("window-placement.json")
}

fn load_window_placement(path: &Path) -> Option<WindowPlacement> {
    let placement: WindowPlacement = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (placement.width >= 900 && placement.height >= 620).then_some(placement)
}

fn placement_is_visible(
    placement: &WindowPlacement,
    event_loop: &tao::event_loop::EventLoop<UserEvent>,
) -> bool {
    let right = i64::from(placement.x) + i64::from(placement.width);
    let bottom = i64::from(placement.y) + i64::from(placement.height);
    event_loop.available_monitors().any(|monitor| {
        let origin = monitor.position();
        let size = monitor.size();
        let monitor_right = i64::from(origin.x) + i64::from(size.width);
        let monitor_bottom = i64::from(origin.y) + i64::from(size.height);
        let visible_width = right.min(monitor_right) - i64::from(placement.x.max(origin.x));
        let visible_height = bottom.min(monitor_bottom) - i64::from(placement.y.max(origin.y));
        visible_width >= 80 && visible_height >= 80
    })
}

fn save_window_placement(
    path: &Path,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    maximized: bool,
) {
    let placement = WindowPlacement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized,
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let result = serde_json::to_vec(&placement)
        .map_err(|error| error.to_string())
        .and_then(|value| std::fs::write(path, value).map_err(|error| error.to_string()));
    if let Err(error) = result {
        log::warn!("窗口位置保存失败：{error}");
    }
}

/// 把窗口收进托盘。
#[cfg(target_os = "windows")]
fn fold_into_tray(window: &Window) {
    window.set_visible(false);
}

/// 托盘可用时关闭窗口收进托盘，否则直接走退出流程。
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
    bridge_enabled: bool,
) -> Result<TrayHandles, Box<dyn std::error::Error>> {
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
        menu::{CheckMenuItem, IconMenuItem, Menu, MenuEvent, PredefinedMenuItem},
    };

    let menu = Menu::new();
    let show = IconMenuItem::new(
        "显示 Sleepy Doll",
        true,
        menu_glyph(MenuGlyph::Window, [151, 116, 38, 255]),
        None,
    );
    let bridge = CheckMenuItem::new("BetterGI 桥", true, bridge_enabled, None);
    let open_user = IconMenuItem::new(
        "打开配置目录",
        true,
        menu_glyph(MenuGlyph::Folder, [102, 102, 102, 255]),
        None,
    );
    let quit = IconMenuItem::new(
        "停止任务并退出",
        true,
        menu_glyph(MenuGlyph::Exit, [196, 43, 28, 255]),
        None,
    );
    menu.append_items(&[
        &show,
        &PredefinedMenuItem::separator(),
        &bridge,
        &open_user,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    let show_id = show.id().clone();
    let bridge_id = bridge.id().clone();
    let open_user_id = open_user.id().clone();
    let quit_id = quit.id().clone();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = if event.id == show_id {
            Some(UserEvent::ShowWindow)
        } else if event.id == bridge_id {
            Some(UserEvent::ToggleBridge)
        } else if event.id == open_user_id {
            Some(UserEvent::OpenUserDirectory)
        } else if event.id == quit_id {
            Some(UserEvent::Quit)
        } else {
            None
        };
        if let Some(action) = action {
            let _ = menu_proxy.send_event(action);
        }
    }));

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
    // 图标编在 exe 的资源里，界面、窗口和托盘共用同一份。
    match Icon::from_resource(APP_ICON, Some((32, 32))) {
        Ok(icon) => builder = builder.with_icon(icon),
        Err(error) => log::warn!("托盘图标不可用: {error}"),
    }
    Ok(TrayHandles {
        icon: builder.build()?,
        bridge,
    })
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
enum MenuGlyph {
    Window,
    Folder,
    Exit,
}

#[cfg(target_os = "windows")]
fn menu_glyph(glyph: MenuGlyph, color: [u8; 4]) -> Option<tray_icon::menu::Icon> {
    let mut rgba = vec![0; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let on = match glyph {
                MenuGlyph::Window => {
                    ((2..=13).contains(&x) && matches!(y, 2 | 10))
                        || ((2..=10).contains(&y) && matches!(x, 2 | 13))
                        || (y == 13 && (5..=10).contains(&x))
                        || (y == 11 && (7..=8).contains(&x))
                        || (y == 12 && (6..=9).contains(&x))
                }
                MenuGlyph::Folder => {
                    (y == 3 && (2..=6).contains(&x))
                        || (y == 4 && (2..=13).contains(&x))
                        || (y == 12 && (2..=13).contains(&x))
                        || ((4..=12).contains(&y) && matches!(x, 2 | 13))
                }
                MenuGlyph::Exit => {
                    (4..=11).contains(&x) && (4..=11).contains(&y) && (x == y || x + y == 15)
                }
            };
            if on {
                let offset = (y * 16 + x) * 4;
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    tray_icon::menu::Icon::from_rgba(rgba, 16, 16).ok()
}

/// 事件循环持有的托盘句柄。
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
                // 拖动要在指针还按着的时候进入系统移动循环，绕行事件循环会慢半拍。
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
            // 托盘菜单的勾选态跟上界面里的开关。
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
                    // 界面只显示一句用户可读的提示，方法名与内部错误留给日志。
                    log::warn!("IPC {} 失败：{error}", request.method);
                    json!({"kind":"response","id":request.id,"ok":false,"error":IpcError { code:"NATIVE_ERROR", message:error.user_message() }})
                }
            };
            let _ = proxy.send_event(UserEvent::ToWeb(response));
        }
        Err(error) => {
            log::warn!("IPC 请求无法解析：{error}");
            let _ = proxy.send_event(UserEvent::ToWeb(json!({"kind":"response","id":"unknown","ok":false,"error":{"code":"INVALID_IPC","message":error.to_string()}})));
        }
    });
}

/// 从配置文件读托盘开关，读不到按默认开启处理。
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
