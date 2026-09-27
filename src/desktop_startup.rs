//! Keep the desktop surface offscreen until its first styled frontend frame.
use tao::{
    dpi::PhysicalPosition,
    window::{Theme, Window},
};

pub fn background(dark: bool) -> (u8, u8, u8, u8) {
    if dark {
        (27, 27, 27, 255)
    } else {
        (255, 255, 255, 255)
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };
    use tao::{
        event::{Event, StartCause},
        event_loop::{ControlFlow, EventLoopBuilder},
        platform::{
            run_return::EventLoopExtRunReturn,
            windows::{EventLoopBuilderExtWindows, WindowBuilderExtWindows, WindowExtWindows},
        },
        window::WindowBuilder,
    };
    use windows_sys::Win32::{
        Foundation::RECT,
        UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible},
    };
    use wry::{WebContext, WebViewBuilder};

    #[test]
    #[ignore = "需要隔离的 WebView2 用户数据目录"]
    fn cold_webview_stays_offscreen_until_first_styled_frame() {
        let directory =
            PathBuf::from(std::env::var_os("SLEEPY_DOLL_WINDOW_VALIDATION").expect("缺少验证目录"));
        let mut events = EventLoopBuilder::<String>::with_user_event()
            .with_any_thread(true)
            .build();
        let proxy = events.create_proxy();
        let window = WindowBuilder::new()
            .with_visible(false)
            .with_skip_taskbar(true)
            .with_position(PhysicalPosition::new(-24000, -24000))
            .with_inner_size(tao::dpi::LogicalSize::new(300., 200.))
            .with_background_color(background(true))
            .build(&events)
            .unwrap();
        let hwnd = window.hwnd();
        assert_eq!(unsafe { IsWindowVisible(hwnd as _) }, 0);
        let mut context = WebContext::new(Some(directory));
        let webview=WebViewBuilder::new_with_web_context(&mut context).with_background_color(background(true)).with_ipc_handler(move|request|{let _=proxy.send_event(request.body().to_owned());}).with_html(r#"<body><script>
            window.ipc.postMessage('cold');
            setTimeout(()=>{document.body.style.background='#1b1b1b';document.body.textContent='Styled frame ready';
                requestAnimationFrame(()=>requestAnimationFrame(()=>window.ipc.postMessage('ready')));},250);
            </script></body>"#).build(&window).unwrap();
        let target = window.outer_position().unwrap();
        let mut startup = DesktopStartup::prime(&window, false);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut saw_cold = false;
        let mut finished = false;
        events.run_return(|event, _, control| {
            *control = ControlFlow::WaitUntil(deadline);
            match event {
                Event::UserEvent(value) if value == "cold" => {
                    let mut rect: RECT = unsafe { std::mem::zeroed() };
                    unsafe { GetWindowRect(hwnd as _, &mut rect) };
                    assert_eq!((rect.left, rect.top), (-32000, -32000));
                    assert!(!startup.is_ready());
                    saw_cold = true;
                }
                Event::UserEvent(value) if value == "ready" => {
                    assert!(startup.present(&window, true, false));
                    assert_eq!(window.outer_position().unwrap(), target);
                    assert!(
                        !startup.present(&window, false, false),
                        "late ready duplicated presentation"
                    );
                    finished = true;
                    window.set_visible(false);
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
        drop(webview);
        drop(context);
        assert!(
            saw_cold && finished,
            "cold/first-frame handshake did not complete"
        );
    }
}

pub struct DesktopStartup {
    ready: bool,
    position: PhysicalPosition<i32>,
    maximized: bool,
}

impl DesktopStartup {
    pub fn prime(window: &Window, maximized: bool) -> Self {
        let position = window.outer_position().unwrap_or_default();
        // A visible offscreen host lets WebView2 composite animation frames without exposing a cold page.
        #[cfg(target_os = "windows")]
        unsafe {
            use tao::platform::windows::WindowExtWindows;
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
                ShowWindow,
            };
            SetWindowPos(
                window.hwnd() as _,
                std::ptr::null_mut(),
                -32000,
                -32000,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            );
            ShowWindow(window.hwnd() as _, SW_SHOWNOACTIVATE);
        }
        #[cfg(not(target_os = "windows"))]
        window.set_visible(true);
        Self {
            ready: false,
            position,
            maximized,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn present(&mut self, window: &Window, dark: bool, focus: bool) -> bool {
        if self.ready {
            return false;
        }
        window.set_background_color(Some(background(dark)));
        window.set_theme(Some(if dark { Theme::Dark } else { Theme::Light }));
        self.ready = true;
        window.set_outer_position(self.position);
        window.set_maximized(self.maximized);
        window.set_visible(true);
        if focus {
            window.set_focus();
        }
        true
    }
}
