//! 自绘标题栏所需的窗口改造：拿掉系统标题栏与边框，接住命中测试，并按系统能力加上圆角。
//!
//! 属于桌面壳（`sleepy-doll.exe`）而不属于库：只有它需要知道窗口的存在。

#[cfg(not(target_os = "windows"))]
use tao::window::Window;

/// 界面可以请求的窗口动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Drag,
    Minimize,
    ToggleMaximize,
    Close,
}

impl Action {
    /// IPC 方法名到动作的映射；`None` 表示这不是窗口控制请求。
    pub fn from_method(method: &str) -> Option<Self> {
        // 自绘标题栏只存在于 Windows 桌面壳，其它平台上界面不会发出这些请求。
        if !cfg!(target_os = "windows") {
            return None;
        }
        match method {
            "window.drag" => Some(Self::Drag),
            "window.minimize" => Some(Self::Minimize),
            "window.toggleMaximize" => Some(Self::ToggleMaximize),
            "window.close" => Some(Self::Close),
            _ => None,
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn install(_window: &Window) {}

#[cfg(not(target_os = "windows"))]
pub fn perform(_window: &Window, _action: Action) {}

#[cfg(target_os = "windows")]
pub use platform::{install, perform};

#[cfg(target_os = "windows")]
mod platform {
    use std::{
        ptr,
        sync::atomic::{AtomicBool, Ordering},
    };

    use tao::{platform::windows::WindowExtWindows, window::Window};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute},
            Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn},
        },
        UI::{
            Input::KeyboardAndMouse::ReleaseCapture,
            WindowsAndMessaging::{
                GW_CHILD, GW_HWNDNEXT, GWL_STYLE, GetParent, GetSystemMetrics, GetWindow,
                GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
                HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HTTRANSPARENT, IsIconic,
                IsZoomed, NCCALCSIZE_PARAMS, SM_CXFRAME, SM_CXPADDEDBORDER, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW,
                SetWindowLongPtrW, SetWindowPos, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN,
                WM_SIZE, WS_THICKFRAME,
            },
        },
    };

    use super::Action;

    /// 圆角半径。DWM 属性与窗口区域两条路径共用它。
    const CORNER_RADIUS: i32 = 12;

    /// 子类化标识。两者只在本模块内使用，取值只需彼此不同。
    const PARENT_SUBCLASS: usize = 1;
    const CHILD_SUBCLASS: usize = 2;

    /// 圆角是否由 DWM 绘制。为真时不能再设窗口区域，那会把 DWM 的圆角切掉。
    static DWM_ROUNDING: AtomicBool = AtomicBool::new(false);

    // comctl32 的子类化接口，windows-sys 没有导出。wry 也用这一套在同一个父窗口上
    // 挂子类（WebView2 靠它跟随窗口尺寸），两边走同一条链才能并存。
    type SubclassProc =
        unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT;

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

    /// GDI 区域句柄。
    /// 让窗口无边框、可缩放、可最大化，并按系统能力加上圆角。在窗口建好之后调用。
    pub fn install(window: &Window) {
        let hwnd = window.hwnd() as HWND;
        unsafe {
            // 子类先装上：下面加回 WS_THICKFRAME 会让系统重算非客户区，那一次
            // WM_NCCALCSIZE 必须由我们接手，否则客户区会按边框宽度缩一圈。
            SetWindowSubclass(hwnd, Some(parent_proc), PARENT_SUBCLASS, 0);

            // 无装饰的窗口没有 WS_THICKFRAME，而缩放循环、贴边吸附、拖离最大化都挂在它上面：
            // 就算命中测试给出 HTLEFT，DefWindowProc 缺了它也不会进入缩放循环。
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            SetWindowLongPtrW(hwnd, GWL_STYLE, (style | WS_THICKFRAME) as isize);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );

            // WebView2 的子窗口这时已经建好并铺满客户区，边缘的命中测试要由它让出来。
            for child in children(hwnd) {
                SetWindowSubclass(child, Some(child_proc), CHILD_SUBCLASS, 0);
            }

            apply_rounding(hwnd);
        }
    }

    /// 执行窗口动作。`Close` 不在这里处理：隐藏到托盘还是退出进程由外壳决定。
    pub fn perform(window: &Window, action: Action) {
        match action {
            Action::Drag => unsafe {
                // 拖动交给系统的移动循环：贴边吸附、拖离最大化、跨显示器换 DPI 都由它处理。
                ReleaseCapture();
                SendMessageW(
                    window.hwnd() as HWND,
                    WM_NCLBUTTONDOWN,
                    HTCAPTION as WPARAM,
                    0,
                );
            },
            Action::Minimize => window.set_minimized(true),
            Action::ToggleMaximize => window.set_maximized(!window.is_maximized()),
            Action::Close => {}
        }
    }

    unsafe extern "system" fn parent_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match message {
            WM_NCCALCSIZE if wparam != 0 => {
                // 系统算出的客户区原样退回：非客户区被压成零宽，系统边框与标题栏就不存在了。
                let params = lparam as *mut NCCALCSIZE_PARAMS;
                let proposed = unsafe { (*params).rgrc[0] };
                unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                unsafe { (*params).rgrc[0] = proposed };
                if unsafe { IsZoomed(hwnd) } != 0 {
                    // 最大化时窗口被外扩了恰好一条边框，客户区要缩回去，否则界面会被推出屏幕。
                    let frame = unsafe { frame_thickness() };
                    let rect = unsafe { &mut (*params).rgrc[0] };
                    rect.left += frame;
                    rect.top += frame;
                    rect.right -= frame;
                    rect.bottom -= frame;
                }
                0
            }
            WM_NCHITTEST => {
                // 内部区域不返回 HTCAPTION：标题栏由界面自绘，交给系统会截走那里的鼠标消息。
                // 拖动由界面显式发起（见 perform 的 Drag）。
                unsafe { resize_hit(hwnd, lparam) }
            }
            WM_SIZE => {
                // 最大化、最小化、还原都走 WM_SIZE；WM_WINDOWPOSCHANGED 会被设置区域自己触发。
                let result = unsafe { DefSubclassProc(hwnd, message, wparam, lparam) };
                unsafe { refresh_region(hwnd) };
                result
            }
            _ => unsafe { DefSubclassProc(hwnd, message, wparam, lparam) },
        }
    }

    unsafe extern "system" fn child_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        // WebView2 的子窗口铺满整个客户区，它答 HTCLIENT 就等于把边框那一条也算进自己的
        // 地盘，缩放会整个失效。边缘交还给父窗口，两边用同一个 resize_hit 判断，不会有死区。
        if message == WM_NCHITTEST {
            let parent = unsafe { GetParent(hwnd) };
            if !parent.is_null() && unsafe { resize_hit(parent, lparam) } != HTCLIENT as LRESULT {
                return HTTRANSPARENT as LRESULT;
            }
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// 边框窄带的命中测试。父窗口按它给出命中码，子窗口按它决定是否让位。
    unsafe fn resize_hit(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        if unsafe { IsZoomed(hwnd) } != 0 || unsafe { IsIconic(hwnd) } != 0 {
            return HTCLIENT as LRESULT;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return HTCLIENT as LRESULT;
        }
        // 屏幕坐标可以是负数，所以先按无符号取位，再按有符号解释。
        let x = (lparam & 0xFFFF) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
        let frame = unsafe { frame_thickness() };
        let code = match (
            x < rect.left + frame,
            x >= rect.right - frame,
            y < rect.top + frame,
            y >= rect.bottom - frame,
        ) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, _, _, _) => HTLEFT,
            (_, true, _, _) => HTRIGHT,
            (_, _, true, _) => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => HTCLIENT,
        };
        code as LRESULT
    }

    /// 系统认定的边框宽度。两条边都按这个值算，抓取区域不会错位。
    unsafe fn frame_thickness() -> i32 {
        unsafe { GetSystemMetrics(SM_CXFRAME) + GetSystemMetrics(SM_CXPADDEDBORDER) }
    }

    /// 优先让 DWM 画圆角；属性不被支持（Win10 返回 E_INVALIDARG）时才退回窗口区域。
    unsafe fn apply_rounding(hwnd: HWND) {
        let preference = DWMWCP_ROUND;
        let result = unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                ptr::from_ref(&preference).cast(),
                size_of::<i32>() as u32,
            )
        };
        if result >= 0 {
            DWM_ROUNDING.store(true, Ordering::Relaxed);
        } else {
            unsafe { refresh_region(hwnd) };
        }
    }

    /// 按窗口当前状态设置圆角区域。最大化与最小化时窗口铺满屏幕，区域必须先撤掉。
    unsafe fn refresh_region(hwnd: HWND) {
        if DWM_ROUNDING.load(Ordering::Relaxed) {
            return;
        }
        if unsafe { IsZoomed(hwnd) } != 0 || unsafe { IsIconic(hwnd) } != 0 {
            unsafe { SetWindowRgn(hwnd, ptr::null_mut(), 1) };
            return;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return;
        }
        // 圆角区域的椭圆参数是直径，区域右边界与下边界又是开区间。
        let region = unsafe {
            CreateRoundRectRgn(
                0,
                0,
                rect.right - rect.left + 1,
                rect.bottom - rect.top + 1,
                CORNER_RADIUS * 2,
                CORNER_RADIUS * 2,
            )
        };
        if region.is_null() {
            return;
        }
        // 成功时区域归系统所有，失败时还归我们，要自己释放。
        if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
            unsafe { DeleteObject(region) };
        }
    }

    unsafe fn children(parent: HWND) -> Vec<HWND> {
        let mut found = Vec::new();
        let mut child = unsafe { GetWindow(parent, GW_CHILD) };
        while !child.is_null() {
            found.push(child);
            child = unsafe { GetWindow(child, GW_HWNDNEXT) };
        }
        found
    }
}
