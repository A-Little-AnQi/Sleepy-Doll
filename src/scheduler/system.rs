use super::{calendar::escape, model::Schedule};
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use time::{OffsetDateTime, PrimitiveDateTime};

pub trait TaskBackend: Send + Sync {
    fn ensure_available(&self) -> Result<()> {
        administrative_identity().map(|_| ())
    }
    fn desktop_available(&self) -> bool {
        desktop_available()
    }
    fn apply(&self, schedule: &Schedule) -> Result<String>;
    fn delete(&self, schedule: &Schedule) -> Result<()>;
    fn verify(&self, schedule: &Schedule) -> Result<()>;
}

pub struct WindowsTasks {
    pub config: PathBuf,
    pub executable: PathBuf,
}

impl WindowsTasks {
    pub fn new(config: &Path) -> Result<Self> {
        Ok(Self {
            config: config.canonicalize()?,
            executable: std::env::current_exe()?.canonicalize()?,
        })
    }

    fn folder(&self, sid: &str) -> Result<String> {
        Ok(format!(
            "\\SleepyDoll\\{}",
            &super::digest(&(sid, self.config.to_string_lossy().to_lowercase()))?[..24]
        ))
    }

    pub fn xml(&self, schedule: &Schedule, sid: &str) -> Result<String> {
        let at = super::model::parse_at(&schedule.created_at)?;
        let triggers = super::calendar::triggers(&schedule.trigger, at)?;
        let arguments = format!(
            "{} --run-schedule {}",
            quote(&self.config.to_string_lossy()),
            schedule.id
        );
        let working = self
            .executable
            .parent()
            .ok_or_else(|| Error::Config("程序目录无效".into()))?;
        let maximum = schedule
            .entries
            .iter()
            .map(|entry| entry.workflow.limits.max_seconds.clamp(1, 86400) + 30)
            .sum::<i64>()
            + 60;
        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-16"?><Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task"><RegistrationInfo><Author>Sleepy Doll</Author><Source>SleepyDoll:{}</Source><Description>{}</Description></RegistrationInfo><Triggers>{}</Triggers><Principals><Principal id="Owner"><UserId>{}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals><Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><StartWhenAvailable>false</StartWhenAvailable><RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable><AllowStartOnDemand>true</AllowStartOnDemand><Enabled>{}</Enabled><Hidden>false</Hidden><WakeToRun>false</WakeToRun><ExecutionTimeLimit>PT{}S</ExecutionTimeLimit></Settings><Actions Context="Owner"><Exec><Command>{}</Command><Arguments>{}</Arguments><WorkingDirectory>{}</WorkingDirectory></Exec></Actions></Task>"#,
            escape(&self.folder(sid)?),
            escape(&schedule.name),
            triggers,
            escape(sid),
            schedule.enabled,
            maximum,
            escape(&self.executable.to_string_lossy()),
            escape(&arguments),
            escape(&working.to_string_lossy())
        ))
    }
}

fn quote(value: &str) -> String {
    // 配置路径来自本机配置解析，不接受模型传入；按 Windows 参数规则转义。
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        output.push(ch);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use windows::{
        Win32::System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            TaskScheduler::{
                ITaskService, TASK_CREATE, TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN,
                TaskScheduler,
            },
            Variant::VARIANT,
        },
        core::BSTR,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, SYSTEMTIME},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_ELEVATION,
            TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
        },
        System::{
            Threading::{GetCurrentProcess, OpenProcessToken},
            Time::{SystemTimeToTzSpecificLocalTimeEx, TzSpecificLocalTimeToSystemTimeEx},
        },
    };

    pub fn identity() -> Result<(String, bool)> {
        unsafe {
            let mut token = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let result = (|| {
                let mut length = 0;
                let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
                if GetTokenInformation(
                    token,
                    TokenElevation,
                    (&mut elevation as *mut TOKEN_ELEVATION).cast(),
                    std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                    &mut length,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut length);
                let mut user =
                    vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
                if GetTokenInformation(
                    token,
                    TokenUser,
                    user.as_mut_ptr().cast(),
                    length,
                    &mut length,
                ) == 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                let user = &*(user.as_ptr().cast::<TOKEN_USER>());
                let mut text = std::ptr::null_mut();
                if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                let mut size = 0;
                while *text.add(size) != 0 {
                    size += 1;
                }
                let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, size));
                LocalFree(text.cast());
                Ok((sid, elevation.TokenIsElevated != 0))
            })();
            CloseHandle(token);
            result
        }
    }

    pub fn with_task(
        backend: &WindowsTasks,
        schedule: &Schedule,
        operation: &str,
    ) -> Result<String> {
        let sid = super::administrative_identity()?;
        let folder = backend.folder(&sid)?;
        let xml = backend.xml(schedule, &sid)?;
        let id = schedule.id.clone();
        let expected = schedule.system_hash.clone();
        let operation = operation.to_owned();
        std::thread::spawn(move || -> Result<String> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(win_error)?;
                struct Apartment;
                impl Drop for Apartment {
                    fn drop(&mut self) {
                        unsafe {
                            CoUninitialize();
                        }
                    }
                }
                let _apartment = Apartment;
                let service: ITaskService =
                    CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                        .map_err(win_error)?;
                let empty = VARIANT::default();
                service
                    .Connect(&empty, &empty, &empty, &empty)
                    .map_err(win_error)?;
                let root = service.GetFolder(&BSTR::from("\\")).map_err(win_error)?;
                let main = match root.GetFolder(&BSTR::from("SleepyDoll")) {
                    Ok(value) => value,
                    Err(error) if missing(&error) && operation == "apply" => root
                        .CreateFolder(&BSTR::from("SleepyDoll"), &empty)
                        .map_err(win_error)?,
                    Err(error) => return Err(win_error(error)),
                };
                let leaf = folder.rsplit('\\').next().expect("固定目录");
                let folder = match main.GetFolder(&BSTR::from(leaf)) {
                    Ok(value) => value,
                    Err(error) if missing(&error) && operation == "apply" => main
                        .CreateFolder(&BSTR::from(leaf), &empty)
                        .map_err(win_error)?,
                    Err(error) => return Err(win_error(error)),
                };
                match folder.GetTask(&BSTR::from(&id)) {
                    Ok(task) => {
                        let current =
                            super::super::digest(&task.Xml().map_err(win_error)?.to_string())?;
                        if expected.as_ref() != Some(&current) {
                            return Err(Error::Conflict(
                                "系统任务被外部修改或归属无法核验，未覆盖".into(),
                            ));
                        }
                    }
                    Err(error) if missing(&error) => {
                        if operation == "delete" {
                            return Ok(String::new());
                        }
                        if expected.is_some() || operation == "verify" {
                            return Err(Error::Conflict("系统任务已被移除".into()));
                        }
                    }
                    Err(error) => return Err(win_error(error)),
                }
                if operation == "delete" {
                    folder.DeleteTask(&BSTR::from(&id), 0).map_err(win_error)?;
                    return Ok(String::new());
                }
                if operation == "verify" {
                    return Ok(expected.unwrap_or_default());
                }
                let flags = if expected.is_some() {
                    TASK_CREATE_OR_UPDATE.0
                } else {
                    TASK_CREATE.0
                };
                let task = folder
                    .RegisterTask(
                        &BSTR::from(&id),
                        &BSTR::from(xml),
                        flags,
                        &empty,
                        &empty,
                        TASK_LOGON_INTERACTIVE_TOKEN,
                        &empty,
                    )
                    .map_err(win_error)?;
                super::super::digest(&task.Xml().map_err(win_error)?.to_string())
            }
        })
        .join()
        .map_err(|_| Error::Tool("Windows 定时任务注册线程异常".into()))?
    }

    fn missing(error: &windows::core::Error) -> bool {
        matches!(error.code().0 as u32, 0x80070002 | 0x80070003)
    }
    fn win_error(error: windows::core::Error) -> Error {
        Error::Tool(format!("Windows 计划任务接口失败：{error}"))
    }

    fn system_time(value: PrimitiveDateTime) -> SYSTEMTIME {
        SYSTEMTIME {
            wYear: value.year() as u16,
            wMonth: u8::from(value.month()) as u16,
            wDay: value.day() as u16,
            wHour: value.hour() as u16,
            wMinute: value.minute() as u16,
            wSecond: value.second() as u16,
            wMilliseconds: 0,
            wDayOfWeek: 0,
        }
    }
    fn primitive(value: SYSTEMTIME) -> Result<PrimitiveDateTime> {
        let month = time::Month::try_from(value.wMonth as u8)
            .map_err(|_| Error::Config("系统月份无效".into()))?;
        let date = time::Date::from_calendar_date(value.wYear as i32, month, value.wDay as u8)
            .map_err(|_| Error::Config("系统日期无效".into()))?;
        date.with_hms(value.wHour as u8, value.wMinute as u8, value.wSecond as u8)
            .map_err(|_| Error::Config("系统时间无效".into()))
    }
    pub fn to_local(value: OffsetDateTime) -> Result<PrimitiveDateTime> {
        let utc = value.to_offset(time::UtcOffset::UTC);
        let input = system_time(PrimitiveDateTime::new(utc.date(), utc.time()));
        let mut output = SYSTEMTIME::default();
        if unsafe { SystemTimeToTzSpecificLocalTimeEx(std::ptr::null(), &input, &mut output) } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        primitive(output)
    }
    pub fn from_local(value: PrimitiveDateTime) -> Result<Option<OffsetDateTime>> {
        let mut output = SYSTEMTIME::default();
        if unsafe {
            TzSpecificLocalTimeToSystemTimeEx(std::ptr::null(), &system_time(value), &mut output)
        } == 0
        {
            return Ok(None);
        }
        let utc = primitive(output)?.assume_utc();
        Ok((to_local(utc)? == value).then_some(utc))
    }
}

#[cfg(target_os = "windows")]
pub use native::{from_local, identity, to_local};

pub fn administrative_identity() -> Result<String> {
    let (sid, elevated) = identity()?;
    if !elevated {
        return Err(Error::Config("需要以管理员身份运行 Sleepy Doll".into()));
    }
    if matches!(sid.as_str(), "S-1-5-18" | "S-1-5-19" | "S-1-5-20") {
        return Err(Error::Config(
            "定时任务只使用已登录的管理员用户，拒绝系统服务账户".into(),
        ));
    }
    Ok(sid)
}

#[cfg(not(target_os = "windows"))]
pub fn identity() -> Result<(String, bool)> {
    Err(Error::Config("定时任务插件仅支持 Windows".into()))
}
#[cfg(not(target_os = "windows"))]
pub fn to_local(value: OffsetDateTime) -> Result<PrimitiveDateTime> {
    Ok(PrimitiveDateTime::new(value.date(), value.time()))
}
#[cfg(not(target_os = "windows"))]
pub fn from_local(value: PrimitiveDateTime) -> Result<Option<OffsetDateTime>> {
    Ok(Some(value.assume_utc()))
}

impl TaskBackend for WindowsTasks {
    fn apply(&self, schedule: &Schedule) -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            native::with_task(self, schedule, "apply")
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = schedule;
            Err(Error::Config("仅支持 Windows".into()))
        }
    }
    fn delete(&self, schedule: &Schedule) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            native::with_task(self, schedule, "delete").map(|_| ())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = schedule;
            Err(Error::Config("仅支持 Windows".into()))
        }
    }
    fn verify(&self, schedule: &Schedule) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            native::with_task(self, schedule, "verify").map(|_| ())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = schedule;
            Err(Error::Config("仅支持 Windows".into()))
        }
    }
}

pub fn desktop_available() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::StationsAndDesktops::{
            CloseDesktop, DESKTOP_READOBJECTS, GetUserObjectInformationW, OpenInputDesktop,
            UOI_NAME,
        };
        let desktop = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if desktop.is_null() {
            return false;
        }
        let mut name = [0u16; 128];
        let mut needed = 0;
        let valid = GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            name.as_mut_ptr().cast(),
            (name.len() * 2) as u32,
            &mut needed,
        ) != 0;
        CloseDesktop(desktop);
        valid
            && String::from_utf16_lossy(
                &name[..name
                    .iter()
                    .position(|value| *value == 0)
                    .unwrap_or(name.len())],
            )
            .eq_ignore_ascii_case("Default")
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}
