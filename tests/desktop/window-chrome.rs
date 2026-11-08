//! 真实 WebView2 加载生产前端，使用隔离配置和演示 IPC 验证窗口交互。
#![cfg(windows)]
#[path = "../../src/window_chrome.rs"]
#[allow(dead_code, unused_imports)]
mod window_chrome;

use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::{run_return::EventLoopExtRunReturn, windows::EventLoopBuilderExtWindows},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder, http::Response};

#[test]
#[ignore = "由 scheduler-ui-check.mjs --webview2 连接隔离的真实 WebView2"]
fn scheduler_layout_driver() {
    use std::net::TcpListener;
    use wry::WebViewBuilderExtWindows;
    let directory =
        PathBuf::from(std::env::var_os("SLEEPY_DOLL_WINDOW_VALIDATION").expect("缺少隔离验证目录"));
    std::fs::create_dir_all(&directory).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut events = EventLoopBuilder::<String>::with_user_event()
        .with_any_thread(true)
        .build();
    let proxy = events.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Sleepy Doll · 定时任务布局验证")
        .with_decorations(false)
        .with_focused(false)
        .with_visible(false)
        .with_inner_size(LogicalSize::new(1456.0, 900.0))
        .build(&events)
        .unwrap();
    let mut context = WebContext::new(Some(directory.join("profile")));
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_additional_browser_args(format!("--remote-debugging-port={port} --disable-renderer-backgrounding --disable-background-timer-throttling"))
        .with_initialization_script(
            "window.__SLEEPY_DOLL_DESKTOP__=true;window.__SLEEPY_DOLL_FRAMELESS__=true;",
        )
        .with_ipc_handler(move |request| {
            let _ = proxy.send_event(request.body().clone());
        })
        .with_url("about:blank")
        .build(&window)
        .unwrap();
    std::fs::write(directory.join("cdp.port"), port.to_string()).unwrap();
    println!(
        "WebView2 layout driver: pid={} port={port}",
        std::process::id()
    );
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut completed = false;
    events.run_return(|event, _, control| {
        *control = ControlFlow::WaitUntil(deadline);
        match event {
            Event::UserEvent(value) => {
                if serde_json::from_str::<Value>(&value)
                    .is_ok_and(|message| message["method"] == "probe.done")
                {
                    completed = serde_json::from_str::<Value>(&value).unwrap()["success"] == true;
                    *control = ControlFlow::Exit;
                } else {
                    let fixture_port = std::fs::read_to_string(
                        directory.parent().unwrap().join("native-ipc.port"),
                    )
                    .unwrap();
                    let request: Value = serde_json::from_str(&value).unwrap();
                    let mut reply: Value =
                        ureq::post(format!("http://127.0.0.1:{}/ipc", fixture_port.trim()))
                            .send_json(&request)
                            .unwrap()
                            .body_mut()
                            .read_json()
                            .unwrap();
                    reply["kind"] = json!("response");
                    webview
                        .evaluate_script(&format!("window.__sleepyDollReceive?.({reply});"))
                        .unwrap();
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control = ControlFlow::Exit;
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. })
                if Instant::now() >= deadline =>
            {
                *control = ControlFlow::Exit
            }
            _ => {}
        }
    });
    window.set_visible(false);
    drop(webview);
    drop(context);
    assert!(completed, "原生布局验证未收到结束信号");
}

#[test]
fn drag_request_returns_before_entering_the_windows_move_loop() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::WindowsAndMessaging::{HTCAPTION, WM_NCLBUTTONDOWN},
    };
    type Proc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT;
    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(hwnd: HWND, proc: Option<Proc>, id: usize, data: usize) -> i32;
        fn DefSubclassProc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    }
    unsafe extern "system" fn observe_move(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
        _: usize,
        data: usize,
    ) -> LRESULT {
        if message == WM_NCLBUTTONDOWN && w == HTCAPTION as usize {
            unsafe {
                (*(data as *const AtomicBool)).store(true, Ordering::Relaxed);
            }
            // 验证消息顺序，不实际移动窗口或注入鼠标输入。
            return 0;
        }
        unsafe { DefSubclassProc(hwnd, message, w, l) }
    }
    let mut events = EventLoopBuilder::<()>::with_user_event()
        .with_any_thread(true)
        .build();
    let window = WindowBuilder::new()
        .with_visible(false)
        .build(&events)
        .unwrap();
    let hwnd = window_chrome::hwnd_id(&window);
    let entered = AtomicBool::new(false);
    assert_ne!(
        unsafe {
            SetWindowSubclass(
                hwnd as _,
                Some(observe_move),
                0x534454,
                (&entered as *const AtomicBool) as usize,
            )
        },
        0
    );
    window_chrome::begin_system_drag(hwnd);
    assert!(
        !entered.load(Ordering::Relaxed),
        "进入系统移动循环前必须先返回 WebView2 回调"
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    events.run_return(|event, _, control| {
        *control = ControlFlow::WaitUntil(deadline);
        if entered.load(Ordering::Relaxed)
            || matches!(
                event,
                Event::NewEvents(StartCause::ResumeTimeReached { .. })
            )
        {
            *control = ControlFlow::Exit;
        }
    });
    assert!(entered.load(Ordering::Relaxed), "未收到投递的系统移动请求");
}

#[test]
#[ignore = "需要 SLEEPY_DOLL_WINDOW_VALIDATION 指定隔离的 WebView2 数据目录"]
fn production_caption_and_mascot() {
    let data =
        PathBuf::from(std::env::var_os("SLEEPY_DOLL_WINDOW_VALIDATION").expect("缺少验证目录"));
    let interactive = std::env::var("SLEEPY_DOLL_WINDOW_INTERACTIVE").as_deref() == Ok("1");
    let mut events = EventLoopBuilder::<String>::with_user_event()
        .with_any_thread(true)
        .build();
    let proxy = events.create_proxy();
    let window = WindowBuilder::new()
        .with_title("Sleepy Doll interaction validation")
        .with_visible(false)
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(1100.0, 760.0))
        .build(&events)
        .unwrap();
    let hwnd = window_chrome::hwnd_id(&window);
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ui");
    let mut context = WebContext::new(Some(data));
    let init = format!(
        r#"
        window.__SLEEPY_DOLL_DESKTOP__ = true;
        window.__SLEEPY_DOLL_FRAMELESS__ = true;
        localStorage.setItem('sleepy-doll-theme', 'dark');
        localStorage.setItem('sleepy-doll-locale', 'zh');
        localStorage.setItem('sleepy-doll-version', '{}');
        window.imageDrags = 0;
        document.addEventListener('dragstart', event => {{
            if (event.target?.classList?.contains('welcome-mascot')) window.imageDrags++;
        }});
    "#,
        env!("CARGO_PKG_VERSION")
    );
    let webview = WebViewBuilder::new_with_web_context(&mut context)
        .with_initialization_script(&init)
        .with_custom_protocol("sleepy".into(), move |_, request| {
            let path = request.uri().path().trim_start_matches('/');
            let path = if path.is_empty() { "index.html" } else { path };
            let body = if !path.split('/').any(|part| part == "..") {
                std::fs::read(assets.join(path)).ok()
            } else {
                None
            };
            let Some(body) = body else {
                println!("Fixture asset unavailable: {path}");
                return Response::builder()
                    .status(404)
                    .body(std::borrow::Cow::Owned(Vec::new()))
                    .unwrap();
            };
            Response::builder()
                .header(
                    "content-type",
                    mime_guess::from_path(path).first_or_octet_stream().as_ref(),
                )
                .header("cache-control", "no-store")
                .body(std::borrow::Cow::Owned(body))
                .unwrap()
        })
        .with_ipc_handler(move |request| {
            let value: Value = serde_json::from_str(request.body()).unwrap();
            match value["method"].as_str().unwrap_or_default() {
                "window.drag" => window_chrome::begin_system_drag(hwnd),
                "window.setDragStrip" => window_chrome::set_drag_strip(
                    value["params"]["height"].as_f64().unwrap(),
                    value["params"]["controls"].as_f64().unwrap(),
                    true,
                ),
                _ => {
                    let _ = proxy.send_event(request.body().clone());
                }
            }
        })
        .with_url("sleepy://localhost/")
        .build(&window)
        .unwrap();
    window_chrome::install(&window);
    let bootstrap = json!({
        "configPath":"fixture", "models":[], "skills":[], "tools":[],
        "runtimeToolLabels":{}, "plugins":[], "conversations":[], "tasks":[],
        "strategies":[], "workflows":[], "operations":[], "resources":[],
        "diagnostics":[], "notifications":[],
        "conversationGroups":{"groups":[],"membership":{},"order":[]},
        "permission":{"mode":"fullAccess","label":"允许执行","levels":[]},
        "bridge":{"enabled":false,"connected":false,"baseUrl":"fixture","launchSilently":false}
    });
    let deadline = Instant::now() + Duration::from_secs(if interactive { 120 } else { 20 });
    let mut verified = false;
    events.run_return(|event, _, control| {
        *control = ControlFlow::WaitUntil(deadline);
        match event {
            Event::UserEvent(message) => {
                let request: Value = serde_json::from_str(&message).unwrap();
                match request["method"].as_str().unwrap_or_default() {
                    "window.ready" => {
                        if interactive { window.set_visible(true); }
                        webview.evaluate_script(r#"
                            const report = () => {
                                const image = document.querySelector('.welcome-mascot');
                                if (!image) return;
                                const css = getComputedStyle(image);
                                window.ipc.postMessage(JSON.stringify({method:'probe.report',params:{
                                    caption:getComputedStyle(document.querySelector('.title-bar')).getPropertyValue('app-region'),
                                    controls:getComputedStyle(document.querySelector('.title-bar-controls')).getPropertyValue('app-region'),
                                    pointerEvents:css.pointerEvents, userDrag:css.getPropertyValue('-webkit-user-drag'),
                                    imageDrags:window.imageDrags
                                }}));
                            };
                            report(); setInterval(report, 500);
                        "#).unwrap();
                    }
                    "probe.report" => {
                        let p = &request["params"];
                        assert_eq!(p["caption"], "drag");
                        assert_eq!(p["controls"], "no-drag");
                        assert_eq!(p["pointerEvents"], "none");
                        assert_eq!(p["userDrag"], "none");
                        assert_eq!(p["imageDrags"], 0);
                        if !verified { println!("Production WebView2 probe: {p}"); }
                        verified = true;
                        if !interactive { *control = ControlFlow::Exit; }
                    }
                    method if method.starts_with("window.") => {}
                    method => {
                        let result = match method {
                            "bootstrap" => bootstrap.clone(),
                            "bridge.status" => bootstrap["bridge"].clone(),
                            "tray.state" => json!({"enabled":true}),
                            _ => json!({}),
                        };
                        let reply = json!({"kind":"response","id":request["id"],"ok":true,"result":result});
                        webview.evaluate_script(&format!("window.__sleepyDollReceive?.({reply});")).unwrap();
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::Moved(position), .. } if interactive => {
                println!("Native window moved: {},{}", position.x, position.y);
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => *control = ControlFlow::Exit,
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) if Instant::now() >= deadline => *control = ControlFlow::Exit,
            _ => {}
        }
    });
    window.set_visible(false);
    drop(webview);
    drop(context);
    assert!(verified, "生产前端未完成原生 WebView2 交互检查");
}
