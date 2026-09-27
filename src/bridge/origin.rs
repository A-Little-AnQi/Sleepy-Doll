//! BGI 离线来源校验。只调用本机元数据解析器，不使用版本白名单或在线认证。
use crate::error::{Error, Result};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

pub const REJECTED_MESSAGE: &str = "连接失败，请使用官方版本的BetterGI。";
type Cached = (u64, SystemTime, String, Value);

pub fn inspect_path(path: &Path) -> Result<Value> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    let path = path
        .canonicalize()
        .map_err(|_| Error::Tool(REJECTED_MESSAGE.into()))?;
    let metadata = path
        .metadata()
        .map_err(|_| Error::Tool(REJECTED_MESSAGE.into()))?;
    let modified = metadata
        .modified()
        .map_err(|_| Error::Tool(REJECTED_MESSAGE.into()))?;
    let parser_file = super::control::directory()?
        .join("BgiBridge.dll")
        .metadata()?;
    let parser = format!("{}:{:?}", parser_file.len(), parser_file.modified()?);
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((size, stamp, cached_parser, result)) = cache.lock().unwrap().get(&path)
        && *size == metadata.len()
        && *stamp == modified
        && cached_parser == &parser
    {
        return Ok(result.clone());
    }
    let result = super::control::recovery(
        "origin",
        &[path
            .to_str()
            .ok_or_else(|| Error::Tool(REJECTED_MESSAGE.into()))?],
    )?;
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 16 {
        cache.clear();
    }
    cache.insert(path, (metadata.len(), modified, parser, result.clone()));
    Ok(result)
}

pub fn require_path(path: &Path) -> Result<()> {
    let result = inspect_path(path)?;
    if result["state"] != "official" {
        log::warn!(
            "BGI 来源拒绝：state={} evidence={}",
            result["state"],
            result["evidence"]
        );
        return Err(Error::Tool(REJECTED_MESSAGE.into()));
    }
    Ok(())
}

pub fn require_info(info: &Value) -> Result<()> {
    let pid = info["processId"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .ok_or_else(|| Error::Tool(REJECTED_MESSAGE.into()))?;
    require_path(&process_path(pid)?)
}

pub fn require_running_hosts() -> Result<()> {
    for pid in host_processes()? {
        require_path(&process_path(pid)?)?;
    }
    Ok(())
}

#[cfg(windows)]
fn process_path(pid: u32) -> Result<PathBuf> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
    };
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Err(Error::Tool(REJECTED_MESSAGE.into()));
    }
    let mut text = vec![0u16; 32768];
    let mut size = text.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, text.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        return Err(Error::Tool(REJECTED_MESSAGE.into()));
    }
    Ok(PathBuf::from(String::from_utf16_lossy(
        &text[..size as usize],
    )))
}

#[cfg(windows)]
fn host_processes() -> Result<Vec<u32>> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    let handle = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if handle == INVALID_HANDLE_VALUE {
        return Err(Error::Tool(REJECTED_MESSAGE.into()));
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut valid = unsafe { Process32FirstW(handle, &mut entry) };
    let mut pids = Vec::new();
    while valid != 0 {
        let length = entry
            .szExeFile
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case("BetterGI.exe")
        {
            pids.push(entry.th32ProcessID);
        }
        valid = unsafe { Process32NextW(handle, &mut entry) };
    }
    unsafe {
        CloseHandle(handle);
    }
    Ok(pids)
}

#[cfg(not(windows))]
fn process_path(_: u32) -> Result<PathBuf> {
    Err(Error::Tool(REJECTED_MESSAGE.into()))
}
#[cfg(not(windows))]
fn host_processes() -> Result<Vec<u32>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_origin_claim_is_not_a_substitute_for_a_real_host_pid() {
        for value in [
            serde_json::json!({"hostOrigin":{"state":"official"}}),
            serde_json::json!({"processId":0}),
            serde_json::json!({"processId":-1}),
            serde_json::json!({"processId":u64::MAX}),
        ] {
            let error = require_info(&value).unwrap_err();
            assert_eq!(error.user_message(), REJECTED_MESSAGE);
        }
    }
}
