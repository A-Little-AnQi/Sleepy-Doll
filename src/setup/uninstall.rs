//! 卸载：按安装时写入的清单删文件，再清快捷方式与注册表，最后安排退出后的清理。

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::setup::{
    DATA_DIRECTORY, Error, Progress, Removal, UNINSTALLER, cleanup_after_exit, payload::Archive,
    shell,
};

/// 从 `directory` 卸载程序和用户数据，包括旧布局与漫游回退目录。
pub fn uninstall(archive: &Archive, directory: &Path, progress: Progress) -> Result<(), Error> {
    let fallback = crate::config::fallback_directory();
    validate_installation(archive, directory, fallback.as_deref())?;
    progress(0.0, "正在移除文件…");
    // 运行中的程序与 BetterGI 加载的桥都锁着自己的文件，动手之前先查一遍。
    let mut programs = removals(archive, directory);
    cached_programs(&directory.join("bridge-cache"), &mut programs);
    if let Some(busy) = busy_program(&programs) {
        return Err(Error::message(busy_message(busy)));
    }

    let leftovers = remove_files(archive, directory, fallback.as_deref());
    if !leftovers.is_empty() {
        return Err(Error::message(format!(
            "以下文件没能删除：{}。请退出占用它们的程序，再重新卸载。",
            listing(&leftovers)
        )));
    }

    progress(0.6, "正在删除快捷方式…");
    remove_shortcuts();

    progress(0.8, "正在清理注册表…");
    super::registry::remove()?;

    progress(0.95, "正在收尾…");
    // 卸载入口与空掉的安装目录都要等本进程退出后才删得掉。
    cleanup_after_exit(&cleanup_plan(directory))?;
    progress(1.0, "卸载完成");
    Ok(())
}

pub(super) fn cleanup_plan(directory: &Path) -> Vec<Removal> {
    vec![Removal::Tree(directory.to_path_buf())]
}

fn blocking_files(archive: &Archive, directory: &Path) -> Vec<PathBuf> {
    let mut files = removals(archive, directory);
    cached_programs(&directory.join("bridge-cache"), &mut files);
    files.retain(|path| is_program(path) && in_use(path));
    files
}

pub fn blocking_programs(archive: &Archive, directory: &Path) -> Result<Vec<String>, Error> {
    validate_installation(
        archive,
        directory,
        crate::config::fallback_directory().as_deref(),
    )?;
    let mut names = Vec::new();
    for path in blocking_files(archive, directory) {
        let owners = lock_owners(&path);
        if owners.is_empty() {
            names.push(path.display().to_string());
        } else {
            names.extend(owners);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

/// 仅在用户点击“关闭占用程序并卸载”后调用，正常关闭已登记文件的占用者。
pub fn close_blocking_programs(archive: &Archive, directory: &Path) -> Result<(), Error> {
    validate_installation(
        archive,
        directory,
        crate::config::fallback_directory().as_deref(),
    )?;
    let files = blocking_files(archive, directory);
    if files.is_empty() {
        return Ok(());
    }
    graceful_close(&files)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if blocking_files(archive, directory).is_empty() {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::message(
                "占用程序尚未退出，卸载未开始。请确认程序已保存工作后重试。",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

#[cfg(windows)]
fn graceful_close(files: &[PathBuf]) -> Result<(), Error> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    #[link(name = "rstrtmgr")]
    unsafe extern "system" {
        fn RmStartSession(handle: *mut u32, flags: u32, key: *mut u16) -> u32;
        fn RmRegisterResources(
            handle: u32,
            count: u32,
            files: *const *const u16,
            p: u32,
            processes: *const std::ffi::c_void,
            s: u32,
            services: *const *const u16,
        ) -> u32;
        fn RmShutdown(
            handle: u32,
            flags: u32,
            callback: Option<unsafe extern "system" fn(u32)>,
        ) -> u32;
        fn RmEndSession(handle: u32) -> u32;
    }
    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                RmEndSession(self.0);
            }
        }
    }
    unsafe {
        let mut handle = 0;
        let mut key = [0u16; 33];
        let result = RmStartSession(&mut handle, 0, key.as_mut_ptr());
        if result != 0 {
            return Err(Error::message(format!(
                "无法准备关闭占用程序（错误码 {result}）"
            )));
        }
        let session = Session(handle);
        let names: Vec<Vec<u16>> = files
            .iter()
            .map(|p| p.as_os_str().encode_wide().chain(Some(0)).collect())
            .collect();
        let pointers: Vec<*const u16> = names.iter().map(|s| s.as_ptr()).collect();
        let result = RmRegisterResources(
            session.0,
            pointers.len() as u32,
            pointers.as_ptr(),
            0,
            ptr::null(),
            0,
            ptr::null(),
        );
        if result != 0 {
            return Err(Error::message(format!(
                "无法登记占用文件（错误码 {result}）"
            )));
        }
        // flags=0：尊重应用的正常退出/保存流程，绝不使用 RmForceShutdown。
        let result = RmShutdown(session.0, 0, None);
        if result != 0 {
            return Err(Error::message(format!(
                "占用程序未能正常退出，卸载未开始（错误码 {result}）"
            )));
        }
        Ok(())
    }
}
#[cfg(not(windows))]
fn graceful_close(_: &[PathBuf]) -> Result<(), Error> {
    Err(Error::message("自动关闭占用程序仅支持 Windows"))
}

/// 名称、安装文件与路径边界必须同时成立，不能只凭目录名删除整棵树。
fn validate_installation(
    archive: &Archive,
    directory: &Path,
    fallback: Option<&Path>,
) -> Result<(), Error> {
    super::validate_directory(&directory.to_string_lossy())?;
    if !directory.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case(super::NAME) || name.eq_ignore_ascii_case(super::SLUG)
    }) || ![super::EXECUTABLE, UNINSTALLER]
        .iter()
        .any(|name| directory.join(name).is_file())
    {
        return Err(Error::message("卸载目录不属于 Sleepy Doll，未删除任何文件"));
    }
    // 校验包括安装根本身在内的所有上级目录。逐项删除不能穿过链接。
    let mut targets: Vec<Removal> = removals(archive, directory)
        .into_iter()
        .map(Removal::File)
        .collect();
    targets.push(Removal::File(directory.join(UNINSTALLER)));
    targets.extend(
        data_directories(directory, fallback)
            .into_iter()
            .map(Removal::Tree),
    );
    super::cleanup::validate(&targets)
}

fn data_directories(directory: &Path, fallback: Option<&Path>) -> Vec<PathBuf> {
    let mut directories = vec![
        directory.join(DATA_DIRECTORY),
        directory.join("bridge").join(DATA_DIRECTORY),
    ];
    if let Some(fallback) = fallback {
        directories.push(fallback.to_path_buf());
    }
    directories
}

fn cached_programs(directory: &Path, targets: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::symlink_metadata(directory) else {
        return;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return;
        }
    }
    if metadata.file_type().is_symlink() {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            cached_programs(&path, targets);
        } else if is_program(&path) {
            targets.push(path);
        }
    }
}

/// 卸载要删的文件：载荷清单里的每一项。
fn removals(archive: &Archive, directory: &Path) -> Vec<PathBuf> {
    archive
        .files()
        .into_iter()
        .map(|(path, _)| super::install::destination(directory, archive, path))
        .collect()
}

/// 删掉安装写在目录里的东西，返回没能删掉的。
///
/// `fallback` 是安装目录不可写时用户数据落到的地方（`%APPDATA%\Sleepy Doll`）。
/// 单个文件删不掉不中断，剩下的照样清。
pub(super) fn remove_files(
    archive: &Archive,
    directory: &Path,
    fallback: Option<&Path>,
) -> Vec<PathBuf> {
    super::remove_retired_skills(archive, directory);
    let mut removed = Vec::new();
    let mut leftovers = Vec::new();
    for target in removals(archive, directory) {
        match fs::remove_file(&target) {
            Ok(()) => removed.push(target),
            // 已经不在了就当删过了。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => leftovers.push(target),
        }
    }
    remove_empty_directories(&removed, directory);

    for data in data_directories(directory, fallback) {
        match fs::symlink_metadata(&data) {
            Ok(metadata) => {
                // remove_dir_all 不遍历目录链接；文件链接则只删除链接本身。
                let result = if metadata.is_dir() {
                    fs::remove_dir_all(&data)
                } else {
                    fs::remove_file(&data)
                };
                if result.is_err() {
                    leftovers.push(data);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => leftovers.push(data),
        }
    }

    // 删掉空掉的安装目录。
    let _ = fs::remove_dir(directory);
    leftovers
}

/// 第一个被占用、删不掉的程序文件。
pub(super) fn busy_program(targets: &[PathBuf]) -> Option<&PathBuf> {
    targets.iter().find(|path| is_program(path) && in_use(path))
}

fn busy_message(path: &Path) -> String {
    let owners = lock_owners(path);
    if owners.is_empty() {
        format!(
            "文件正在使用中，请关闭占用它的程序后重试卸载：{}",
            path.display()
        )
    } else {
        format!(
            "{} 正在使用此文件，请关闭后重试卸载：{}",
            owners.join("、"),
            path.display()
        )
    }
}

/// Restart Manager 只查询占用者，不关闭或重启任何进程。
#[cfg(windows)]
fn lock_owners(path: &Path) -> Vec<String> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Foundation::FILETIME;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Process {
        pid: u32,
        started: FILETIME,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Info {
        process: Process,
        name: [u16; 256],
        service: [u16; 64],
        kind: u32,
        status: u32,
        session: u32,
        restartable: i32,
    }
    #[link(name = "rstrtmgr")]
    unsafe extern "system" {
        fn RmStartSession(handle: *mut u32, flags: u32, key: *mut u16) -> u32;
        fn RmRegisterResources(
            handle: u32,
            files: u32,
            names: *const *const u16,
            processes: u32,
            ids: *const std::ffi::c_void,
            services: u32,
            service_names: *const *const u16,
        ) -> u32;
        fn RmGetList(
            handle: u32,
            needed: *mut u32,
            count: *mut u32,
            info: *mut Info,
            reasons: *mut u32,
        ) -> u32;
        fn RmEndSession(handle: u32) -> u32;
    }
    struct Session(u32);
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                RmEndSession(self.0);
            }
        }
    }
    unsafe {
        let mut handle = 0;
        let mut key = [0u16; 33];
        if RmStartSession(&mut handle, 0, key.as_mut_ptr()) != 0 {
            return Vec::new();
        }
        let session = Session(handle);
        let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if RmRegisterResources(session.0, 1, &name.as_ptr(), 0, ptr::null(), 0, ptr::null()) != 0 {
            return Vec::new();
        }
        let mut needed = 0;
        let mut count = 0;
        let mut reasons = 0;
        let result = RmGetList(
            session.0,
            &mut needed,
            &mut count,
            ptr::null_mut(),
            &mut reasons,
        );
        if result != 234 {
            return Vec::new();
        }
        for _ in 0..3 {
            if needed == 0 || needed > 1024 {
                return Vec::new();
            }
            let mut info = vec![std::mem::zeroed::<Info>(); needed as usize];
            count = needed;
            match RmGetList(
                session.0,
                &mut needed,
                &mut count,
                info.as_mut_ptr(),
                &mut reasons,
            ) {
                0 => {
                    return info
                        .iter()
                        .take(count as usize)
                        .map(|i| {
                            let end = i.name.iter().position(|&c| c == 0).unwrap_or(i.name.len());
                            let name = String::from_utf16_lossy(&i.name[..end]);
                            format!(
                                "{}（PID {}）",
                                if name.is_empty() {
                                    "未知程序"
                                } else {
                                    &name
                                },
                                i.process.pid
                            )
                        })
                        .collect();
                }
                234 => continue,
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }
}
#[cfg(not(windows))]
fn lock_owners(_: &Path) -> Vec<String> {
    Vec::new()
}

/// 是不是程序文件：`.exe` 与 `.dll`。
fn is_program(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("dll")
    })
}

/// 文件被别的进程占着了吗。只读文件不算占用。
fn in_use(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() || metadata.permissions().readonly() {
        return false;
    }
    fs::OpenOptions::new().write(true).open(path).is_err()
}

/// 清单里的路径，逐个列给用户看。
fn listing(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("、")
}

/// 删掉因文件移除而空掉的目录。从文件所在目录往上收到 `root` 为止，
/// `remove_dir` 只删得掉空目录。
fn remove_empty_directories(removed: &[PathBuf], root: &Path) {
    let mut directories: Vec<PathBuf> = removed
        .iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();
    for directory in directories {
        let mut current = Some(directory.as_path());
        while let Some(path) = current {
            if path == root {
                let _ = fs::remove_dir(path);
                break;
            }
            if !path.starts_with(root) {
                break;
            }
            if fs::remove_dir(path).is_err() {
                break;
            }
            current = path.parent();
        }
    }
}

/// 开始菜单与桌面上的快捷方式。
fn remove_shortcuts() {
    let name = super::install::shortcut_name();
    for directory in [shell::programs_directory(), shell::desktop_directory()]
        .into_iter()
        .flatten()
    {
        let _ = fs::remove_file(directory.join(&name));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    #[ignore = "需要 Windows 交互桌面；仅关闭本测试创建的占用进程"]
    fn uninstall_gracefully_closes_fixture_owner() {
        use std::{
            os::windows::process::CommandExt,
            time::{Duration, Instant},
        };
        let root = crate::test_root("graceful-owner");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("fixture.dll");
        fs::write(&file, b"fixture").unwrap();
        let ready = root.join("ready");
        let script = root.join("host.ps1");
        fs::write(
            &script,
            r#"param($file,$ready)
Add-Type -AssemblyName System.Windows.Forms
$lock=[IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
$form=New-Object System.Windows.Forms.Form
$form.Text='Sleepy Doll uninstall fixture'
$form.ShowInTaskbar=$false
$form.Opacity=0
$form.Add_Shown({[IO.File]::WriteAllText($ready,'ready')})
try{[System.Windows.Forms.Application]::Run($form)}finally{$lock.Dispose();$form.Dispose()}
"#,
        )
        .unwrap();
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                if self.0.try_wait().ok().flatten().is_none() {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
        }
        let mut child = Child(
            std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Sta",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(script)
                .arg(&file)
                .arg(&ready)
                .creation_flags(0x08000000)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(Instant::now() < deadline, "fixture did not start");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(in_use(&file));
        assert!(
            lock_owners(&file)
                .iter()
                .any(|owner| owner.contains(&format!("PID {}", child.0.id())))
        );
        graceful_close(std::slice::from_ref(&file)).unwrap();
        while child.0.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "fixture did not exit");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(!in_use(&file));
    }

    #[test]
    fn uninstall_cleanup_preflight_detects_loaded_cached_bridge() {
        let root = crate::test_root("cached-lock");
        fs::create_dir_all(root.join("bridge-cache/hash")).unwrap();
        let path = root.join("bridge-cache/hash/BgiBridge.dll");
        fs::write(&path, b"bridge").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let mut programs = Vec::new();
        cached_programs(&root.join("bridge-cache"), &mut programs);
        assert_eq!(busy_program(&programs), Some(&path));
        let owners = lock_owners(&path);
        assert!(
            owners
                .iter()
                .any(|owner| owner.contains(&format!("PID {}", std::process::id()))),
            "{owners:?}"
        );
        assert!(busy_message(&path).contains(&format!("PID {}", std::process::id())));
        drop(lock);
        assert!(busy_program(&programs).is_none());
    }

    #[test]
    fn uninstall_cleanup_rejects_unrelated_installation_roots_before_deleting() {
        let archive = Archive::new(b"[]", vec![]).unwrap();
        for path in ["D:\\", "D:\\BetterGI", "C:\\Windows"] {
            assert!(uninstall(&archive, Path::new(path), &mut |_, _| {}).is_err());
        }
    }

    #[test]
    fn uninstall_cleanup_requires_product_files_before_deleting_a_named_directory() {
        let root = crate::test_root("uninstall-root-identity").join(super::super::NAME);
        fs::create_dir_all(root.join("user")).unwrap();
        let sentinel = root.join("user/config.json");
        fs::write(&sentinel, b"unrelated data").unwrap();
        let archive = Archive::new(b"[]", vec![]).unwrap();
        assert!(validate_installation(&archive, &root, None).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"unrelated data");
        fs::write(root.join(UNINSTALLER), b"fixture").unwrap();
        assert!(validate_installation(&archive, &root, None).is_ok());
    }

    #[test]
    fn uninstall_cleanup_preflight_refuses_linked_manifest_parents() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use std::os::windows::process::CommandExt;

        let fixture = crate::test_root("uninstall-linked-manifest");
        let root = fixture.join(super::super::NAME);
        let outside = fixture.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(root.join(UNINSTALLER), b"fixture").unwrap();
        fs::write(outside.join("keep.txt"), b"outside installation").unwrap();
        let data = STANDARD.encode(
            serde_json::to_vec(&serde_json::json!({"link":root.join("plugins"),"target":outside}))
                .unwrap(),
        );
        let script = format!(
            "$d=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{data}'))|ConvertFrom-Json; New-Item -ItemType Junction -Path $d.link -Target $d.target -ErrorAction Stop | Out-Null"
        );
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        assert!(
            std::process::Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
                .creation_flags(0x08000000)
                .status()
                .unwrap()
                .success()
        );
        let archive = Archive::new(br#"[{"path":"plugins/keep.txt","size":0}]"#, vec![]).unwrap();
        assert!(validate_installation(&archive, &root, None).is_err());
        assert_eq!(
            fs::read(outside.join("keep.txt")).unwrap(),
            b"outside installation"
        );
        assert!(root.join(UNINSTALLER).is_file());
    }
}
