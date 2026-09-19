//! 安装引擎：目录规则、载荷解包、外壳集成与卸载。不含界面，进度通过回调交出去。

pub mod install;
pub mod payload;
pub mod uninstall;

mod cleanup;
mod registry;
mod shell;

use std::path::{Path, PathBuf};

pub use cleanup::{Removal, cleanup_after_exit};
pub use install::install;
pub use payload::Archive;
pub use shell::{browse_for_directory, launch};
pub use uninstall::uninstall;

/// 产品名。安装目录、快捷方式与注册表项都用它。
pub const NAME: &str = "Sleepy Doll";
/// 目录名里不留空格的写法。
pub const SLUG: &str = "Sleepy-Doll";
/// 主程序文件名。
pub const EXECUTABLE: &str = "sleepy-doll.exe";
/// 安装时复制出来的卸载入口。
pub const UNINSTALLER: &str = "uninstall.exe";
/// 用户数据目录，装在安装根下。
pub(crate) const DATA_DIRECTORY: &str = "user";
/// Windows 的路径上限。
const MAX_PATH: usize = 260;

/// 进度回调：0..1 的比例与当前动作。
pub type Progress<'a> = &'a mut dyn FnMut(f64, &str);

/// 注册表里的 DisplayVersion 与它一致，取自 Cargo.toml。
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error("{context}：{source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}

/// 给 io 错误补上「在做什么」。
pub(crate) fn failed(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Error {
    move |source| Error::Io {
        context: context.into(),
        source,
    }
}

/// 默认安装位置：D: 是固定磁盘就装过去，否则装进当前用户的目录。
pub fn default_directory() -> String {
    if let Some(drive) = fixed_drive() {
        return resolve_directory(&drive);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    normalize(&base.join("Programs").join(NAME).to_string_lossy())
}

/// 统一分隔符并去掉结尾的反斜杠。盘符根与 UNC 共享根的结尾反斜杠保留，大小写不动。
pub fn normalize(directory: &str) -> String {
    let text = directory.replace('/', "\\");
    let trimmed = text.trim_end_matches('\\');
    if trimmed.len() != text.len() && is_root(trimmed) {
        format!("{trimmed}\\")
    } else {
        trimmed.to_string()
    }
}

/// `D:` 与 `\\srv\share` 这两种根：后面必须带一个反斜杠才表示根目录。
fn is_root(path: &str) -> bool {
    if path.len() == 2 {
        let mut characters = path.chars();
        return characters.next().is_some_and(|c| c.is_ascii_alphabetic())
            && characters.next() == Some(':');
    }
    // UNC：去掉开头的两条反斜杠之后正好剩「服务器\共享」两段。
    path.strip_prefix("\\\\").is_some_and(|rest| {
        let mut parts = rest.split('\\');
        parts.next().is_some_and(|part| !part.is_empty())
            && parts.next().is_some_and(|part| !part.is_empty())
            && parts.next().is_none()
    })
}

/// 归一化之后，结尾不是产品目录名就补一层。只看最后一段，不会二次追加。
pub fn resolve_directory(requested: &str) -> String {
    let normalized = normalize(requested);
    if normalized.is_empty() || is_product_name(&normalized) {
        return normalized;
    }
    if normalized.ends_with('\\') {
        format!("{normalized}{NAME}")
    } else {
        format!("{normalized}\\{NAME}")
    }
}

/// 最后一段是不是产品名，大小写不敏感。
fn is_product_name(directory: &str) -> bool {
    directory
        .rsplit('\\')
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(NAME) || name.eq_ignore_ascii_case(SLUG))
}

/// 校验归一化并追加之后的路径，而不是用户的原始输入。系统目录一律拒绝。
pub fn validate_directory(directory: &str) -> Result<(), Error> {
    if directory.is_empty() || !is_absolute(directory) {
        return Err(Error::message(
            "请选择一个安装位置。空路径和相对路径会装到安装程序当前所在的目录里。",
        ));
    }
    if let Some(root) = system_directories()
        .into_iter()
        .find(|root| is_same_or_under(directory, root))
    {
        return Err(Error::message(format!(
            "{directory} 在 {root} 之下，不能装到系统目录里：Sleepy Doll 把配置与会话数据放在安装位置旁边的 user 目录里，那里普通程序写不进去。请换一个位置。"
        )));
    }
    let length = directory.chars().count();
    if length > MAX_PATH {
        return Err(Error::message(format!(
            "{directory} 有 {length} 个字符，超过 Windows 的 {MAX_PATH} 个字符上限。请换一个短一些的位置。"
        )));
    }
    Ok(())
}

/// 目录里有别的东西又没有主程序，装进去之前要问用户一次。只有 `user\` 时不算。
pub fn is_foreign_directory(directory: &Path) -> bool {
    if directory.join(EXECUTABLE).is_file() || directory.join(DATA_DIRECTORY).is_dir() {
        return false;
    }
    std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some())
}

/// 已安装的位置与版本，取自注册表。
pub fn installed() -> Option<Installed> {
    let directory = registry::install_location()?;
    Some(Installed {
        directory,
        version: registry::installed_version(),
    })
}

#[derive(Debug, Clone)]
pub struct Installed {
    pub directory: PathBuf,
    pub version: Option<String>,
}

/// 卸载对象：注册表登记的安装位置优先，取不到才退回当前程序所在目录。
pub fn uninstall_directory() -> Option<PathBuf> {
    registry::install_location().or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|executable| executable.parent().map(Path::to_path_buf))
    })
}

/// 卸载入口是安装时复制出来的 `uninstall.exe`；带 `--uninstall` 也按卸载处理。
pub fn is_uninstall_mode() -> bool {
    if std::env::args_os().any(|argument| argument == "--uninstall") {
        return true;
    }
    std::env::current_exe().is_ok_and(|executable| {
        executable
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(UNINSTALLER))
    })
}

/// 清单里表示「桥配置」的路径。放在根下还是 `bridge\` 下、叫不叫 example 都装成同一份文件。
pub(crate) fn is_bridge_config(path: &str) -> bool {
    matches!(
        path,
        "bridge.config.json"
            | "bridge.config.example.json"
            | "bridge/bridge.config.json"
            | "bridge/bridge.config.example.json"
    )
}

/// 清掉旧版留在安装根下的 `skills\`。载荷自己还带 `skills\` 时不动。
pub(crate) fn remove_retired_skills(archive: &Archive, directory: &Path) {
    if archive
        .entries()
        .iter()
        .any(|entry| entry.path.starts_with("skills/"))
    {
        return;
    }
    let retired = directory.join("skills");
    if retired.is_dir() {
        let _ = std::fs::remove_dir_all(retired);
    }
}

/// `D:\Games`、`D:\`、`\\srv\share` 算绝对路径。
fn is_absolute(directory: &str) -> bool {
    let normalized = normalize(directory);
    if normalized.len() >= 2 {
        let mut characters = normalized.chars();
        if characters.next().is_some_and(|c| c.is_ascii_alphabetic())
            && characters.next() == Some(':')
        {
            return normalized.len() == 2 || normalized.as_bytes().get(2) == Some(&b'\\');
        }
    }
    normalized.starts_with("\\\\")
}

/// 不允许写入的目录根：Windows 目录、System32，以及两个 Program Files 与各自的
/// Common Files。
fn system_directories() -> Vec<String> {
    let mut roots = Vec::new();
    for name in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(base) = std::env::var_os(name) else {
            continue;
        };
        let base = base.to_string_lossy().into_owned();
        roots.push(format!("{base}\\Common Files"));
        roots.push(base);
    }
    if let Some(windows) = std::env::var_os("SystemRoot") {
        let windows = windows.to_string_lossy().into_owned();
        roots.push(format!("{windows}\\System32"));
        roots.push(windows);
    }
    roots
}

/// 按路径分段比较：`C:\Program Files Extra` 不在 `C:\Program Files` 之下。
fn is_same_or_under(directory: &str, root: &str) -> bool {
    let directory = normalize(directory).to_ascii_lowercase();
    let root = normalize(root).to_ascii_lowercase();
    let root = root.trim_end_matches('\\');
    !root.is_empty() && (directory == root || directory.starts_with(&format!("{root}\\")))
}

/// D: 存在且是固定磁盘时给出 `D:\`。
#[cfg(windows)]
fn fixed_drive() -> Option<String> {
    // 与 GetDriveTypeW 的 DRIVE_FIXED 对应。
    const DRIVE_FIXED: u32 = 3;
    let root = "D:\\";
    if !Path::new(root).is_dir() {
        return None;
    }
    let wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
    let kind = unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr()) };
    (kind == DRIVE_FIXED).then(|| root.to_string())
}

#[cfg(not(windows))]
fn fixed_drive() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_product_name_is_appended_once() {
        let cases = [
            ("E:\\Games", "E:\\Games\\Sleepy Doll"),
            ("E:\\Games\\", "E:\\Games\\Sleepy Doll"),
            ("E:\\Games\\\\", "E:\\Games\\Sleepy Doll"),
            ("E:\\Games/Mixed", "E:\\Games\\Mixed\\Sleepy Doll"),
            ("D:\\", "D:\\Sleepy Doll"),
            ("\\\\srv\\share", "\\\\srv\\share\\Sleepy Doll"),
            ("", ""),
        ];
        for (requested, expected) in cases {
            assert_eq!(resolve_directory(requested), expected, "{requested}");
        }
    }

    #[test]
    fn a_directory_that_already_names_the_product_is_left_alone() {
        for requested in [
            "E:\\Games\\Sleepy Doll",
            "E:\\Games\\sleepy doll\\",
            "E:\\Games\\Sleepy-Doll",
            "E:\\Games\\SLEEPY-DOLL",
        ] {
            let expected = requested.trim_end_matches('\\');
            assert_eq!(resolve_directory(requested), expected, "{requested}");
        }
    }

    #[test]
    fn separators_are_normalised_without_touching_case() {
        assert_eq!(normalize("E:/Games/"), "E:\\Games");
        assert_eq!(
            normalize("E:\\Games\\Sleepy Doll\\"),
            "E:\\Games\\Sleepy Doll"
        );
        assert_eq!(normalize("D:\\"), "D:\\");
        assert_eq!(normalize("\\\\srv\\share\\"), "\\\\srv\\share\\");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn every_system_root_covers_its_whole_tree() {
        let roots = system_directories();
        assert_eq!(roots.len(), 6, "{roots:?}");
        for root in roots {
            assert!(is_same_or_under(&root, &root), "{root}");
            assert!(
                is_same_or_under(&format!("{root}\\Sleepy Doll"), &root),
                "{root}"
            );
            assert!(
                validate_directory(&resolve_directory(&root)).is_err(),
                "{root}"
            );
        }
    }

    #[test]
    fn lookalike_directories_are_not_system_ones() {
        for directory in ["C:\\Program Files Extra", "C:\\WindowsApps"] {
            assert!(
                !system_directories()
                    .iter()
                    .any(|root| is_same_or_under(directory, root)),
                "{directory}"
            );
        }
    }

    #[test]
    fn empty_and_relative_paths_are_refused() {
        for requested in ["", "Sleepy Doll", "Sleepy-Doll", "Games"] {
            assert!(
                validate_directory(&resolve_directory(requested)).is_err(),
                "{requested}"
            );
        }
        assert!(validate_directory(&resolve_directory("E:\\Games")).is_ok());
        assert!(validate_directory(&resolve_directory("D:\\")).is_ok());
    }

    #[test]
    fn overlong_paths_are_refused() {
        let long = format!("E:\\{}", "a".repeat(MAX_PATH));
        assert!(validate_directory(&resolve_directory(&long)).is_err());
        assert!(validate_directory(&resolve_directory("E:\\Games")).is_ok());
    }

    #[test]
    fn only_a_directory_with_other_things_in_it_is_foreign() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        // 目录不存在
        assert!(!is_foreign_directory(&target));
        std::fs::create_dir_all(&target).unwrap();
        // 空目录
        assert!(!is_foreign_directory(&target));
        std::fs::write(target.join("other.txt"), b"x").unwrap();
        // 非空但主程序在，视为覆盖安装
        std::fs::write(target.join(EXECUTABLE), b"exe").unwrap();
        assert!(!is_foreign_directory(&target));
        std::fs::remove_file(target.join(EXECUTABLE)).unwrap();
        assert!(is_foreign_directory(&target));
        // 只剩 `user\`：仍按自己的目录处理。
        std::fs::remove_file(target.join("other.txt")).unwrap();
        std::fs::create_dir_all(target.join(DATA_DIRECTORY)).unwrap();
        assert!(!is_foreign_directory(&target));
    }
}
