//! 把原神窗口拉到前台。桥的多数动作走截图 + 模拟输入，游戏在后台时输入
//! 会被系统丢弃或被别的窗口吃掉，所以提交动作前必须保证游戏是前台窗口。
//!
//! 只用 Win32 的公开接口：直接 SetForegroundWindow 可能被系统拒绝（前台锁），
//! 这里按惯例先发一个无害的按键再切，失败重试几次。

use windows_sys::Win32::{
    Foundation::HWND,
    UI::Input::KeyboardAndMouse::{INPUT, INPUT_KEYBOARD, KEYBDINPUT, SendInput, VK_MENU},
    UI::WindowsAndMessaging::{
        GetForegroundWindow, IsIconic, IsWindow, SW_RESTORE, SetForegroundWindow, ShowWindow,
    },
};

/// 窗口是否已是前台。
pub fn is_foreground(handle: isize) -> bool {
    if handle == 0 {
        return false;
    }
    unsafe { GetForegroundWindow() == handle as HWND }
}

/// 把游戏窗口拉到前台并等它真正成为前台。窗口最小化时先还原。
/// 返回是否在时限内成功。
pub fn focus_game_window(handle: isize, wait: std::time::Duration) -> bool {
    if handle == 0 {
        return false;
    }
    let hwnd = handle as HWND;
    let started = std::time::Instant::now();
    loop {
        // windows-sys 的 BOOL 就是 i32，非零即真。
        if unsafe { IsWindow(hwnd) } == 0 {
            return false;
        }
        if unsafe { IsIconic(hwnd) } != 0 {
            unsafe { ShowWindow(hwnd, SW_RESTORE) };
        }
        if is_foreground(handle) {
            return true;
        }
        // 直接切前台常被前台锁拒绝；先往自己的消息队列里塞一次 Alt 按下再抬起，
        // 让系统认为本进程刚收到输入，SetForegroundWindow 就会被放行。
        unsafe {
            let down = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            let up = INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_MENU,
                        wScan: 0,
                        dwFlags: 2, // KEYEVENTF_KEYUP
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            };
            SendInput(2, [down, up].as_ptr(), size_of::<INPUT>() as i32);
            SetForegroundWindow(hwnd);
        }
        if is_foreground(handle) {
            return true;
        }
        if started.elapsed() >= wait {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}
