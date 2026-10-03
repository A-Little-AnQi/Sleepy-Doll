use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use serde_json::Value;
use tao::{
    dpi::LogicalSize,
    event_loop::{EventLoopProxy, EventLoopWindowTarget},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::{Window, WindowBuilder},
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use wry::{WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

use crate::{UserEvent, tray_request::TrayRequest};

const WIDTH: f64 = 288.0;
const HEIGHT: f64 = 420.0;
/// 屏外停靠坐标：Windows 惯例值，任何显示器都够不到。
const OFFSCREEN: i32 = -32000;

/// 低级鼠标钩子的共享状态。回调在主线程执行，只读原子量。
static POPUP_HWND: AtomicIsize = AtomicIsize::new(0);
static POPUP_VISIBLE: AtomicBool = AtomicBool::new(false);
static EVENT_PROXY: OnceLock<EventLoopProxy<UserEvent>> = OnceLock::new();
static MOUSE_HOOK: AtomicIsize = AtomicIsize::new(0);

/// 宿主窗口的擦除底色（COLORREF，即菜单的 --surface）。WebView2 首帧之前
/// DWM 合成的是宿主窗口这一层，用白底就会在首次打开时闪白。
static ERASE_COLOR: AtomicU32 = AtomicU32::new(0x001b_1b1b);

type SubclassProc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT;

#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        hwnd: HWND,
        procedure: Option<SubclassProc>,
        id: usize,
        data: usize,
    ) -> i32;
    fn DefSubclassProc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
}

const ERASE_SUBCLASS: usize = 3;

/// Windows 11 由 DWM 切圆角；旧系统忽略，保持原生直角。
fn apply_rounding(hwnd: isize) {
    unsafe {
        let corner: u32 = 2; // DWMWCP_ROUND
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd as _,
            33, // DWMWA_WINDOW_CORNER_PREFERENCE
            &corner as *const _ as _,
            std::mem::size_of_val(&corner) as _,
        );
    }
}

/// 只接管擦除底色，其余消息照常走默认链。
unsafe extern "system" fn erase_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    const WM_ERASEBKGND: u32 = 0x0014;
    if message == WM_ERASEBKGND {
        unsafe {
            use windows_sys::Win32::Foundation::RECT;
            use windows_sys::Win32::Graphics::Gdi::{
                CreateSolidBrush, DeleteObject, FillRect, HDC,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
            let brush = CreateSolidBrush(ERASE_COLOR.load(Ordering::Relaxed));
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetClientRect(hwnd, &mut rect);
            FillRect(wparam as HDC, &rect, brush);
            DeleteObject(brush);
        }
        return 1;
    }
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

/// 点击落在菜单窗口外就关闭。焦点事件在这个场景不可靠（托盘唤起的窗口
/// 可能拿不到前台），成熟托盘工具用低级鼠标钩子做外部点击检测。
extern "system" fn outside_click_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsChild, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_RBUTTONDOWN, WindowFromPoint,
    };
    let lparam = lparam as usize;
    if code >= 0
        && POPUP_VISIBLE.load(Ordering::Relaxed)
        && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN)
        && let Some(proxy) = EVENT_PROXY.get()
    {
        let hwnd = POPUP_HWND.load(Ordering::Relaxed);
        if hwnd != 0 {
            let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let hit = unsafe { WindowFromPoint(info.pt) };
            let inside = hit == hwnd as _ || unsafe { IsChild(hwnd as _, hit) } != 0;
            if !inside {
                let _ = proxy.send_event(UserEvent::TrayAction(
                    serde_json::json!({"action":"dismiss"}),
                ));
            }
        }
    }
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::CallNextHookEx(
            std::ptr::null_mut(),
            code,
            wparam,
            lparam as isize,
        )
    }
}

/// 装载外部点击检测。主线程调用一次；菜单 show/hide 只翻状态位。
pub fn install_outside_click_hook(proxy: EventLoopProxy<UserEvent>) {
    if MOUSE_HOOK.load(Ordering::Relaxed) != 0 {
        return;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{SetWindowsHookExW, WH_MOUSE_LL};
    let _ = EVENT_PROXY.set(proxy);
    let hook = unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(outside_click_proc),
            std::ptr::null_mut(),
            0,
        )
    };
    MOUSE_HOOK.store(hook as isize, Ordering::Relaxed);
}

pub fn uninstall_outside_click_hook() {
    POPUP_VISIBLE.store(false, Ordering::Relaxed);
    let hook = MOUSE_HOOK.swap(0, Ordering::Relaxed);
    if hook != 0 {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::UnhookWindowsHookEx(hook as _);
        }
    }
}

pub struct TrayPopup {
    webview: WebView,
    pub window: Window,
    _context: WebContext,
    request: TrayRequest,
    anchor: (f64, f64),
    height: f64,
    state: Value,
}

impl Drop for TrayPopup {
    fn drop(&mut self) {
        self.hide();
        self.window.set_visible(false);
        POPUP_HWND.store(0, Ordering::Relaxed);
    }
}

impl TrayPopup {
    pub fn new(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        directory: &std::path::Path,
        state: Value,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(target, proxy, directory.join("tray-webview"), state)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn new_for_validation(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        cache: std::path::PathBuf,
        state: Value,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::build(target, proxy, cache, state)
    }

    fn build(
        target: &EventLoopWindowTarget<UserEvent>,
        proxy: EventLoopProxy<UserEvent>,
        cache: std::path::PathBuf,
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
        // 擦除底色与菜单主题一致：WebView2 首帧之前 DWM 合成的是宿主这一层。
        ERASE_COLOR.store(
            if state["dark"] == true {
                0x001b_1b1b
            } else {
                0x00ff_ffff
            },
            Ordering::Relaxed,
        );
        unsafe {
            let border: u32 = 0xffff_fffe; // DWMWA_COLOR_NONE: HTML paints the one complete client border.
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                window.hwnd() as _,
                34,
                &border as *const _ as _,
                std::mem::size_of_val(&border) as _,
            );
        }
        // 圆角要在首次显示之前声明：窗口已在合成树里时改这个属性不生效。
        apply_rounding(window.hwnd());
        // 初始化无装饰窗口的非客户区，再创建 WebView，避免首帧残留标题栏尺寸。
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SetWindowPos,
            };
            SetWindowSubclass(window.hwnd() as _, Some(erase_proc), ERASE_SUBCLASS, 0);
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
        // WebView2 只渲染宿主可见的窗口：先把窗口挪到屏外，再以不激活方式
        // 置为可见。渲染管线常开，打开菜单时内容已在帧缓冲里。
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
                ShowWindow,
            };
            SetWindowPos(
                window.hwnd() as _,
                std::ptr::null_mut(),
                OFFSCREEN,
                OFFSCREEN,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            );
            ShowWindow(window.hwnd() as _, SW_SHOWNOACTIVATE);
        }
        let mut context = WebContext::new(Some(cache));
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
            .with_default_context_menus(false)
            .with_devtools(cfg!(debug_assertions))
            .with_navigation_handler(|url| {
                matches!(
                    url.as_str(),
                    "sleepy-tray://localhost/" | "http://sleepy-tray.localhost/"
                )
            })
            .with_ipc_handler(move |request| {
                if let Ok(payload) = serde_json::from_str::<Value>(request.body())
                    && payload["action"].as_str().is_some()
                {
                    let _ = proxy.send_event(UserEvent::TrayAction(payload));
                }
            })
            .build(&window)?;
        // Windows 11 由 DWM 切圆角；旧系统保留原生直角。移上屏前再补一次，
        // 屏外停靠期间改属性不被 DWM 重算。
        apply_rounding(window.hwnd());
        Ok(Self {
            webview,
            window,
            _context: context,
            request: TrayRequest::default(),
            anchor: (0.0, 0.0),
            height: HEIGHT,
            state,
        })
    }

    pub fn show(&mut self, x: f64, y: f64, state: Value) {
        if self.request.presented {
            self.hide();
        }
        self.anchor = (x, y);
        self.request.show();
        self.update(state);
        POPUP_HWND.store(self.window.hwnd(), Ordering::Relaxed);
        POPUP_VISIBLE.store(true, Ordering::Relaxed);
        if self.request.ready {
            self.prepare();
        }
    }

    fn position(&self) {
        let (x, y) = self.anchor;
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
            (self.height * scale).ceil() as i32 + outer.height.saturating_sub(client.height) as i32;
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
        // 尺寸没变就不带尺寸参数：SetWindowPos 改尺寸会让 WebView2 重新布局，闪一帧底色。
        apply_rounding(self.window.hwnd());
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
        };
        let flags = if width == outer.width as i32 && height == outer.height as i32 {
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE
        } else {
            SWP_NOACTIVATE | SWP_NOZORDER
        };
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos;
            SetWindowPos(
                self.window.hwnd() as _,
                std::ptr::null_mut(),
                left,
                top,
                width,
                height,
                flags,
            );
        }
    }

    fn prepare(&self) {
        let _ = self.webview.evaluate_script(&format!(
            "window.showMenu?.({}, {});",
            self.state, self.request.generation
        ));
    }

    pub fn ready(&mut self, height: f64) {
        self.resize(height);
        self.request.ready = true;
        if self.request.requested {
            self.prepare();
        }
    }

    fn resize(&mut self, height: f64) {
        if height.is_finite() && (160.0..=800.0).contains(&height) {
            self.height = height;
            self.window.set_inner_size(LogicalSize::new(WIDTH, height));
        }
    }

    pub fn present(&mut self, generation: u64, height: f64) {
        if !self.request.accept_frame(generation) {
            return;
        }
        // Height was measured in the same prepared frame; cold content never moves onscreen.
        self.height = if height.is_finite() && (160.0..=800.0).contains(&height) {
            height
        } else {
            self.height
        };
        self.position();
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                HWND_TOPMOST, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
                SetWindowPos, ShowWindow,
            };
            ShowWindow(self.window.hwnd() as _, SW_SHOW);
            // 创建时带的 WS_EX_TOPMOST 只保证进入置顶带；带内排位不会自动提前，
            // 会被其他置顶窗口（全屏游戏、置顶工具）盖住。每次显示都重插到
            // 置顶带最前。
            SetWindowPos(
                self.window.hwnd() as _,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
        self.window.set_focus();
    }

    pub fn lost_external_focus(&self) -> bool {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetFocus;
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsChild};
        let root = self.window.hwnd() as HWND;
        let foreground = unsafe { GetForegroundWindow() };
        let focus = unsafe { GetFocus() };
        // Parent WM_KILLFOCUS also fires when clicking the embedded WebView child.
        // That is focus within this menu, not a request to dismiss it.
        self.request.presented
            && !foreground.is_null()
            && foreground != root
            && focus != root
            && unsafe { IsChild(root, foreground) } == 0
            && unsafe { IsChild(root, focus) } == 0
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub fn validation_script(&self, script: &str) {
        self.webview.evaluate_script(script).unwrap();
    }

    pub fn hide(&mut self) {
        self.request.hide();
        POPUP_VISIBLE.store(false, Ordering::Relaxed);
        // 移回屏外而不是隐藏窗口，保持 WebView2 的渲染管线活着。
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
            };
            SetWindowPos(
                self.window.hwnd() as _,
                std::ptr::null_mut(),
                OFFSCREEN,
                OFFSCREEN,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOSIZE,
            );
        }
    }

    pub fn update(&mut self, state: Value) {
        self.state = state;
        unsafe {
            let border: u32 = 0xffff_fffe;
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                self.window.hwnd() as _,
                34,
                &border as *const _ as _,
                std::mem::size_of_val(&border) as _,
            );
        }
        ERASE_COLOR.store(
            if self.state["dark"] == true {
                0x001b_1b1b
            } else {
                0x00ff_ffff
            },
            Ordering::Relaxed,
        );
        let _ = self
            .webview
            .set_background_color(if self.state["dark"] == true {
                (27, 27, 27, 255)
            } else {
                (255, 255, 255, 255)
            });
        if self.request.ready {
            let _ = self
                .webview
                .evaluate_script(&format!("window.updateMenu?.({});", self.state));
        }
    }
}
