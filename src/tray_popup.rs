use serde_json::Value;
use tao::{
    dpi::LogicalSize,
    event_loop::{EventLoopProxy, EventLoopWindowTarget},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use wry::{WebContext, WebView, WebViewBuilder};

use crate::UserEvent;

const WIDTH: f64 = 288.0;
const HEIGHT: f64 = 359.0;

pub struct TrayPopup {
    webview: WebView,
    pub window: Window,
    _context: WebContext,
    ready: bool,
    requested: bool,
    state: Value,
}

impl TrayPopup {
    pub fn new(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        directory: &std::path::Path,
        state: Value,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if <crate::UiAssets as rust_embed::RustEmbed>::get("tray.html").is_none() {
            return Err("托盘界面资源缺失".into());
        }
        let window = WindowBuilder::new()
            .with_title("Sleepy Doll · 托盘")
            .with_inner_size(LogicalSize::new(WIDTH, HEIGHT))
            .with_visible(false)
            .with_decorations(false)
            .with_resizable(false)
            .with_always_on_top(true)
            .with_skip_taskbar(true)
            .build(target)?;
        // 初始化无装饰窗口的非客户区，再创建 WebView，避免首帧残留标题栏尺寸。
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SetWindowPos,
            };
            SetWindowPos(
                window.hwnd() as _,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }
        let mut context = WebContext::new(Some(directory.join("tray-webview")));
        let webview = WebViewBuilder::new_with_web_context(&mut context)
            .with_background_color(if state["dark"] == true {
                (27, 27, 27, 255)
            } else {
                (255, 255, 255, 255)
            })
            .with_custom_protocol("sleepy-tray".into(), |_, request| {
                use rust_embed::RustEmbed;
                let path = request.uri().path().trim_start_matches('/');
                let path = if path.is_empty() { "tray.html" } else { path };
                match <crate::UiAssets as RustEmbed>::get(path) {
                    Some(asset) => wry::http::Response::builder()
                        .header(
                            "content-type",
                            mime_guess::from_path(path).first_or_octet_stream().as_ref(),
                        )
                        .body(asset.data)
                        .expect("托盘页面响应"),
                    None => wry::http::Response::builder()
                        .status(404)
                        .body(std::borrow::Cow::Borrowed(&[][..]))
                        .expect("托盘资源响应"),
                }
            })
            .with_url("sleepy-tray://localhost/")
            .with_initialization_script(format!("window.__TRAY_STATE__={state};"))
            .with_devtools(cfg!(debug_assertions))
            .with_navigation_handler(|url| {
                matches!(
                    url.as_str(),
                    "sleepy-tray://localhost/" | "http://sleepy-tray.localhost/"
                )
            })
            .with_ipc_handler(move |request| {
                if let Ok(payload) = serde_json::from_str::<Value>(request.body())
                    && let Some(action) = payload["action"].as_str()
                {
                    let _ = proxy.send_event(UserEvent::TrayAction(action.to_owned()));
                }
            })
            .build(&window)?;
        // Windows 11 由 DWM 切圆角；旧系统保留原生直角。
        unsafe {
            let corner: u32 = 2;
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                window.hwnd() as _,
                33,
                &corner as *const _ as _,
                std::mem::size_of_val(&corner) as _,
            );
        }
        Ok(Self {
            webview,
            window,
            _context: context,
            ready: false,
            requested: false,
            state,
        })
    }

    pub fn show(&mut self, x: f64, y: f64, state: Value) {
        use windows_sys::Win32::{
            Foundation::POINT,
            Graphics::Gdi::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            },
        };
        let point = POINT {
            x: x as i32,
            y: y as i32,
        };
        let mut area: MONITORINFO = unsafe { std::mem::zeroed() };
        area.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let valid = unsafe {
            GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut area)
        } != 0;
        let scale = self
            .window
            .available_monitors()
            .find(|monitor| {
                let p = monitor.position();
                let s = monitor.size();
                x >= p.x as f64
                    && y >= p.y as f64
                    && x < p.x as f64 + s.width as f64
                    && y < p.y as f64 + s.height as f64
            })
            .map(|monitor| monitor.scale_factor())
            .unwrap_or(self.window.scale_factor());
        let client = self.window.inner_size();
        let outer = self.window.outer_size();
        let width = (WIDTH * scale).ceil() as i32 + outer.width.saturating_sub(client.width) as i32;
        let height =
            (HEIGHT * scale).ceil() as i32 + outer.height.saturating_sub(client.height) as i32;
        let gap = (8.0 * scale) as i32;
        let mut left = point.x - width + gap;
        let mut top = point.y - height - gap;
        if valid {
            left = left.clamp(
                area.rcWork.left + gap,
                (area.rcWork.right - width - gap).max(area.rcWork.left + gap),
            );
            if top < area.rcWork.top + gap {
                top = point.y + gap;
            }
            top = top.clamp(
                area.rcWork.top + gap,
                (area.rcWork.bottom - height - gap).max(area.rcWork.top + gap),
            );
        }
        // 无边框浮层按物理外框定位，避免系统补回隐藏标题栏的高度。
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos,
            };
            SetWindowPos(
                self.window.hwnd() as _,
                std::ptr::null_mut(),
                left,
                top,
                width,
                height,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        self.state = state;
        self.requested = true;
        if self.ready {
            self.prepare();
        }
    }

    fn prepare(&self) {
        let _ = self
            .webview
            .evaluate_script(&format!("window.showMenu?.({});", self.state));
    }

    pub fn ready(&mut self) {
        self.ready = true;
        if self.requested {
            self.prepare();
        }
    }

    pub fn present(&self) {
        if self.requested {
            self.window.set_visible(true);
            self.window.set_focus();
            let _ = self.webview.evaluate_script("window.enterMenu?.();");
        }
    }

    pub fn hide(&mut self) {
        self.requested = false;
        self.window.set_visible(false);
    }

    pub fn update(&mut self, state: Value) {
        self.state = state;
        let _ = self
            .webview
            .set_background_color(if self.state["dark"] == true {
                (27, 27, 27, 255)
            } else {
                (255, 255, 255, 255)
            });
        if self.ready {
            let _ = self
                .webview
                .evaluate_script(&format!("window.updateMenu?.({});", self.state));
        }
    }
}
