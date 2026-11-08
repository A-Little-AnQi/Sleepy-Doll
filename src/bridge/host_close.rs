//! 从已连接桥锁定宿主进程；不接受进程名、PID 或路径参数。
use super::BgiClient;
use crate::{Error, Result};
use serde_json::Value;
#[cfg(windows)]
use serde_json::json;

#[cfg(windows)]
pub fn close(client: &BgiClient) -> Result<Value> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM},
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        },
        UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE},
    };
    let first = client.info()?;
    let pid = first["processId"]
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|pid| *pid > 0 && *pid != std::process::id())
        .ok_or_else(|| Error::Conflict("无法锁定当前连接的 BetterGI 进程".into()))?;
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | 0x00100000,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    struct Process(HANDLE);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    let process = Process(handle);
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let path = std::path::PathBuf::from(String::from_utf16_lossy(&path[..length as usize]));
    if !path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("BetterGI.exe"))
    {
        return Err(Error::Conflict("当前连接不是可关闭的 BetterGI 宿主".into()));
    }
    super::origin::require_path(&path)?;
    let second = client.info()?;
    if first["instanceId"] != second["instanceId"] || first["processId"] != second["processId"] {
        return Err(Error::Conflict("BetterGI 连接已变化，未关闭软件".into()));
    }
    unsafe extern "system" fn request(window: HWND, target: LPARAM) -> i32 {
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut pid);
        }
        if pid == target as u32 {
            unsafe {
                PostMessageW(window, WM_CLOSE, 0, 0);
            }
        }
        1
    }
    unsafe {
        EnumWindows(Some(request), pid as LPARAM);
    }
    let result = unsafe { WaitForSingleObject(process.0, 5000) };
    let forced = result == 0x102;
    if forced {
        if unsafe { TerminateProcess(process.0, 0) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if unsafe { WaitForSingleObject(process.0, 5000) } != 0 {
            return Err(Error::Conflict("关闭工具软件后仍未确认进程退出".into()));
        }
    } else if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(
        json!({"closed":true,"forced":forced,"verification":{"status":"succeeded","reason":"已确认当前连接的 BetterGI 进程退出"}}),
    )
}

#[cfg(not(windows))]
pub fn close(_: &BgiClient) -> Result<Value> {
    Err(Error::Config("关闭工具软件仅支持 Windows".into()))
}
