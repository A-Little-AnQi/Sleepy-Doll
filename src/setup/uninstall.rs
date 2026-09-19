//! 卸载：按安装时写入的清单删文件，再清快捷方式与注册表，最后安排退出后的清理。

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::setup::{
    DATA_DIRECTORY, Error, Progress, Removal, UNINSTALLER, cleanup_after_exit, payload::Archive,
    shell,
};

/// 从 `directory` 卸载。按载荷清单删文件，`user\` 由 `remove_user_data` 决定。
pub fn uninstall(
    archive: &Archive,
    directory: &Path,
    remove_user_data: bool,
    progress: Progress,
) -> Result<(), Error> {
    progress(0.0, "正在移除文件…");
    // 运行中的程序与 BetterGI 加载的桥都锁着自己的文件，动手之前先查一遍。
    if let Some(busy) = busy_program(&removals(archive, directory)) {
        return Err(Error::message(format!(
            "{} 正在使用中：请先退出 Sleepy Doll（连着 BetterGI 的话也退出它），再重新卸载。",
            busy.display()
        )));
    }

    let leftovers = remove_files(
        archive,
        directory,
        remove_user_data,
        crate::config::fallback_directory().as_deref(),
    );
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
    cleanup_after_exit(&[
        Removal::File(directory.join(UNINSTALLER)),
        Removal::Empty(directory.to_path_buf()),
    ]);
    progress(1.0, "卸载完成");
    Ok(())
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
fn remove_files(
    archive: &Archive,
    directory: &Path,
    remove_user_data: bool,
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

    if remove_user_data {
        let user = directory.join(DATA_DIRECTORY);
        if user.is_dir() && fs::remove_dir_all(&user).is_err() {
            leftovers.push(user);
        }
        // 旧布局下桥会在自己旁边另建一棵 user\。
        let nested = directory.join("bridge").join(DATA_DIRECTORY);
        if nested.is_dir() {
            let _ = fs::remove_dir_all(&nested);
        }
        // 安装目录不可写时程序把数据落在漫游目录里。
        if let Some(fallback) = fallback
            && fallback.is_dir()
            && fs::remove_dir_all(fallback).is_err()
        {
            leftovers.push(fallback.to_path_buf());
        }
    }

    // 删掉空掉的安装目录。
    let _ = fs::remove_dir(directory);
    leftovers
}

/// 第一个被占用、删不掉的程序文件。
fn busy_program(targets: &[PathBuf]) -> Option<&PathBuf> {
    targets.iter().find(|path| is_program(path) && in_use(path))
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

#[cfg(test)]
mod tests {
    //! 这里只测文件层面的动作。`uninstall()` 会动真实的快捷方式与注册表，不在这里执行。

    use super::*;

    fn archive(files: &[(&str, &[u8])]) -> Archive {
        let manifest = files
            .iter()
            .map(|(path, bytes)| format!(r#"{{"path":"{path}","size":{}}}"#, bytes.len()))
            .collect::<Vec<_>>()
            .join(",");
        let data = files
            .iter()
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect();
        Archive::new(format!("[{manifest}]").as_bytes(), data).unwrap()
    }

    fn target() -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(&target).unwrap();
        (root, target)
    }

    #[test]
    fn installed_files_and_their_directories_go_away() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("sleepy-doll.exe"), b"exe").unwrap();
        fs::write(target.join("bridge/BgiBridge.dll"), b"dll").unwrap();
        let archive = archive(&[
            ("sleepy-doll.exe", b"exe"),
            ("bridge/BgiBridge.dll", b"dll"),
        ]);

        let leftovers = remove_files(&archive, &target, false, None);

        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert!(!target.join("sleepy-doll.exe").exists());
        assert!(!target.join("bridge").exists());
    }

    #[test]
    fn nested_skill_directories_are_collected() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("skills/bgi-assistant/references")).unwrap();
        fs::write(
            target.join("skills/bgi-assistant/references/a.md"),
            b"skill",
        )
        .unwrap();
        let archive = archive(&[("skills/bgi-assistant/references/a.md", b"skill")]);

        remove_files(&archive, &target, false, None);

        assert!(!target.join("skills").exists());
        assert!(!target.exists());
    }

    /// 桥的配置跟着程序一起删，`user\` 留下。
    #[test]
    fn the_bridge_config_goes_with_the_program_and_the_data_stays() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"token").unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.json", b"template"),
            ("sleepy-doll.exe", b"exe"),
        ]);

        let leftovers = remove_files(&archive, &target, false, None);

        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert!(!target.join("bridge").exists(), "程序文件不该剩下");
        assert_eq!(
            fs::read(target.join("user/config.json")).unwrap(),
            b"secrets"
        );
    }

    #[test]
    fn user_data_goes_away_only_when_asked() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        remove_files(&archive, &target, true, None);

        assert!(!target.join("user").exists());
    }

    /// 勾了连数据一起删时，桥的配置也走。
    #[test]
    fn user_data_takes_the_bridge_config_with_it() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"token").unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        // 配置跟着桥组件走，组件在 `bridge\` 下它就也在那儿。
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.json", b"template"),
            ("sleepy-doll.exe", b"exe"),
        ]);

        remove_files(&archive, &target, true, None);

        assert!(!target.exists(), "目录里不该再剩下什么");
    }

    /// 载荷里的模板名与装出来的名字不同，卸载要按安装那套落点规则找回来。
    #[test]
    fn a_seeded_bridge_config_goes_away_with_the_user_data() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"token").unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.example.json", b"template"),
        ]);

        remove_files(&archive, &target, true, None);

        assert!(!target.exists(), "目录里不该再剩下什么");
    }

    /// 旧版把技能装在安装根下，那份不在清单里。
    #[test]
    fn the_retired_root_skills_go_away_too() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("skills/bgi-assistant")).unwrap();
        fs::write(target.join("skills/bgi-assistant/SKILL.md"), b"old").unwrap();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        remove_files(&archive, &target, true, None);

        assert!(!target.exists(), "目录里不该再剩下什么");
    }

    #[test]
    fn leftover_bridge_user_goes_away_with_user_data() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge/user")).unwrap();
        fs::write(target.join("bridge/user/log.txt"), b"log").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        remove_files(&archive, &target, true, None);

        assert!(!target.join("bridge/user").exists());
    }

    /// 勾了连数据一起删时，漫游目录里那份用户数据也要走。
    #[test]
    fn the_roaming_fallback_goes_away_with_the_user_data() {
        let (_root, target) = target();
        fs::write(target.join("sleepy-doll.exe"), b"exe").unwrap();
        let (_roaming, product) = fallback();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        remove_files(&archive, &target, true, Some(&product));

        assert!(!product.exists(), "漫游目录里那份也该走");
    }

    #[test]
    fn the_roaming_fallback_survives_when_the_user_data_does() {
        let (_root, target) = target();
        let (_roaming, product) = fallback();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        remove_files(&archive, &target, false, Some(&product));

        assert!(product.join("user/config.json").is_file(), "没勾就不动它");
    }

    /// 一份漫游目录下的用户数据，形状与 `%APPDATA%\Sleepy Doll` 一致。
    fn fallback() -> (tempfile::TempDir, PathBuf) {
        let roaming = tempfile::tempdir().unwrap();
        let product = roaming.path().join("Sleepy Doll");
        fs::create_dir_all(product.join("user")).unwrap();
        fs::write(product.join("user/config.json"), b"secrets").unwrap();
        (roaming, product)
    }

    #[test]
    fn missing_files_are_not_an_error() {
        let (_root, target) = target();
        let archive = archive(&[("sleepy-doll.exe", b"exe"), ("skills/a.md", b"skill")]);

        let leftovers = remove_files(&archive, &target, false, None);

        assert!(leftovers.is_empty(), "{leftovers:?}");
        assert!(!target.exists());
    }

    /// 一个文件删不掉，其余的照样清。
    #[cfg(windows)]
    #[test]
    fn a_stuck_file_does_not_stop_the_rest() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("sleepy-doll.exe"), b"exe").unwrap();
        fs::write(target.join("bridge/BgiBridge.dll"), b"dll").unwrap();
        let archive = archive(&[
            ("sleepy-doll.exe", b"exe"),
            ("bridge/BgiBridge.dll", b"dll"),
        ]);
        let stuck = target.join("bridge/BgiBridge.dll");
        let held = hold_exclusively(&stuck);

        let leftovers = remove_files(&archive, &target, false, None);
        close(held);

        assert_eq!(leftovers, vec![stuck.clone()], "只剩被占住的那一个");
        assert!(stuck.is_file());
        assert!(!target.join("sleepy-doll.exe").exists(), "别的照样删掉");
    }

    /// 程序还开着的时候整个卸载都不动手。
    #[cfg(windows)]
    #[test]
    fn a_running_program_stops_the_uninstall_before_it_starts() {
        let (_root, target) = target();
        fs::write(target.join("sleepy-doll.exe"), b"exe").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);
        let held = hold_exclusively(&target.join("sleepy-doll.exe"));

        let busy = busy_program(&removals(&archive, &target)).cloned();
        close(held);

        assert_eq!(busy, Some(target.join("sleepy-doll.exe")));
    }

    /// 用户数据被占用不算程序还开着。
    #[cfg(windows)]
    #[test]
    fn a_held_data_file_is_not_mistaken_for_a_running_program() {
        let (_root, target) = target();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("user/sleepy-doll.db"), b"db").unwrap();
        let archive = archive(&[("user/sleepy-doll.db", b"template")]);
        let held = hold_exclusively(&target.join("user/sleepy-doll.db"));

        let busy = busy_program(&removals(&archive, &target)).cloned();
        close(held);

        assert_eq!(busy, None);
    }

    /// 卸载入口与空掉的安装目录都归退出后的清理。
    #[cfg(windows)]
    #[test]
    fn the_uninstaller_deletes_itself_and_the_emptied_directory() {
        let (_root, target) = target();
        fs::write(target.join(UNINSTALLER), b"setup copy").unwrap();

        cleanup_after_exit(&[
            Removal::File(target.join(UNINSTALLER)),
            Removal::Empty(target.clone()),
        ]);

        assert!(
            wait_until_gone(&target.join(UNINSTALLER)),
            "卸载入口该自己消失"
        );
        assert!(!target.exists(), "空掉的安装目录也该消失");
    }

    /// 卸载程序退出前删不掉自己：这里替它占住那个文件，两秒后放开。
    #[cfg(windows)]
    #[test]
    fn a_locked_uninstaller_is_deleted_once_it_is_released() {
        let (_root, target) = target();
        let uninstaller = target.join(UNINSTALLER);
        fs::write(&uninstaller, b"setup copy").unwrap();
        // 句柄是个裸指针，进不了别的线程，转成整数带过去。
        let held = hold_exclusively(&uninstaller) as usize;
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            close(held as _);
        });

        cleanup_after_exit(&[
            Removal::File(uninstaller.clone()),
            Removal::Empty(target.clone()),
        ]);

        assert!(wait_until_gone(&uninstaller), "放开之后该被删掉");
        release.join().unwrap();
    }

    /// 独占打开一个文件，模拟运行中的程序。
    #[cfg(windows)]
    fn hold_exclusively(path: &Path) -> windows_sys::Win32::Foundation::HANDLE {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, OPEN_EXISTING,
        };

        const GENERIC_READ: u32 = 0x8000_0000;
        let path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(handle, windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE);
        handle
    }

    #[cfg(windows)]
    fn close(handle: windows_sys::Win32::Foundation::HANDLE) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
    }

    /// 等一个文件消失。删除是另一个进程干的，只能轮询。
    #[cfg(windows)]
    fn wait_until_gone(path: &Path) -> bool {
        for _ in 0..300 {
            if !path.exists() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        false
    }
}
