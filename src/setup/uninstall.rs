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
    super::validate_directory(&directory.to_string_lossy())?;
    if !directory.file_name().is_some_and(|name| {
        name.eq_ignore_ascii_case(super::NAME) || name.eq_ignore_ascii_case(super::SLUG)
    }) {
        return Err(Error::message("卸载目录不属于 Sleepy Doll，未删除任何文件"));
    }
    progress(0.0, "正在移除文件…");
    // 运行中的程序与 BetterGI 加载的桥都锁着自己的文件，动手之前先查一遍。
    let mut programs = removals(archive, directory);
    cached_programs(&directory.join("bridge-cache"), &mut programs);
    if let Some(busy) = busy_program(&programs) {
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
    cleanup_after_exit(&cleanup_plan(directory, remove_user_data))?;
    progress(1.0, "卸载完成");
    Ok(())
}

pub(super) fn cleanup_plan(directory: &Path, remove_user_data: bool) -> Vec<Removal> {
    let keeps_local_data = !remove_user_data
        && (directory.join(DATA_DIRECTORY).exists()
            || directory.join("bridge").join(DATA_DIRECTORY).exists());
    if !keeps_local_data {
        return vec![Removal::Tree(directory.to_path_buf())];
    }
    // Explicitly retained user data keeps its location; runtime caches never do.
    vec![
        Removal::File(directory.join(UNINSTALLER)),
        Removal::Tree(directory.join("bridge-cache")),
        Removal::Tree(directory.join(".cache")),
    ]
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
        if nested.is_dir() && fs::remove_dir_all(&nested).is_err() {
            leftovers.push(nested);
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn uninstall_cleanup_preflight_detects_loaded_cached_bridge() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/.tmp/uninstall-verification/cached-lock");
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
        drop(lock);
        assert!(busy_program(&programs).is_none());
    }

    #[test]
    fn uninstall_cleanup_rejects_unrelated_installation_roots_before_deleting() {
        let archive = Archive::new(b"[]", vec![]).unwrap();
        for path in ["D:\\", "D:\\BetterGI", "C:\\Windows"] {
            assert!(uninstall(&archive, Path::new(path), true, &mut |_, _| {}).is_err());
        }
    }
}
