//! WebView2 的前置检查与整机补装。此模块不创建 WebView，原生安装界面可独立调用。

use std::path::Path;

pub const MINIMUM_VERSION: &str = "155.0.4283.62";
pub const DOWNLOAD_PAGE: &str =
    "https://developer.microsoft.com/microsoft-edge/webview2/#download-section";

/// Chromium 的版本是四段数字，不能按字符串或三段 semver 比较。
fn version_parts(version: &str) -> Option<[u32; 4]> {
    let mut parts = version.split('.');
    let mut numbers = [0; 4];
    for number in &mut numbers {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        *number = part.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

pub fn meets_minimum(version: &str) -> bool {
    version_parts(version).is_some_and(|version| {
        version >= version_parts(MINIMUM_VERSION).expect("最低 WebView2 版本格式错误")
    })
}

/// Evergreen 优先使用整机安装；没有整机安装时再读取当前用户安装。
pub fn installed_version() -> Option<String> {
    platform::installed_version()
}

pub fn machine_version() -> Option<String> {
    platform::machine_version()
}

/// 安装器和主程序共用同一个版本门槛，正式发行要求整机运行时。
pub fn requirement_message() -> Option<String> {
    match machine_version() {
        Some(version) if meets_minimum(&version) => None,
        Some(version) => Some(format!(
            "需要更新 WebView2（当前 {version}，需要 {MINIMUM_VERSION} 或更高版本）。请重新运行 Sleepy Doll 安装程序。"
        )),
        None => Some("需要安装 WebView2。请重新运行 Sleepy Doll 安装程序。".to_string()),
    }
}

/// 工作线程调用；scratch 由安装器管理，退出时统一清理。
/// 进度阶段为 download / install / verify，不声称知道微软引导程序内部的下载百分比。
pub fn ensure(scratch: &Path, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
    platform::ensure(scratch, progress)
}

#[cfg(any(windows, test))]
fn installation_result(exit_code: i32, version: Option<&str>) -> Result<(), String> {
    // ERROR_SUCCESS_REBOOT_REQUIRED 只在版本已经达标时接受。
    if !matches!(exit_code, 0 | 3010) {
        return Err(format!(
            "WebView2 安装失败（错误码 0x{:08X}）。",
            exit_code as u32
        ));
    }
    match version {
        Some(version) if meets_minimum(version) => Ok(()),
        Some(version) => Err(format!(
            "WebView2 尚未更新到所需版本（当前 {version}，需要 {MINIMUM_VERSION}）。"
        )),
        None => Err("未检测到整机安装的 WebView2，请安装后重试。".to_string()),
    }
}

#[cfg(windows)]
mod platform {
    use super::{installation_result, meets_minimum, version_parts};
    use std::{
        fs::{self, File, OpenOptions},
        io::Read,
        mem::size_of,
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle, process::CommandExt},
        path::Path,
        process::Command,
        ptr,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE},
        Security::{
            Cryptography::{CERT_NAME_ATTR_TYPE, CertGetNameStringW, szOID_ORGANIZATION_NAME},
            GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
            WinTrust::{
                WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
                WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
                WTD_UI_NONE, WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
                WTHelperProvDataFromStateData, WinVerifyTrust,
            },
        },
        Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ},
        System::{
            Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY,
                KEY_WOW64_64KEY, REG_SZ, RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
            },
            Threading::{CREATE_NO_WINDOW, GetCurrentProcess, OpenProcessToken},
        },
    };

    const BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
    const BOOTSTRAPPER_NAME: &str = "MicrosoftEdgeWebview2Setup.exe";
    const MAX_DOWNLOAD_BYTES: u64 = 32 * 1024 * 1024;
    const RUNTIME_KEY: &str =
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

    pub fn installed_version() -> Option<String> {
        machine_version().or_else(|| registry_version(HKEY_CURRENT_USER))
    }

    pub fn machine_version() -> Option<String> {
        registry_version(HKEY_LOCAL_MACHINE)
    }

    fn registry_version(root: HKEY) -> Option<String> {
        // HKLM 的官方位置在 32 位注册表视图；两种视图都查，兼容 32/64 位宿主。
        [KEY_WOW64_32KEY, KEY_WOW64_64KEY]
            .into_iter()
            .filter_map(|view| read_version(root, view))
            .filter(|version| version_parts(version).is_some_and(|v| v != [0; 4]))
            .max_by_key(|version| version_parts(version))
    }

    fn read_version(root: HKEY, view: u32) -> Option<String> {
        let key_name: Vec<u16> = RUNTIME_KEY.encode_utf16().chain(Some(0)).collect();
        let value_name: Vec<u16> = "pv".encode_utf16().chain(Some(0)).collect();
        let mut key = ptr::null_mut();
        if unsafe { RegOpenKeyExW(root, key_name.as_ptr(), 0, KEY_READ | view, &mut key) }
            != ERROR_SUCCESS
        {
            return None;
        }
        let mut buffer = [0u16; 128];
        let mut bytes = size_of::<[u16; 128]>() as u32;
        let mut kind = 0;
        let result = unsafe {
            RegQueryValueExW(
                key,
                value_name.as_ptr(),
                ptr::null(),
                &mut kind,
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        unsafe { RegCloseKey(key) };
        if result != ERROR_SUCCESS || kind != REG_SZ || !bytes.is_multiple_of(2) {
            return None;
        }
        let count = (bytes as usize / 2).min(buffer.len());
        let end = buffer[..count]
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(count);
        String::from_utf16(&buffer[..end]).ok()
    }

    fn elevated() -> bool {
        let mut token: HANDLE = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned = 0;
        let result = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                (&mut elevation as *mut TOKEN_ELEVATION).cast(),
                size_of::<TOKEN_ELEVATION>() as u32,
                &mut returned,
            )
        };
        unsafe { CloseHandle(token) };
        result != 0 && elevation.TokenIsElevated != 0
    }

    pub fn ensure(scratch: &Path, progress: &mut dyn FnMut(&str)) -> Result<(), String> {
        if !elevated() {
            return Err("请以管理员身份运行安装程序。".to_string());
        }
        if machine_version().is_some_and(|version| meets_minimum(&version)) {
            return Ok(());
        }
        let directory = scratch.join("webview2-runtime");
        fs::create_dir_all(&directory).map_err(|error| format!("无法准备运行组件：{error}"))?;
        reject_link(&directory)?;
        let installer = directory.join(BOOTSTRAPPER_NAME);
        if installer.exists() {
            reject_link(&installer)?;
        }
        progress("download");
        download(&installer)?;
        // 禁止其他进程在签名检查和启动之间替换已校验的文件。
        let locked_file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&installer)
            .map_err(|error| format!("无法读取 WebView2 安装程序：{error}"))?;
        verify_microsoft_signature(&installer, &locked_file)?;
        progress("install");
        let mut child = Command::new(&installer)
            .args(["/silent", "/install"])
            .current_dir(&directory)
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| format!("无法启动 WebView2 安装程序：{error}"))?;
        let deadline = Instant::now() + Duration::from_secs(10 * 60);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(200));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("WebView2 安装等待超时，请检查网络后重试。".to_string());
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("无法确认 WebView2 安装结果：{error}"));
                }
            }
        };
        drop(locked_file);
        progress("verify");
        let code = status.code().unwrap_or(-1);
        // 等待 Edge Updater 发布版本登记，退出码为零不能替代安装后的检测。
        if matches!(code, 0 | 3010) {
            for _ in 0..20 {
                if machine_version().is_some_and(|version| meets_minimum(&version)) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        installation_result(code, machine_version().as_deref())
    }

    fn reject_link(path: &Path) -> Result<(), String> {
        use std::os::windows::fs::MetadataExt;
        let metadata =
            fs::symlink_metadata(path).map_err(|error| format!("无法读取运行组件路径：{error}"))?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err("运行组件缓存路径不能是链接。".to_string());
        }
        Ok(())
    }

    fn trusted_download_url(url: &url::Url) -> bool {
        url.scheme() == "https"
            && url
                .host_str()
                .is_some_and(|host| host == "microsoft.com" || host.ends_with(".microsoft.com"))
    }

    fn download(path: &Path) -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 8 || !trusted_download_url(attempt.url()) {
                    attempt.error("WebView2 下载地址无效")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|error| format!("无法连接下载服务：{error}"))?;
        let response = client
            .get(BOOTSTRAPPER_URL)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| format!("WebView2 下载失败：{error}"))?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
        {
            return Err("WebView2 安装程序大小异常。".to_string());
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_DOWNLOAD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("WebView2 下载失败：{error}"))?;
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES || !bytes.starts_with(b"MZ") {
            return Err("下载的 WebView2 安装程序无效。".to_string());
        }
        fs::write(path, bytes).map_err(|error| format!("无法保存 WebView2 安装程序：{error}"))
    }

    fn verify_microsoft_signature(path: &Path, file: &File) -> Result<(), String> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut information = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: wide.as_ptr(),
            hFile: file.as_raw_handle(),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 {
                pFile: &mut information,
            },
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let result = unsafe {
            WinVerifyTrust(
                ptr::null_mut(),
                &mut action,
                (&mut data as *mut WINTRUST_DATA).cast(),
            )
        };
        let microsoft = result == 0
            && unsafe {
                let provider = WTHelperProvDataFromStateData(data.hWVTStateData);
                if provider.is_null() {
                    false
                } else {
                    let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
                    if signer.is_null() {
                        false
                    } else {
                        let certificate = WTHelperGetProvCertFromChain(signer, 0);
                        if certificate.is_null() || (*certificate).pCert.is_null() {
                            false
                        } else {
                            let mut organization = [0u16; 128];
                            let length = CertGetNameStringW(
                                (*certificate).pCert,
                                CERT_NAME_ATTR_TYPE,
                                0,
                                szOID_ORGANIZATION_NAME.cast(),
                                organization.as_mut_ptr(),
                                organization.len() as u32,
                            ) as usize;
                            length > 1
                                && length <= organization.len()
                                && String::from_utf16_lossy(&organization[..length - 1])
                                    == "Microsoft Corporation"
                        }
                    }
                }
            };
        // 无论验证成功与否，必须释放 WinTrust 的证书链与状态句柄。
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        unsafe {
            WinVerifyTrust(
                ptr::null_mut(),
                &mut action,
                (&mut data as *mut WINTRUST_DATA).cast(),
            );
        }
        if microsoft {
            Ok(())
        } else {
            Err("WebView2 安装程序的微软数字签名验证失败。".to_string())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::trusted_download_url;

        #[test]
        #[ignore = "需要连接微软下载服务；仅下载和验签，不执行安装程序"]
        fn official_bootstrapper_download_and_signature() {
            use std::os::windows::fs::OpenOptionsExt;
            let root = crate::test_root("webview-bootstrapper");
            std::fs::create_dir_all(&root).unwrap();
            let path = root.join(super::BOOTSTRAPPER_NAME);
            super::download(&path).unwrap();
            let locked = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(super::FILE_SHARE_READ)
                .open(&path)
                .unwrap();
            super::verify_microsoft_signature(&path, &locked).unwrap();
        }

        #[test]
        fn download_only_follows_official_https_hosts() {
            for url in [
                "https://go.microsoft.com/fwlink/p/?LinkId=2124703",
                "https://msedge.sf.dl.delivery.mp.microsoft.com/file.exe",
            ] {
                assert!(trusted_download_url(&url.parse().unwrap()), "{url}");
            }
            for url in [
                "http://go.microsoft.com/file.exe",
                "https://microsoft.com.example.org/file.exe",
                "https://notmicrosoft.com/file.exe",
                "https://example.org/microsoft.com/file.exe",
            ] {
                assert!(!trusted_download_url(&url.parse().unwrap()), "{url}");
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;
    pub fn installed_version() -> Option<String> {
        None
    }
    pub fn machine_version() -> Option<String> {
        None
    }
    pub fn ensure(_: &Path, _: &mut dyn FnMut(&str)) -> Result<(), String> {
        Err("WebView2 运行组件仅适用于 Windows。".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM_VERSION, installation_result, meets_minimum, version_parts};

    #[test]
    fn compares_all_four_components_numerically() {
        assert!(meets_minimum(MINIMUM_VERSION));
        assert!(meets_minimum("155.0.4283.100"));
        assert!(meets_minimum("156.0.0.0"));
        assert!(!meets_minimum("155.0.4283.61"));
        assert!(!meets_minimum("154.999.9999.999"));
        assert!(!meets_minimum("99.0.9999.999"));
    }

    #[test]
    fn rejects_partial_preview_and_malformed_versions() {
        for version in [
            "",
            "0.0.0.0",
            "155.0.4283",
            "155.0.4283.62 beta",
            "155.0.4283.62.1",
            "+155.0.4283.62",
            "155.0.4283.-1",
            "4294967296.0.0.0",
        ] {
            assert!(!meets_minimum(version), "{version}");
        }
        assert_eq!(version_parts("155.0.4283.62"), Some([155, 0, 4283, 62]));
    }

    #[test]
    fn success_requires_rechecked_machine_version() {
        assert!(installation_result(0, None).is_err());
        assert!(installation_result(0, Some("154.0.0.1")).is_err());
        assert!(installation_result(0, Some(MINIMUM_VERSION)).is_ok());
        assert!(installation_result(3010, Some(MINIMUM_VERSION)).is_ok());
        assert!(installation_result(3010, None).is_err());
        assert!(installation_result(1603, Some(MINIMUM_VERSION)).is_err());
    }
}
