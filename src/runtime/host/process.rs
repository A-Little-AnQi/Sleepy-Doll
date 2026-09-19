//! MCP 服务器与领域适配器共用的子进程隔离。
use crate::error::Result;
use tokio::process::{Child, Command};

/// 插件子进程保留的环境变量，其余全部丢弃。本进程环境里有模型密钥与桥 token。
const ALLOWED_ENVIRONMENT: [&str; 5] = ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"];

/// 清空继承的环境变量，只恢复子进程运行所需的部分。
pub fn isolate_environment(command: &mut Command) {
    command.env_clear();
    for key in ALLOWED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

/// 防止后台子进程打开控制台窗口。
pub fn hide_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

/// 持有子进程运行期间的约束。
pub struct ProcessConstraint {
    #[cfg(target_os = "windows")]
    _job: std::os::windows::io::OwnedHandle,
}

/// 把子进程放进 Job 对象：限制内存占用，本进程退出时连同它派生的进程一起终止。
#[cfg(target_os = "windows")]
pub fn constrain_process(child: &Child) -> Result<ProcessConstraint> {
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    let process = child
        .raw_handle()
        .ok_or_else(|| crate::error::Error::Tool("子进程句柄不可用".into()))?;
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let owned = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
    limits.ProcessMemoryLimit = 512 * 1024 * 1024;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 || unsafe { AssignProcessToJobObject(job, process as _) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(ProcessConstraint { _job: owned })
}

#[cfg(not(target_os = "windows"))]
pub fn constrain_process(_child: &Child) -> Result<ProcessConstraint> {
    Ok(ProcessConstraint {})
}
