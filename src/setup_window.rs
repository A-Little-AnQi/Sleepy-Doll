//! 固定尺寸安装窗口的原生标题栏命中；不依赖页面、遮罩或输入捕获状态。
#[cfg(windows)]
mod platform {
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::{
                GetWindowRect, HTCAPTION, WM_NCDESTROY, WM_NCHITTEST, WM_NCLBUTTONDBLCLK,
            },
        },
    };
    type Proc = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT;
    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(hwnd: HWND, proc: Option<Proc>, id: usize, data: usize) -> i32;
        fn RemoveWindowSubclass(hwnd: HWND, proc: Option<Proc>, id: usize) -> i32;
        fn DefSubclassProc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    }
    const ID: usize = 0x534449;
    pub fn install(handle: isize) -> bool {
        unsafe { SetWindowSubclass(handle as HWND, Some(window_proc), ID, 0) != 0 }
    }
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        w: WPARAM,
        l: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        if message == WM_NCHITTEST {
            let mut rect: RECT = unsafe { std::mem::zeroed() };
            if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
                let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64 / 96.0;
                let x = (l as u16 as i16) as i32 - rect.left;
                let y = ((l >> 16) as u16 as i16) as i32 - rect.top;
                if x >= 0
                    && y >= 0
                    && y < (32.0 * scale).round() as i32
                    && x < rect.right - rect.left - (92.0 * scale).round() as i32
                {
                    return HTCAPTION as LRESULT;
                }
            }
        }
        if message == WM_NCLBUTTONDBLCLK && w == HTCAPTION as usize {
            return 0;
        }
        if message == WM_NCDESTROY {
            unsafe {
                RemoveWindowSubclass(hwnd, Some(window_proc), ID);
            }
        }
        unsafe { DefSubclassProc(hwnd, message, w, l) }
    }
}
#[cfg(windows)]
pub use platform::install;
#[cfg(not(windows))]
pub fn install(_: isize) -> bool {
    true
}
