//! 本进程退出之后才删得掉的东西。

use std::path::PathBuf;

/// 退出后要删掉的一项。
#[derive(Debug)]
pub enum Removal {
    /// 一个文件。
    File(PathBuf),
    /// 一个目录树。
    Tree(PathBuf),
    /// 只在空的时候删得掉。
    Empty(PathBuf),
}

impl Removal {
    #[cfg(windows)]
    fn path(&self) -> &std::path::Path {
        match self {
            Self::File(path) | Self::Tree(path) | Self::Empty(path) => path,
        }
    }
}

/// 安排本进程退出后的删除。
///
/// `File` 与 `Tree` 都消失就提前收工，`Empty` 不参与这个判断。
pub fn cleanup_after_exit(removals: &[Removal]) {
    if removals.is_empty() {
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        // 不弹控制台窗口。
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("cmd")
            // 原样交给 cmd，不再包一层引号。
            .raw_arg(format!("/c {}", script(removals)))
            // 工作目录留在待删目录里会让 rmdir 失败。
            .current_dir(std::env::temp_dir())
            .stdin(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    #[cfg(not(windows))]
    let _ = removals;
}

/// 那条 cmd。`if` 写在每次尝试的最后：`&` 归 `if` 管。
#[cfg(windows)]
fn script(removals: &[Removal]) -> String {
    let commands = removals.iter().map(command).collect::<Vec<_>>().join(" & ");
    let done = removals
        .iter()
        .filter(|removal| !matches!(removal, Removal::Empty(_)))
        .map(|removal| format!("if not exist \"{}\" ", removal.path().display()))
        .collect::<String>();
    let body = if done.is_empty() {
        commands
    } else {
        let attempt =
            |wait: u32| format!("{commands} & ping -n {wait} 127.0.0.1 >nul & {done}exit");
        format!(
            "for /l %i in (1,1,60) do ({}) & for /l %i in (1,1,180) do ({})",
            attempt(2),
            attempt(11)
        )
    };
    format!("@echo off & {body}")
}

/// 单条删除命令。重定向写在每一条里：`a & b >nul` 只重定向 b。
#[cfg(windows)]
fn command(removal: &Removal) -> String {
    let path = removal.path().display();
    match removal {
        Removal::File(_) => format!("del /f /q \"{path}\" >nul 2>&1"),
        Removal::Tree(_) => format!("rmdir /s /q \"{path}\" >nul 2>&1"),
        Removal::Empty(_) => format!("rmdir \"{path}\" >nul 2>&1"),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    /// 完成判定只看卸载入口在不在。
    #[cfg(windows)]
    #[test]
    fn the_uninstaller_goes_with_the_emptied_directory() {
        let script = script(&[
            Removal::File("C:\\App\\uninstall.exe".into()),
            Removal::Empty("C:\\App".into()),
        ]);

        assert!(
            script.contains("del /f /q \"C:\\App\\uninstall.exe\" >nul 2>&1"),
            "{script}"
        );
        assert!(script.contains("rmdir \"C:\\App\" >nul 2>&1"), "{script}");
        assert!(
            script.contains("if not exist \"C:\\App\\uninstall.exe\" exit"),
            "{script}"
        );
        assert!(!script.contains("if not exist \"C:\\App\" "), "{script}");
    }

    /// 缓存目录连树一起删。
    #[cfg(windows)]
    #[test]
    fn a_cache_tree_is_removed_with_its_contents() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("sleepy-doll-setup-webview2");
        std::fs::create_dir_all(cache.join("EBWebView").join("Default")).unwrap();
        std::fs::write(
            cache.join("EBWebView").join("Default").join("Cookies"),
            b"cookie",
        )
        .unwrap();

        cleanup_after_exit(&[Removal::Tree(cache.clone())]);

        // 删除由另一个进程完成，只能轮询。
        let mut gone = false;
        for _ in 0..300 {
            if !cache.exists() {
                gone = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(gone, "缓存目录该被整棵删掉");
    }
}
