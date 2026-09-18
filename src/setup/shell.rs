//! Windows 外壳接触面：`.lnk` 快捷方式、已知文件夹与系统文件夹选择框。
//!
//! 走原始 COM 与 Win32 调用：不引入新依赖，也不起子进程。

pub use platform::{browse_for_directory, create_shortcut, desktop_directory, programs_directory};

#[cfg(windows)]
mod platform {
    use std::{
        ffi::{OsStr, c_void},
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
        ptr,
    };

    use windows_sys::{
        Win32::{
            Foundation::HWND,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                BIF_EDITBOX, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW,
                FOLDERID_Desktop, FOLDERID_Programs, SHBrowseForFolderW, SHGetKnownFolderPath,
                SHGetPathFromIDListW,
            },
        },
        core::GUID,
    };

    use crate::setup::{Error, NAME, failed};

    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
    const IID_SHELL_LINK_W: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
    const IID_PERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

    /// 线程上的 COM 初始化。`.lnk` 与文件夹对话框都要求线程已初始化 COM。
    struct Apartment;

    impl Apartment {
        fn join() -> Result<Self, Error> {
            // S_OK 与 S_FALSE 都表示线程可用；两者都要配对一次 CoUninitialize。
            let result = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as u32) };
            if result < 0 {
                return Err(Error::message(format!("COM 初始化失败（0x{result:08X}）")));
            }
            Ok(Self)
        }
    }

    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    /// COM 接口指针。离开作用域时按 IUnknown 的第 3 个槽位调 Release。
    struct Interface(*mut c_void);

    impl Drop for Interface {
        fn drop(&mut self) {
            unsafe {
                let vtable = *(self.0 as *const *const *const c_void);
                let release =
                    *(vtable.add(2) as *const unsafe extern "system" fn(*mut c_void) -> u32);
                release(self.0);
            }
        }
    }

    /// `IShellLinkW` 的虚表。
    ///
    /// windows-sys 不导出 COM 接口定义，这里按 SDK 里的方法顺序自己声明。槽位顺序
    /// 不能改，调用不到的也必须占位，否则后面所有偏移都会错位。
    #[allow(dead_code)]
    #[repr(C)]
    struct ShellLinkVtable {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32,
        add_ref: *const c_void,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_path: *const c_void,
        get_id_list: *const c_void,
        set_id_list: *const c_void,
        get_description: *const c_void,
        set_description: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_working_directory: *const c_void,
        set_working_directory: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_arguments: *const c_void,
        set_arguments: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_hotkey: *const c_void,
        set_hotkey: *const c_void,
        get_show_cmd: *const c_void,
        set_show_cmd: *const c_void,
        get_icon_location: *const c_void,
        set_icon_location: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        set_relative_path: *const c_void,
        resolve: *const c_void,
        set_path: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
    }

    /// `IPersistFile` 的虚表，只声明到调用到的 `Save`。
    #[allow(dead_code)]
    #[repr(C)]
    struct PersistFileVtable {
        query_interface: *const c_void,
        add_ref: *const c_void,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_class_id: *const c_void,
        is_dirty: *const c_void,
        load: *const c_void,
        save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
    }

    /// 写一个指向 `target` 的快捷方式。`arguments` 为空就不设命令行参数。
    pub fn create_shortcut(link: &Path, target: &Path, arguments: &str) -> Result<(), Error> {
        let _apartment = Apartment::join()?;
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)
                .map_err(failed(format!("无法创建 {}", parent.display())))?;
        }

        let mut raw: *mut c_void = ptr::null_mut();
        let result = unsafe {
            CoCreateInstance(
                &CLSID_SHELL_LINK,
                ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_SHELL_LINK_W,
                &mut raw,
            )
        };
        if result < 0 || raw.is_null() {
            return Err(Error::message(format!(
                "无法创建快捷方式对象（0x{result:08X}）"
            )));
        }
        let _link_object = Interface(raw);
        let vtable = unsafe { *(raw as *const *const ShellLinkVtable) };
        let target_utf16 = wide(target.as_os_str());
        unsafe {
            ((*vtable).set_path)(raw, target_utf16.as_ptr());
            // 图标取主程序自己的，与注册表里的 DisplayIcon 一致。
            ((*vtable).set_icon_location)(raw, target_utf16.as_ptr(), 0);
            let description = wide(OsStr::new(NAME));
            ((*vtable).set_description)(raw, description.as_ptr());
            if let Some(parent) = target.parent() {
                let parent = wide(parent.as_os_str());
                ((*vtable).set_working_directory)(raw, parent.as_ptr());
            }
            if !arguments.is_empty() {
                let arguments = wide(OsStr::new(arguments));
                ((*vtable).set_arguments)(raw, arguments.as_ptr());
            }
        }

        let mut persist: *mut c_void = ptr::null_mut();
        let result = unsafe { ((*vtable).query_interface)(raw, &IID_PERSIST_FILE, &mut persist) };
        if result < 0 || persist.is_null() {
            return Err(Error::message(format!(
                "快捷方式不支持 IPersistFile（0x{result:08X}）"
            )));
        }
        let _persist_object = Interface(persist);
        let persist_vtable = unsafe { *(persist as *const *const PersistFileVtable) };
        let link_utf16 = wide(link.as_os_str());
        let result = unsafe { ((*persist_vtable).save)(persist, link_utf16.as_ptr(), 1) };
        if result < 0 {
            return Err(Error::message(format!(
                "无法写入 {}（0x{result:08X}）",
                link.display()
            )));
        }
        Ok(())
    }

    /// 开始菜单里「所有程序」那层。
    pub fn programs_directory() -> Option<PathBuf> {
        known_folder(&FOLDERID_Programs)
    }

    /// 用户的桌面。可能被重定向到 OneDrive，所以要问系统而不是拼 `%USERPROFILE%`。
    pub fn desktop_directory() -> Option<PathBuf> {
        known_folder(&FOLDERID_Desktop)
    }

    /// 系统文件夹选择框。用户取消时返回 `None`。
    pub fn browse_for_directory(owner: isize, title: &str) -> Result<Option<PathBuf>, Error> {
        let _apartment = Apartment::join()?;
        let title = wide(OsStr::new(title));
        // pszDisplayName 由系统填写，缓冲区至少要 MAX_PATH。
        let mut display = [0u16; 260];
        let info = BROWSEINFOW {
            hwndOwner: owner as HWND,
            pidlRoot: ptr::null_mut(),
            pszDisplayName: display.as_mut_ptr(),
            lpszTitle: title.as_ptr(),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_EDITBOX,
            lpfn: None,
            lParam: 0,
            iImage: 0,
        };
        let pidl = unsafe { SHBrowseForFolderW(&info) };
        if pidl.is_null() {
            return Ok(None);
        }
        let mut buffer = [0u16; 260];
        let found = unsafe { SHGetPathFromIDListW(pidl, buffer.as_mut_ptr()) } != 0;
        unsafe { CoTaskMemFree(pidl.cast()) };
        if !found {
            return Ok(None);
        }
        Ok(Some(unsafe { wide_to_path(buffer.as_ptr()) }))
    }

    fn known_folder(id: &GUID) -> Option<PathBuf> {
        let _apartment = Apartment::join().ok()?;
        let mut raw: *mut u16 = ptr::null_mut();
        let result = unsafe { SHGetKnownFolderPath(id, 0, ptr::null_mut(), &mut raw) };
        if result < 0 || raw.is_null() {
            return None;
        }
        let path = unsafe { wide_to_path(raw) };
        unsafe { CoTaskMemFree(raw.cast()) };
        Some(path)
    }

    /// 以 NUL 结尾的 UTF-16。
    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// 读走系统给出的以 NUL 结尾的 UTF-16 路径。
    unsafe fn wide_to_path(value: *const u16) -> PathBuf {
        let mut length = 0;
        while unsafe { *value.add(length) } != 0 {
            length += 1;
        }
        PathBuf::from(String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(value, length)
        }))
    }
}

#[cfg(not(windows))]
mod platform {
    use std::path::{Path, PathBuf};

    use crate::setup::Error;

    pub fn create_shortcut(_link: &Path, _target: &Path, _arguments: &str) -> Result<(), Error> {
        Ok(())
    }

    pub fn programs_directory() -> Option<PathBuf> {
        None
    }

    pub fn desktop_directory() -> Option<PathBuf> {
        None
    }

    pub fn browse_for_directory(_owner: isize, _title: &str) -> Result<Option<PathBuf>, Error> {
        Ok(None)
    }
}
