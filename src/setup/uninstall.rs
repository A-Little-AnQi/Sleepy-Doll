//! 卸载：按安装时写入的清单删文件，再清快捷方式与注册表，最后安排自删除。

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::setup::{
    Error, Progress, UNINSTALLER, failed, is_bridge_config, payload::Archive, shell,
};

/// 用户自己的数据目录。安装时不创建，卸载时也要用户点了才删。
const USER_DIRECTORY: &str = "user";

/// 从 `directory` 卸载。
///
/// 按载荷清单删除，删不掉目录里别人的东西；`bridge.config.json` 与 `user\` 默认保留：
/// 前者的 token 也被程序写进了 `user\config.json`，删了下次装上会对不上。
pub fn uninstall(
    archive: &Archive,
    directory: &Path,
    remove_user_data: bool,
    progress: Progress,
) -> Result<(), Error> {
    progress(0.0, "正在移除文件…");
    let mut removed = Vec::new();
    for (path, _) in archive.files() {
        if is_bridge_config(path) {
            continue;
        }
        let target = directory.join(super::install::relative(path));
        match fs::remove_file(&target) {
            Ok(()) => removed.push(target),
            // 已经不在了就当删过了：用户可能自己清过安装目录。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(failed(format!("无法删除 {}", target.display()))(error));
            }
        }
    }
    remove_empty_directories(&removed);

    progress(0.6, "正在删除快捷方式…");
    remove_shortcuts();

    progress(0.8, "正在清理注册表…");
    super::registry::remove()?;

    if remove_user_data {
        progress(0.9, "正在删除用户数据…");
        let user = directory.join(USER_DIRECTORY);
        // 只删这一棵树。删不掉（文件被占用、权限不够）就留着，不必因此中断卸载。
        if user.is_dir() {
            let _ = fs::remove_dir_all(&user);
        }
    }

    progress(0.95, "正在收尾…");
    schedule_self_delete();
    // 上面删完文件后目录可能空了；留着也没用，但里面还有别人的东西就必须留着。
    let _ = fs::remove_dir(directory);
    progress(1.0, "卸载完成");
    Ok(())
}

/// 删掉因文件移除而空掉的目录。`remove_dir` 只删得掉空目录，别人的东西不会被带走。
fn remove_empty_directories(removed: &[PathBuf]) {
    let mut directories: Vec<&Path> = removed.iter().filter_map(|path| path.parent()).collect();
    // 深的先删：父目录要等子目录空了才删得掉。
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();
    for directory in directories {
        let _ = fs::remove_dir(directory);
    }
}

/// 开始菜单与桌面上的快捷方式。两处都按同一个文件名找。
fn remove_shortcuts() {
    let name = super::install::shortcut_name();
    for directory in [shell::programs_directory(), shell::desktop_directory()]
        .into_iter()
        .flatten()
    {
        let _ = fs::remove_file(directory.join(&name));
    }
}

/// 卸载程序自己删不掉自己（进程还占着文件），交给系统在下次重启时删。
/// 这个标志在部分系统上要求管理员权限，失败时安装目录里会留下 uninstall.exe。
fn schedule_self_delete() {
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    if !executable
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(UNINSTALLER))
    {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};

        let path: Vec<u16> = executable
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // 目标为 NULL：只登记删除，不搬家。
        unsafe { MoveFileExW(path.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn installed_files_and_their_directories_go_away() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("sleepy-doll.exe"), b"exe").unwrap();
        fs::write(target.join("bridge/BgiBridge.dll"), b"dll").unwrap();
        let archive = archive(&[
            ("sleepy-doll.exe", b"exe"),
            ("bridge/BgiBridge.dll", b"dll"),
        ]);

        uninstall(&archive, &target, false, &mut |_, _| {}).unwrap();

        assert!(!target.join("sleepy-doll.exe").exists());
        assert!(!target.join("bridge").exists());
    }

    #[test]
    fn bridge_config_and_user_data_survive() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"token").unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        let archive = archive(&[
            ("bridge/bridge.config.json", b"template"),
            ("sleepy-doll.exe", b"exe"),
        ]);

        uninstall(&archive, &target, false, &mut |_, _| {}).unwrap();

        assert_eq!(
            fs::read(target.join("bridge/bridge.config.json")).unwrap(),
            b"token"
        );
        assert_eq!(
            fs::read(target.join("user/config.json")).unwrap(),
            b"secrets"
        );
    }

    #[test]
    fn user_data_goes_away_only_when_asked() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("user")).unwrap();
        fs::write(target.join("user/config.json"), b"secrets").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe")]);

        uninstall(&archive, &target, true, &mut |_, _| {}).unwrap();

        assert!(!target.join("user").exists());
    }

    #[test]
    fn missing_files_are_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(&target).unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"exe"), ("skills/a.md", b"skill")]);

        uninstall(&archive, &target, false, &mut |_, _| {}).unwrap();

        assert!(!target.exists());
    }
}
