//! 自绘标题栏所需的窗口改造：拿掉系统标题栏与边框，接住命中测试，并按系统能力加上圆角。
//!
//! 属于桌面壳（`sleepy-doll.exe`）而不属于库：只有它需要知道窗口的存在。
//!
//! 圆角完全交给 DWM：Windows 11 会切出平滑圆角；Windows 10 没有这个属性，
//! 窗口保持方角，与 Chrome、Edge 在该系统上的形态一致。页面绘制与窗口区域
//! （`SetWindowRgn`）都不参与圆角 —— 前者会在角上留透明缝，后者是硬裁剪，
//! 必有锯齿，两者都试过并放弃了。

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

#[cfg(not(target_os = "windows"))]
pub fn set_drag_strip(_height: f64, _controls_width: f64, _can_maximize: bool) {}

#[cfg(not(target_os = "windows"))]
pub fn begin_system_drag(_hwnd: isize) {}

/// 窗口句柄的数值形态，跨平台外壳都能拿到一个可传递的值。
#[cfg(target_os = "windows")]
pub fn hwnd_id(window: &tao::window::Window) -> isize {
    use tao::platform::windows::WindowExtWindows;
    window.hwnd()
}

#[cfg(not(target_os = "windows"))]
pub fn hwnd_id(_window: &tao::window::Window) -> isize {
    0
}

#[cfg(target_os = "windows")]
pub use platform::{begin_system_drag, install, perform, set_drag_strip};

#[cfg(target_os = "windows")]
mod platform {
    use std::{
        ptr,
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
    };

    use tao::{platform::windows::WindowExtWindows, window::Window};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute},
        UI::{
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::ReleaseCapture,
            WindowsAndMessaging::{
                GW_CHILD, GW_HWNDNEXT, GWL_STYLE, GetParent, GetSystemMetrics, GetWindow,
                GetWindowLongPtrW, GetWindowRect, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
                HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HTTRANSPARENT, IsZoomed,
                NCCALCSIZE_PARAMS, SM_CXFRAME, SM_CXPADDEDBORDER, SWP_FRAMECHANGED, SWP_NOACTIVATE,
                SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SendMessageW, SetWindowLongPtrW,
                SetWindowPos, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCLBUTTONDOWN,
                WS_THICKFRAME,
            },
        },
    };

    use super::Action;

    /// 自绘标题栏的可拖动条带，由界面在挂载后经 `set_drag_strip` 上报，单位是 CSS 像素。
    /// 高度为 0 表示还没有上报（页面刚启动），此时拖动退回界面发起的 `Action::Drag`。
    static STRIP_HEIGHT: AtomicU32 = AtomicU32::new(0);
    /// 标题栏右侧控制按钮组的宽度：条带要把它让出来，按钮才能收到点击。
    static CONTROLS_WIDTH: AtomicU32 = AtomicU32::new(0);
    /// 上报条带时声明的最大化能力。不能最大化的窗口（安装器）要在双击时吞掉
    /// HTCAPTION 的默认最大化行为。
    static CAN_MAXIMIZE: AtomicBool = AtomicBool::new(true);

    /// 子类化标识。两者只在本模块内使用，取值只需彼此不同。
    const PARENT_SUBCLASS: usize = 1;
    const CHILD_SUBCLASS: usize = 2;

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

    /// 请求 DWM 画圆角。Windows 11 接受并切出平滑圆角；Windows 10 不认识这个
    /// 属性，返回错误后窗口保持方角。无论结果如何都不需要界面配合。
    pub fn apply_rounding(hwnd: HWND) {
        let preference = DWMWCP_ROUND;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                ptr::from_ref(&preference).cast(),
                size_of::<i32>() as u32,
            );
        }
    }

    /// 界面挂载标题栏后上报可拖动条带的几何信息（CSS 像素）。上报之后条带区域
    /// 由原生命中测试答 HTCAPTION，拖动、贴边吸附、双击最大化全部走系统路径。
    pub fn set_drag_strip(height: f64, controls_width: f64, can_maximize: bool) {
        if height >= 0.0 && controls_width >= 0.0 {
            STRIP_HEIGHT.store(height as u32, Ordering::Relaxed);
            CONTROLS_WIDTH.store(controls_width as u32, Ordering::Relaxed);
            CAN_MAXIMIZE.store(can_maximize, Ordering::Relaxed);
        }
    }

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
            Action::Drag => begin_system_drag(window.hwnd()),
            Action::Minimize => window.set_minimized(true),
            Action::ToggleMaximize => window.set_maximized(!window.is_maximized()),
            Action::Close => {}
        }
    }

    /// 立即进入系统的移动循环。界面在收到 pointerdown 时调用它作为兜底：
    /// 条带上报之前页面还没被原生命中测试接管。
    pub fn begin_system_drag(hwnd: isize) {
        unsafe {
            // 拖动交给系统的移动循环：贴边吸附、拖离最大化、跨显示器换 DPI 都由它处理。
            ReleaseCapture();
            SendMessageW(hwnd as HWND, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
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
                // 内部区域不返回 HTCAPTION 之外的值：条带是界面自绘的标题栏，按钮自己处理点击。
                let hit = unsafe { resize_hit(hwnd, lparam) };
                if hit != HTCLIENT as LRESULT {
                    return hit;
                }
                if unsafe { in_drag_strip(hwnd, lparam) } {
                    return HTCAPTION as LRESULT;
                }
                HTCLIENT as LRESULT
            }
            WM_NCLBUTTONDBLCLK if wparam as i32 == HTCAPTION as i32 => {
                // 不能最大化的窗口把双击吞掉，其余交给系统：HTCAPTION 的双击
                // 最大化/还原是 DefWindowProc 的行为。
                if CAN_MAXIMIZE.load(Ordering::Relaxed) {
                    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
                } else {
                    0
                }
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
        // 地盘，缩放会整个失效。边缘与标题栏条带都交还给父窗口，两边用同一套判断，
        // 不会有死区。
        if message == WM_NCHITTEST {
            let parent = unsafe { GetParent(hwnd) };
            if !parent.is_null() {
                let hit = unsafe { resize_hit(parent, lparam) };
                if hit != HTCLIENT as LRESULT || unsafe { in_drag_strip(parent, lparam) } {
                    return HTTRANSPARENT as LRESULT;
                }
            }
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    /// 命中点是否落在自绘标题栏的条带里（控制按钮组除外）。条带未上报时恒为否。
    unsafe fn in_drag_strip(hwnd: HWND, lparam: LPARAM) -> bool {
        let strip = STRIP_HEIGHT.load(Ordering::Relaxed);
        if strip == 0 {
            return false;
        }
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
            return false;
        }
        // 屏幕坐标可以是负数，所以先按无符号取位，再按有符号解释。
        let x = (lparam & 0xFFFF) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
        let scale = unsafe { GetDpiForWindow(hwnd) } as f64 / 96.0;
        let strip_height = strip as f64 * scale;
        let y = (y - rect.top) as f64;
        if (0.0..strip_height).contains(&y) {
            let controls = CONTROLS_WIDTH.load(Ordering::Relaxed) as f64 * scale;
            let limit = (rect.right - rect.left) as f64 - controls;
            (x as f64) < limit
        } else {
            false
        }
    }

    /// 边框窄带的命中测试。父窗口按它给出命中码，子窗口按它决定是否让位。
    unsafe fn resize_hit(hwnd: HWND, lparam: LPARAM) -> LRESULT {
        if unsafe { IsZoomed(hwnd) } != 0 {
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
