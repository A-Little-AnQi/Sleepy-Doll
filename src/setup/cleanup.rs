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
