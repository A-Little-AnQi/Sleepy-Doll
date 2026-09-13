//! Child-process isolation shared by MCP servers and domain adapters.
//!
//! Both spawn binaries supplied by plugins, so both need the same treatment:
//! a cleared environment, no console window, and a job object that keeps the
//! child from outliving the application.
use crate::error::Result;
use tokio::process::{Child, Command};

/// The environment a plugin child process is allowed to keep. Everything else is
/// dropped, so a third-party binary cannot read the model API key or the bridge
/// token that `${ENV:...}` configuration keeps in this process's environment.
const ALLOWED_ENVIRONMENT: [&str; 5] = ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"];

/// Clears the inherited environment and restores only what a child needs to run.
pub fn isolate_environment(command: &mut Command) {
    command.env_clear();
    for key in ALLOWED_ENVIRONMENT {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

/// Keeps a background child from opening a console window.
pub fn hide_console(command: &mut Command) {
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000);
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

/// Holds whatever keeps the child confined for as long as it runs.
pub struct ProcessConstraint {
    #[cfg(target_os = "windows")]
    _job: std::os::windows::io::OwnedHandle,
}

/// Confines the child to a job object that caps its memory and terminates it
/// when this process exits, including any grandchildren it spawned.
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
        .ok_or_else(|| crate::error::Error::Tool("child process handle unavailable".into()))?;
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
