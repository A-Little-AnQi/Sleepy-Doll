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
