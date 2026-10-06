#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop_startup;
#[cfg(target_os = "windows")]
mod tray_popup;
#[cfg(target_os = "windows")]
mod tray_request;
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
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    window::{Window, WindowBuilder},
};
#[cfg(target_os = "windows")]
use wry::WebViewBuilderExtWindows;
use wry::{
    NewWindowResponse, WebContext, WebView, WebViewBuilder,
    http::{Request, Response, header::CONTENT_TYPE},
};

/// 让窗口与 WebView 共用明确的释放顺序；Drop 先隐藏父窗口，避免子视图关闭后露底。
struct DesktopSurface {
    webview: WebView,
    window: Window,
}

impl Drop for DesktopSurface {
    fn drop(&mut self) {
        self.window.set_visible(false);
    }
}

#[cfg(all(test, target_os = "windows"))]
mod shutdown_tests {
    use super::*;
    use tao::platform::windows::{EventLoopBuilderExtWindows, WindowExtWindows};

    #[test]
    #[ignore = "需要隔离的 WebView2 用户数据目录"]
    fn desktop_surface_hides_parent_before_webview_teardown() {
        let data =
            PathBuf::from(std::env::var_os("SLEEPY_DOLL_WINDOW_VALIDATION").expect("缺少验证目录"));
        let event_loop = EventLoopBuilder::<UserEvent>::with_user_event()
            .with_any_thread(true)
            .build();
        let window = WindowBuilder::new()
            .with_title("Sleepy Doll shutdown validation")
            .with_skip_taskbar(true)
            .with_position(PhysicalPosition::new(-32000, -32000))
            .build(&event_loop)
            .unwrap();
        let hwnd = window.hwnd();
        let mut context = WebContext::new(Some(data));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_html("<body style='background:#1b1b1b'>shutdown validation</body>")
            .build(&window)
            .unwrap();
        assert_ne!(
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd as _) },
            0
        );
        drop(DesktopSurface { webview, window });
        assert_eq!(
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd as _) },
            0
        );
    }
}

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
    OpenSettings,
    #[cfg(target_os = "windows")]
    OpenHelp,
    FrontendReady(bool),
    #[cfg(target_os = "windows")]
    ShowTrayMenu(f64, f64),
    #[cfg(target_os = "windows")]
    TrayAction(Value),
    #[cfg(target_os = "windows")]
    WindowTheme(bool),
    #[cfg(target_os = "windows")]
    BridgeFailed(String),
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
    /// 有没有运行在执行（含排队）。托盘菜单与气泡文字随它切换。
    #[cfg(target_os = "windows")]
    RunActivity(bool),
    /// 全局急停热键（Ctrl+Alt+Q）：取消正在进行的运行并把主窗口带回前台。
    #[cfg(target_os = "windows")]
    PanicStop,
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
        log::error!("Sleepy Doll 启动失败：{error}");
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
    {
        let distribution = controller.distribution.clone();
        thread::spawn(move || distribution.startup());
    }
    let startup_config = sleepy_doll::AppConfig::load(&config_path)?;
    if startup_config.host_plugin_enabled()
        && (startup_config.bridge.auto_start
            || (startup_config.bridge.enabled && sleepy_doll::bridge::control::is_host_running()))
    {
        let startup = controller.clone();
        thread::spawn(move || {
            // Opt-in startup may launch BetterGI; otherwise only attach to a running instance.
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
                if !watcher.bridge_enabled() || watcher.has_active_runs() {
                    continue;
                }
                let connected = watcher.bridge_connected();
                if connected && !watcher.bridge_stale() {
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
    #[cfg(target_os = "windows")]
    install_panic_hotkey(proxy.clone());
    {
        // 统计事件不再发官网：把白名单负载转交给界面，由其中的 Google tag 直连发送。
        // 启动早期（app_start 等）积压的事件会在注册时按序补发；FrontendReady
        // 前由事件循环的 pending_messages 缓存。
        let distribution = controller.distribution.clone();
        let analytics_proxy = proxy.clone();
        distribution.set_sink(Arc::new(move |payload| {
            let _ = analytics_proxy.send_event(UserEvent::ToWeb(
                json!({"kind":"event","id":"analytics","result":payload}),
            ));
        }));
    }
    let placement_path = window_placement_path(&user_directory);
    let placement = load_window_placement(&placement_path)
        .filter(|placement| placement_is_visible(placement, &event_loop));
    // 桥开关的当前值。
    #[cfg(target_os = "windows")]
    let bridge_enabled = Arc::new(AtomicBool::new(startup_config.bridge.enabled));
    // 有没有运行在执行。托盘文字随它切换，只由 RunActivity 事件写。
    #[cfg(target_os = "windows")]
    let run_active = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "windows")]
    {
        // 运行活动靠轮询：任务从界面、任务页、对话等任何入口启动都能被看到。
        let watcher = controller.clone();
        let activity_proxy = proxy.clone();
        let activity = run_active.clone();
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(2));
                let next = watcher.has_active_runs();
                if next != activity.load(Ordering::Relaxed) {
                    let _ = activity_proxy.send_event(UserEvent::RunActivity(next));
                }
            }
        });
    }
    #[cfg(target_os = "windows")]
    let mut tray = if startup_config.tray.enabled {
        Some(create_tray(
            proxy.clone(),
            bridge_enabled.load(Ordering::Relaxed),
            run_active.load(Ordering::Relaxed),
        )?)
    } else {
        None
    };
    // 窗口保持不透明：圆角由 DWM 切（Win11），Win10 上是方角。透明管线会被拖动等
    // DWM 状态变化打破，不能用。
    let startup_dark = std::fs::read_to_string(user_directory.join("tray-theme.txt"))
        .is_ok_and(|value| value.trim() == "dark");
    let builder = WindowBuilder::new()
        .with_visible(false)
        .with_background_color(desktop_startup::background(startup_dark))
        .with_title("Sleepy Doll")
        .with_inner_size(placement.map_or(
            Size::Logical(LogicalSize::new(1360.0, 860.0)),
            |placement| Size::Physical(PhysicalSize::new(placement.width, placement.height)),
        ))
        .with_min_inner_size(Size::Logical(LogicalSize::new(900.0, 620.0)));
    let builder = if let Some(placement) = placement {
        builder.with_position(Position::Physical(PhysicalPosition::new(
            placement.x,
            placement.y,
        )))
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
    let builder = WebViewBuilder::new_with_web_context(&mut web_context)
        .with_background_color(desktop_startup::background(startup_dark))
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
            open_external_url(&destination);
            false
        })
        .with_new_window_req_handler(|destination, _features| {
            // target=_blank 的外链同样交给系统浏览器，WebView 内不弹窗。
            open_external_url(&destination);
            NewWindowResponse::Deny
        })
        .with_url("sleepy://localhost/")
        .with_devtools(cfg!(debug_assertions))
        .with_clipboard(true);
    // WebView2 自带的右键菜单（刷新、检查、另存为）会把网页壳直接暴露给用户；
    // 原生菜单关闭后由界面层的自绘菜单提供复制、剪切与粘贴。
    #[cfg(target_os = "windows")]
    let builder = builder.with_default_context_menus(false);
    let webview = builder.build(&window)?;

    // 安装原窗口的无边框命中测试。
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
    #[cfg(target_os = "windows")]
    let mut shutting_down = false;
    let mut startup = desktop_startup::DesktopStartup::prime(
        &window,
        placement.is_some_and(|value| value.maximized),
    );
    let surface = DesktopSurface { webview, window };
    let mut pending_messages = Vec::new();
    #[cfg(target_os = "windows")]
    let mut bridge_busy = false;
    #[cfg(target_os = "windows")]
    let mut tray_popup: Option<tray_popup::TrayPopup> = None;
    #[cfg(target_os = "windows")]
    let mut tray_state = json!({"dark":startup_dark,"bridge":bridge_enabled.load(Ordering::Relaxed),"bridgeBusy":false,"active":run_active.load(Ordering::Relaxed)});
    event_loop.run(move |event, event_target, control_flow| {
        let window = &surface.window;
        let webview = &surface.webview;
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
        // 退出过程中忽略托盘/快捷键再次显示窗口，只等待清理完成。
        #[cfg(target_os = "windows")]
        if shutting_down
            && !matches!(
                event,
                Event::UserEvent(UserEvent::ShutdownFinished) | Event::LoopDestroyed
            )
        {
            return;
        }
        match event {
            Event::LoopDestroyed => {
                window.set_visible(false);
                #[cfg(target_os = "windows")]
                if let Some(popup) = tray_popup.as_mut() {
                    popup.hide();
                }
                #[cfg(target_os = "windows")]
                tray_popup::uninstall_outside_click_hook();
            }
            #[cfg(target_os = "windows")]
            Event::NewEvents(StartCause::Init) => {
                if tray.is_some() {
                    match tray_popup::TrayPopup::new(
                        event_target,
                        proxy.clone(),
                        &user_directory,
                        tray_state.clone(),
                    ) {
                        Ok(popup) => tray_popup = Some(popup),
                        Err(error) => log::warn!("托盘菜单预热失败：{error}"),
                    }
                    tray_popup::install_outside_click_hook(proxy.clone());
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ShowTrayMenu(x, y)) => {
                tray_popup::install_outside_click_hook(proxy.clone());
                if tray_popup.is_none() {
                    match tray_popup::TrayPopup::new(
                        event_target,
                        proxy.clone(),
                        &user_directory,
                        tray_state.clone(),
                    ) {
                        Ok(popup) => tray_popup = Some(popup),
                        Err(error) => log::warn!("托盘菜单不可用：{error}"),
                    }
                }
                if let Some(popup) = tray_popup.as_mut() {
                    popup.show(x, y, tray_state.clone());
                } else if let Some(tray) = tray.as_ref() {
                    use tray_icon::menu::ContextMenu;
                    unsafe {
                        tray.menu
                            .show_context_menu_for_hwnd(window_chrome::hwnd_id(window), None);
                    }
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::TrayAction(payload)) => {
                let action = payload["action"].as_str().unwrap_or("");
                if let Some(popup) = tray_popup.as_mut() {
                    match action {
                        "ready" => popup.ready(payload["height"].as_f64().unwrap_or(420.0)),
                        "shown" => popup.present(
                            payload["generation"].as_u64().unwrap_or(0),
                            payload["height"].as_f64().unwrap_or(420.0),
                        ),
                        "bridge" => {}
                        _ => popup.hide(),
                    }
                }
                let action = match action {
                    "open" => Some(UserEvent::ShowWindow),
                    "settings" => Some(UserEvent::OpenSettings),
                    "help" => Some(UserEvent::OpenHelp),
                    "folder" => Some(UserEvent::OpenUserDirectory),
                    "bridge" => Some(UserEvent::ToggleBridge),
                    "stop" => Some(UserEvent::PanicStop),
                    "quit" => Some(UserEvent::Quit),
                    _ => None,
                };
                if let Some(action) = action {
                    let _ = proxy.send_event(action);
                }
            }
            #[cfg(target_os = "windows")]
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Focused(false),
                ..
            } if tray_popup.as_ref().is_some_and(|popup| {
                popup.window.id() == window_id && popup.lost_external_focus()
            }) =>
            {
                if let Some(popup) = tray_popup.as_mut() {
                    popup.hide();
                }
            }
            #[cfg(target_os = "windows")]
            Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } if tray_popup
                .as_ref()
                .is_some_and(|popup| popup.window.id() == window_id) =>
            {
                if let Some(popup) = tray_popup.as_mut() {
                    popup.hide();
                }
            }
            Event::UserEvent(UserEvent::FrontendReady(dark)) => {
                if startup.is_ready() {
                    return;
                }
                let _ = webview.set_background_color(desktop_startup::background(dark));
                if startup.present(window, dark, true) {
                    for payload in std::mem::take(&mut pending_messages) {
                        let _ = proxy.send_event(UserEvent::ToWeb(payload));
                    }
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::WindowTheme(dark)) => {
                tray_state["dark"] = json!(dark);
                if let Some(popup) = tray_popup.as_mut() {
                    popup.update(tray_state.clone());
                }
                window.set_background_color(Some(desktop_startup::background(dark)));
                window.set_theme(Some(if dark {
                    tao::window::Theme::Dark
                } else {
                    tao::window::Theme::Light
                }));
                let _ = webview.set_background_color(desktop_startup::background(dark));
                let _ = std::fs::write(
                    user_directory.join("tray-theme.txt"),
                    if dark { "dark" } else { "light" },
                );
            }
            Event::UserEvent(UserEvent::ToWeb(payload)) => {
                if !startup.is_ready() {
                    pending_messages.push(payload);
                    return;
                }
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
                    close_window(
                        tray.as_ref(),
                        window,
                        &controller,
                        &proxy,
                        &mut shutting_down,
                    );
                }
                #[cfg(not(target_os = "windows"))]
                {
                    window.set_visible(false);
                    controller.shutdown();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                window_id,
                event: WindowEvent::Resized(size),
                ..
            } if window_id == window.id() => {
                if !startup.is_ready() {
                    return;
                }
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
                if !startup.is_ready() {
                    return;
                }
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
                    close_window(
                        tray.as_ref(),
                        window,
                        &controller,
                        &proxy,
                        &mut shutting_down,
                    );
                } else {
                    window_chrome::perform(window, action);
                }
                #[cfg(not(target_os = "windows"))]
                window_chrome::perform(window, action);
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
                if startup.is_ready() {
                    window.set_minimized(false);
                    window.set_visible(true);
                    window.set_focus();
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::OpenSettings | UserEvent::OpenHelp) => {
                if startup.is_ready() {
                    window.set_minimized(false);
                    window.set_visible(true);
                    window.set_focus();
                }
                let name = if matches!(event, Event::UserEvent(UserEvent::OpenSettings)) {
                    "openSettings"
                } else {
                    "openHelp"
                };
                let _ = proxy.send_event(UserEvent::ToWeb(json!({"kind":"event","id":name})));
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ToggleBridge) => {
                if bridge_busy {
                    return;
                }
                bridge_busy = true;
                tray_state["bridgeBusy"] = json!(true);
                if let Some(popup) = tray_popup.as_mut() {
                    popup.update(tray_state.clone());
                }
                if let Some(tray) = tray.as_ref() {
                    tray.bridge.set_enabled(false);
                }
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
                        Err(error) => {
                            log::warn!("托盘切换 BetterGI 失败：{error}");
                            let _ = proxy.send_event(UserEvent::BridgeFailed(error.user_message()));
                        }
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
                        match create_tray(
                            proxy.clone(),
                            bridge_enabled.load(Ordering::Relaxed),
                            run_active.load(Ordering::Relaxed),
                        ) {
                            Ok(handles) => tray = Some(handles),
                            Err(error) => log::warn!("托盘图标不可用: {error}"),
                        }
                    }
                    // 隐藏要连句柄一起丢弃：close_window 按句柄是否存在决定收进托盘
                    // 还是退出。丢弃 TrayIcon 时图标也从通知区移除。
                    (_, false) => {
                        tray = None;
                        if let Some(popup) = tray_popup.as_mut() {
                            popup.hide();
                        }
                    }
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::BridgeState(enabled)) => {
                tray_state["bridge"] = json!(enabled);
                tray_state["bridgeBusy"] = json!(false);
                if let Some(popup) = tray_popup.as_mut() {
                    popup.update(tray_state.clone());
                }
                bridge_enabled.store(enabled, Ordering::Relaxed);
                if let Some(tray) = tray.as_ref() {
                    tray.bridge.set_checked(enabled);
                    tray.bridge.set_enabled(true);
                }
                bridge_busy = false;
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::BridgeFailed(message)) => {
                tray_state["bridgeBusy"] = json!(false);
                if let Some(popup) = tray_popup.as_mut() {
                    popup.update(tray_state.clone());
                    popup.hide();
                }
                let enabled = bridge_enabled.load(Ordering::Relaxed);
                if let Some(tray) = tray.as_ref() {
                    tray.bridge.set_checked(enabled);
                    tray.bridge.set_enabled(true);
                }
                bridge_busy = false;
                let _ = proxy.send_event(UserEvent::ShowWindow);
                let _ = proxy.send_event(UserEvent::ToWeb(
                    json!({"kind":"event","id":"trayError","result":{"message":message}}),
                ));
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::RunActivity(active)) => {
                tray_state["active"] = json!(active);
                if let Some(popup) = tray_popup.as_mut() {
                    popup.update(tray_state.clone());
                }
                run_active.store(active, Ordering::Relaxed);
                if let Some(tray) = tray.as_ref() {
                    tray.set_active(active);
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::PanicStop) => {
                // 游戏在前台时用户按不回主窗口：急停要先掐掉所有运行，
                // 再把窗口带回前台，让对话重新可见。
                let stopped = controller.cancel_active_runs();
                if startup.is_ready() {
                    window.set_minimized(false);
                    window.set_visible(true);
                    window.set_focus();
                }
                window.request_user_attention(Some(tao::window::UserAttentionType::Critical));
                let _ = proxy.send_event(UserEvent::ToWeb(
                    json!({"kind":"event","id":"panicStop","result":{"stopped":stopped}}),
                ));
                if stopped > 0 {
                    log::warn!("急停热键：已请求停止 {stopped} 个运行");
                }
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::Quit) => {
                if let Some(popup) = tray_popup.as_mut() {
                    popup.hide();
                }
                begin_shutdown(window, &controller, &proxy, &mut shutting_down);
            }
            #[cfg(target_os = "windows")]
            Event::UserEvent(UserEvent::ShutdownFinished) => {
                if let Some(popup) = tray_popup.as_mut() {
                    popup.hide();
                }
                window.set_visible(false);
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
        if !matches!(*control_flow, ControlFlow::Exit)
            && let Some(deadline) = placement_deadline
        {
            *control_flow = ControlFlow::WaitUntil(deadline);
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
    shutting_down: &mut bool,
) {
    if tray.is_some() {
        fold_into_tray(window);
        return;
    }
    begin_shutdown(window, controller, proxy, shutting_down);
}

/// 先从屏幕移除窗口，再停止后台任务，最后由事件循环释放 WebView 与窗口。
#[cfg(target_os = "windows")]
fn begin_shutdown(
    window: &Window,
    controller: &Arc<AppController>,
    proxy: &EventLoopProxy<UserEvent>,
    shutting_down: &mut bool,
) {
    if *shutting_down {
        return;
    }
    *shutting_down = true;
    window.set_visible(false);
    let controller = controller.clone();
    let proxy = proxy.clone();
    thread::spawn(move || {
        controller.shutdown();
        let _ = proxy.send_event(UserEvent::ShutdownFinished);
    });
}

/// 全局急停热键 Ctrl+Alt+Q。在自己的线程上注册并跑一个消息循环：
/// RegisterHotKey 绑定线程，WM_HOTKEY 也投递到注册线程，收到后转发事件。
#[cfg(target_os = "windows")]
fn install_panic_hotkey(proxy: EventLoopProxy<UserEvent>) {
    thread::spawn(move || {
        use windows_sys::Win32::{
            Foundation::HWND,
            System::LibraryLoader::GetModuleHandleW,
            UI::Input::KeyboardAndMouse::{MOD_ALT, MOD_CONTROL, RegisterHotKey, UnregisterHotKey},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
                TranslateMessage, WM_HOTKEY, WNDCLASSW,
            },
        };
        let class_name: Vec<u16> = "SleepyDollPanicHotkey\0".encode_utf16().collect();
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                hInstance: instance,
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            };
            if RegisterClassW(&class) == 0 {
                log::warn!("急停热键窗口类注册失败，热键不可用");
                return;
            }
            let hwnd: HWND = CreateWindowExW(
                0, // 不可见的仅消息窗口：无样式、无尺寸
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                log::warn!("急停热键消息窗口创建失败，热键不可用");
                return;
            }
            // Ctrl+Alt+Q。0x51 是 'Q'。
            if RegisterHotKey(hwnd, 1, MOD_CONTROL | MOD_ALT, 0x0051) == 0 {
                log::warn!("急停热键 Ctrl+Alt+Q 注册失败，可能被其他程序占用");
                return;
            }
            let mut message = std::mem::zeroed();
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                if message.message == WM_HOTKEY && message.wParam == 1 {
                    let _ = proxy.send_event(UserEvent::PanicStop);
                }
                DispatchMessageW(&message);
            }
            let _ = UnregisterHotKey(hwnd, 1);
        }
    });
}

#[cfg(target_os = "windows")]
fn create_tray(
    proxy: EventLoopProxy<UserEvent>,
    bridge_enabled: bool,
    active: bool,
) -> Result<TrayHandles, Box<dyn std::error::Error>> {
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
        menu::{CheckMenuItem, IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };

    let menu = Menu::new();
    let status = MenuItem::new("Sleepy Doll · 就绪", false, None);
    let show = IconMenuItem::new(
        "打开主窗口",
        true,
        menu_glyph(MenuGlyph::Window, [104, 112, 122, 255]),
        None,
    );
    let bridge = CheckMenuItem::new("连接 BetterGI", true, bridge_enabled, None);
    let settings = IconMenuItem::new(
        "设置",
        true,
        menu_glyph(MenuGlyph::Settings, [104, 112, 122, 255]),
        None,
    );
    let help = IconMenuItem::new(
        "使用说明与更新",
        true,
        menu_glyph(MenuGlyph::Help, [104, 112, 122, 255]),
        None,
    );
    let open_user = IconMenuItem::new(
        "打开数据文件夹",
        true,
        menu_glyph(MenuGlyph::Folder, [104, 112, 122, 255]),
        None,
    );
    let stop = IconMenuItem::new(
        "停止所有任务\tCtrl+Alt+Q",
        active,
        menu_glyph(MenuGlyph::Stop, [180, 77, 64, 255]),
        None,
    );
    let quit = IconMenuItem::new(
        "退出 Sleepy Doll",
        true,
        menu_glyph(MenuGlyph::Exit, [104, 112, 122, 255]),
        None,
    );
    menu.append_items(&[
        &status,
        &PredefinedMenuItem::separator(),
        &show,
        &settings,
        &help,
        &open_user,
        &PredefinedMenuItem::separator(),
        &bridge,
        &stop,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    let status_item = status.clone();
    let show_id = show.id().clone();
    let bridge_id = bridge.id().clone();
    let settings_id = settings.id().clone();
    let help_id = help.id().clone();
    let stop_id = stop.id().clone();
    let open_user_id = open_user.id().clone();
    let quit_id = quit.id().clone();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = if event.id == show_id {
            Some(UserEvent::ShowWindow)
        } else if event.id == bridge_id {
            Some(UserEvent::ToggleBridge)
        } else if event.id == settings_id {
            Some(UserEvent::OpenSettings)
        } else if event.id == help_id {
            Some(UserEvent::OpenHelp)
        } else if event.id == stop_id {
            Some(UserEvent::PanicStop)
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
            button,
            button_state: MouseButtonState::Up,
            position,
            ..
        } = event
        {
            match button {
                MouseButton::Left => {
                    let _ = proxy.send_event(UserEvent::ShowWindow);
                }
                MouseButton::Right => {
                    let _ = proxy.send_event(UserEvent::ShowTrayMenu(position.x, position.y));
                }
                _ => {}
            }
        }
    }));
    let mut builder = TrayIconBuilder::new()
        .with_tooltip("Sleepy Doll · 关闭窗口后继续运行")
        .with_menu_on_left_click(false);
    // 图标编在 exe 的资源里，界面、窗口和托盘共用同一份。
    match Icon::from_resource(APP_ICON, Some((32, 32))) {
        Ok(icon) => builder = builder.with_icon(icon),
        Err(error) => log::warn!("托盘图标不可用: {error}"),
    }
    let handles = TrayHandles {
        menu,
        icon: builder.build()?,
        bridge,
        status: status_item,
        stop,
        quit,
    };
    handles.set_active(active);
    Ok(handles)
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
enum MenuGlyph {
    Window,
    Folder,
    Exit,
    Stop,
    Settings,
    Help,
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
                    (x == 7 && (2..=7).contains(&y))
                        || (y == 13 && (5..=10).contains(&x))
                        || (matches!(x, 2 | 13) && (6..=10).contains(&y))
                        || (matches!(
                            (x, y),
                            (3, 4)
                                | (4, 3)
                                | (11, 3)
                                | (12, 4)
                                | (3, 11)
                                | (4, 12)
                                | (11, 12)
                                | (12, 11)
                        ))
                }
                MenuGlyph::Stop => {
                    ((3..=12).contains(&x) && matches!(y, 3 | 12))
                        || ((3..=12).contains(&y) && matches!(x, 3 | 12))
                }
                MenuGlyph::Settings => {
                    ((3..=12).contains(&x) && matches!(y, 4 | 11))
                        || (matches!(x, 5 | 6) && (2..=6).contains(&y))
                        || (matches!(x, 9 | 10) && (9..=13).contains(&y))
                }
                MenuGlyph::Help => {
                    ((3..=12).contains(&x) && matches!(y, 2 | 13))
                        || ((3..=12).contains(&y) && matches!(x, 2 | 13))
                        || (matches!(x, 7 | 8) && matches!(y, 4 | 7 | 8 | 10))
                        || (x == 9 && matches!(y, 5 | 6))
                        || (x == 6 && y == 5)
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
    menu: tray_icon::menu::Menu,
    icon: tray_icon::TrayIcon,
    bridge: tray_icon::menu::CheckMenuItem,
    /// 只读状态行：有运行进行中时提示急停热键。
    status: tray_icon::menu::MenuItem,
    stop: tray_icon::menu::IconMenuItem,
    quit: tray_icon::menu::IconMenuItem,
}

#[cfg(target_os = "windows")]
impl TrayHandles {
    fn set_active(&self, active: bool) {
        self.status.set_text(if active {
            "Sleepy Doll · 任务进行中"
        } else {
            "Sleepy Doll · 就绪"
        });
        self.stop.set_enabled(active);
        self.quit.set_text(if active {
            "停止任务并退出"
        } else {
            "退出 Sleepy Doll"
        });
        let _ = self.icon.set_tooltip(Some(if active {
            "Sleepy Doll · 任务进行中\nCtrl+Alt+Q 停止所有任务"
        } else {
            "Sleepy Doll · 就绪\n单击打开主窗口，右键查看更多"
        }));
    }
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
            #[cfg(target_os = "windows")]
            "release.install" => {
                let outcome = (|| -> Result<(), Box<dyn std::error::Error>> {
                    if controller.has_active_runs() {
                        return Err("请先结束正在运行的任务，再安装更新".into());
                    }
                    let installed = sleepy_doll::setup::installed()
                        .ok_or("当前程序没有安装登记，请通过安装程序更新")?;
                    let executable = std::env::current_exe()?;
                    if executable
                        .parent()
                        .ok_or("程序目录不存在")?
                        .canonicalize()?
                        != installed.directory.canonicalize()?
                    {
                        return Err("当前程序不在已登记的安装目录，请通过安装程序更新".into());
                    }
                    let installer = controller.distribution.installer()?;
                    std::process::Command::new(installer)
                        .arg("--apply-update")
                        .arg(std::process::id().to_string())
                        .spawn()?;
                    Ok(())
                })();
                let response = match outcome {
                    Ok(()) => {
                        let _ = proxy.send_event(UserEvent::Quit);
                        json!({"kind":"response","id":request.id,"ok":true,"result":{"installing":true}})
                    }
                    Err(error) => {
                        json!({"kind":"response","id":request.id,"ok":false,"error":{"code":"UPDATE_ERROR","message":error.to_string()}})
                    }
                };
                let _ = proxy.send_event(UserEvent::ToWeb(response));
                return;
            }
            "window.ready" => {
                let _ = proxy.send_event(UserEvent::FrontendReady(
                    request.params["dark"].as_bool().unwrap_or(false),
                ));
                return;
            }
            #[cfg(target_os = "windows")]
            "window.setTheme" => {
                let _ = proxy.send_event(UserEvent::WindowTheme(request.params["theme"] == "dark"));
                return;
            }
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
            "window.resize" => {
                if let Some(direction) = request.params["direction"].as_str() {
                    window_chrome::begin_system_resize(hwnd, direction);
                }
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

/// 把 HTTP(S) 链接交给系统默认浏览器；其他 scheme 不处理。打开失败只记日志。
fn open_external_url(destination: &str) {
    if !url::Url::parse(destination).is_ok_and(|url| matches!(url.scheme(), "http" | "https")) {
        return;
    }
    #[cfg(target_os = "windows")]
    let opened = std::process::Command::new("explorer.exe")
        .arg(destination)
        .spawn();
    #[cfg(target_os = "linux")]
    let opened = std::process::Command::new("xdg-open")
        .arg(destination)
        .spawn();
    #[cfg(target_os = "macos")]
    let opened = std::process::Command::new("open").arg(destination).spawn();
    #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
    if let Err(error) = opened {
        log::warn!("系统浏览器打开 {destination} 失败：{error}");
    }
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
