//! 安装：先把载荷解包到临时目录，再逐项写入安装目录，最后做外壳集成。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use crate::setup::{
    DATA_DIRECTORY, EXECUTABLE, Error, NAME, Progress, UNINSTALLER, failed, is_bridge_config,
    payload::Archive, registry, shell,
};
use serde_json::{Value, json};

/// 桥组件里定位布局用的那一个：它在哪，桥的配置就该在哪。
const BRIDGE_INJECTOR: &str = "BgiBridge.Injector.exe";

/// 把载荷装到 `directory`。失败时删掉本次新建的文件，已被覆盖的只能在错误信息里列出。
pub fn install(
    archive: &Archive,
    directory: &Path,
    desktop_shortcut: bool,
    progress: Progress,
) -> Result<(), Error> {
    progress(0.0, "正在准备安装目录…");
    let staging = Staging::create()?;
    extract(&staging.0, archive)?;

    progress(0.5, "正在写入文件…");
    let mut written = Written::new(directory);
    fs::create_dir_all(directory).map_err(failed(format!("无法创建 {}", directory.display())))?;
    migrate(archive, directory)?;
    let outcome =
        propagate(&staging.0, directory, archive, &mut written, progress).and_then(|()| {
            pin_data_root(directory, archive)?;
            progress(0.9, "正在创建快捷方式…");
            integrate(directory, desktop_shortcut)
        });
    if let Err(error) = outcome {
        written.rollback();
        return Err(written.describe(error));
    }

    progress(1.0, "安装完成");
    Ok(())
}

/// 解包的临时目录，安装结束后删掉。
struct Staging(PathBuf);

impl Staging {
    fn create() -> Result<Self, Error> {
        // 同一进程里可以起多次安装，路径必须每次都不同。
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let name = format!(
            "sleepy-doll-setup-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        // 进程号会被复用，同名目录可能是上次崩溃留下的。
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path)
            .map_err(failed(format!("无法创建临时目录 {}", path.display())))?;
        Ok(Self(path))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 本次写入的文件。回滚只撤得掉新建的那些。
struct Written {
    /// 安装目录。回滚往上删到它为止。
    root: PathBuf,
    /// 安装目录原本就存在时不能删。
    root_created: bool,
    created: Vec<PathBuf>,
    replaced: Vec<PathBuf>,
}

impl Written {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            root_created: !root.is_dir(),
            created: Vec::new(),
            replaced: Vec::new(),
        }
    }

    /// 删掉本次新建的文件与随之空掉的目录。
    fn rollback(&self) {
        for path in self.created.iter().rev() {
            let _ = fs::remove_file(path);
            self.prune(path);
        }
    }

    /// 从 `path` 所在目录往上删空目录。
    fn prune(&self, path: &Path) {
        let mut directory = path.parent();
        while let Some(current) = directory {
            if current == self.root && !self.root_created {
                return;
            }
            if fs::remove_dir(current).is_err() {
                return;
            }
            if current == self.root {
                return;
            }
            directory = current.parent();
        }
    }

    /// 把写入痕迹并进错误信息。
    fn describe(&self, error: Error) -> Error {
        if self.created.is_empty() && self.replaced.is_empty() {
            return error;
        }
        let mut text = format!("{error}。新建的 {} 个文件已经删除", self.created.len());
        if !self.replaced.is_empty() {
            let listing = self
                .replaced
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("、");
            text.push_str(&format!("；此前就有、已被覆盖的文件：{listing}"));
        }
        Error::message(text)
    }
}

/// 旧版把桥的文件平铺在根下、技能放在根下的 `skills\`，装载新布局的包时把它们归位。
/// 载荷自己还是旧布局时不动。
fn migrate(archive: &Archive, directory: &Path) -> Result<(), Error> {
    super::remove_retired_skills(archive, directory);
    if bridge_directory(archive).is_none() {
        return Ok(());
    }
    let flat = directory.join("bridge.config.json");
    let nested = directory.join(bridge_config_path(archive));
    // 先搬到 `bridge\`，下面的写入会跳过它。
    if flat.is_file() && !nested.is_file() {
        if let Some(parent) = nested.parent() {
            fs::create_dir_all(parent).map_err(failed(format!("无法创建 {}", parent.display())))?;
        }
        fs::rename(&flat, &nested).map_err(failed(format!("无法移动 {}", flat.display())))?;
    }

    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("bgibridge.") {
            // 新包会把桥重新写到 `bridge\` 下；旧文件删不掉就留着。
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(())
}

/// 桥组件在载荷里的目录。返回 `None` 表示平铺在安装目录根下。
fn bridge_directory(archive: &Archive) -> Option<&str> {
    archive.entries().iter().find_map(|entry| {
        let (directory, name) = entry.path.rsplit_once('/')?;
        (name == BRIDGE_INJECTOR).then_some(directory)
    })
}

/// 桥配置在安装目录里的相对位置：桥组件在哪，它就在哪，文件名固定。
fn bridge_config_path(archive: &Archive) -> PathBuf {
    match bridge_directory(archive) {
        Some(directory) => relative(directory).join("bridge.config.json"),
        None => PathBuf::from("bridge.config.json"),
    }
}

/// 清单里的路径在安装目录里的落点。桥配置的模板名一律归到 `bridge_config_path`。
pub(super) fn destination(directory: &Path, archive: &Archive, path: &str) -> PathBuf {
    if is_bridge_config(path) {
        directory.join(bridge_config_path(archive))
    } else {
        directory.join(relative(path))
    }
}

/// 把载荷写进临时目录。这一步不碰安装目录。
fn extract(staging: &Path, archive: &Archive) -> Result<(), Error> {
    for (path, bytes) in archive.files() {
        let destination = staging.join(relative(path));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(failed(format!("无法创建 {}", parent.display())))?;
        }
        fs::write(&destination, bytes)
            .map_err(failed(format!("无法写入 {}", destination.display())))?;
    }
    Ok(())
}

/// 逐项复制到安装目录。
fn propagate(
    staging: &Path,
    directory: &Path,
    archive: &Archive,
    written: &mut Written,
    progress: Progress,
) -> Result<(), Error> {
    let files = archive.files();
    for (index, (path, _)) in files.iter().enumerate() {
        let destination = destination(directory, archive, path);
        // 覆盖安装保留已有的桥配置，不换成包里的模板。
        if is_bridge_config(path) && destination.is_file() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(failed(format!("无法创建 {}", parent.display())))?;
        }
        progress(
            0.5 + 0.4 * index as f64 / files.len() as f64,
            &format!("正在写入 {path}"),
        );
        let existed = destination.is_file();
        let outcome = fs::copy(staging.join(relative(path)), &destination)
            .map_err(failed(format!("无法写入 {}", destination.display())));
        if let Err(error) = outcome {
            written.prune(&destination);
            return Err(error);
        }
        if existed {
            written.replaced.push(destination);
        } else {
            written.created.push(destination);
        }
    }
    Ok(())
}

/// 数据根固定为安装根的 `user\`：配置里没有这个字段时，恢复工具和引导 DLL 会在
/// `bridge\user\` 另建一棵树。
fn pin_data_root(directory: &Path, archive: &Archive) -> Result<(), Error> {
    if bridge_directory(archive).is_none() {
        return Ok(());
    }
    let path = directory.join(bridge_config_path(archive));
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut settings) = serde_json::from_str::<Value>(&text) else {
        return Ok(());
    };
    if !settings.is_object() {
        return Ok(());
    }
    let wanted = directory.join(DATA_DIRECTORY).to_string_lossy().to_string();
    if settings.get("userDirectory").and_then(Value::as_str) == Some(wanted.as_str()) {
        return Ok(());
    }
    settings["userDirectory"] = json!(wanted);
    crate::config::atomic_write(&path, &settings)
        .map_err(|error| Error::message(format!("无法写入 {}：{error}", path.display())))
}

/// 安装目录之外的集成：卸载入口、快捷方式、注册表登记。
fn integrate(directory: &Path, desktop_shortcut: bool) -> Result<(), Error> {
    let executable = directory.join(EXECUTABLE);
    let uninstaller = directory.join(UNINSTALLER);
    // 卸载入口是安装程序自己的副本。
    let current = std::env::current_exe().map_err(failed("无法定位安装程序自身"))?;
    fs::copy(&current, &uninstaller)
        .map_err(failed(format!("无法写入 {}", uninstaller.display())))?;

    let programs = shell::programs_directory()
        .ok_or_else(|| Error::message("找不到开始菜单目录，无法创建快捷方式"))?;
    shell::create_shortcut(&programs.join(shortcut_name()), &executable, "")?;
    if desktop_shortcut && let Some(desktop) = shell::desktop_directory() {
        shell::create_shortcut(&desktop.join(shortcut_name()), &executable, "")?;
    }
    registry::register(directory)
}

/// 开始菜单与桌面上的快捷方式文件名。
pub fn shortcut_name() -> String {
    format!("{NAME}.lnk")
}

/// 清单里的 `/` 分隔路径转成本机路径。
pub(crate) fn relative(path: &str) -> PathBuf {
    path.split('/')
        .fold(PathBuf::new(), |path, part| path.join(part))
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
    fn files_land_in_the_target_directory() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        let archive = archive(&[("sleepy-doll.exe", b"exe"), ("skills/a.md", b"skill")]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();

        assert_eq!(fs::read(target.join("sleepy-doll.exe")).unwrap(), b"exe");
        assert_eq!(fs::read(target.join("skills/a.md")).unwrap(), b"skill");
        assert_eq!(written.created.len(), 2);
        assert!(written.replaced.is_empty());
    }

    #[test]
    fn bridge_config_is_kept_when_the_target_already_has_one() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"token").unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.json", b"template"),
        ]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();

        assert_eq!(
            fs::read(target.join("bridge/bridge.config.json")).unwrap(),
            b"token"
        );
        assert_eq!(written.created.len(), 1);
    }

    #[test]
    fn packaged_layout_pins_the_install_user_directory() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            (
                "bridge/bridge.config.json",
                br#"{"enabled":true,"token":""}"#,
            ),
        ]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();
        pin_data_root(&target, &archive).unwrap();

        let settings: Value =
            serde_json::from_slice(&fs::read(target.join("bridge/bridge.config.json")).unwrap())
                .unwrap();
        assert_eq!(
            settings["userDirectory"].as_str(),
            Some(target.join("user").to_string_lossy().as_ref())
        );
    }

    #[test]
    fn kept_token_configs_also_get_the_data_root() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(
            target.join("bridge/bridge.config.json"),
            br#"{"token":"keep"}"#,
        )
        .unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.json", br#"{"token":"template"}"#),
        ]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();
        pin_data_root(&target, &archive).unwrap();

        let settings: Value =
            serde_json::from_slice(&fs::read(target.join("bridge/bridge.config.json")).unwrap())
                .unwrap();
        assert_eq!(settings["token"], "keep");
        assert_eq!(
            settings["userDirectory"].as_str(),
            Some(target.join("user").to_string_lossy().as_ref())
        );
    }

    #[test]
    fn a_config_template_is_seeded_into_the_bridge_directory() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        // 包里有组件和模板，没有真正的配置。
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.example.json", b"template"),
        ]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();

        assert_eq!(
            fs::read(target.join("bridge/bridge.config.json")).unwrap(),
            b"template"
        );
        assert!(!target.join("bridge/bridge.config.example.json").exists());
    }

    #[test]
    fn the_old_flat_layout_is_moved_into_place() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("bridge.config.json"), b"token").unwrap();
        for name in ["BgiBridge.dll", "BgiBridge.deps.json"] {
            fs::write(target.join(name), b"old").unwrap();
        }
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("bridge/bridge.config.json", b"template"),
        ]);

        migrate(&archive, &target).unwrap();

        assert_eq!(
            fs::read(target.join("bridge/bridge.config.json")).unwrap(),
            b"token"
        );
        assert!(!target.join("bridge.config.json").exists());
        assert!(!target.join("BgiBridge.dll").exists());
        assert!(!target.join("BgiBridge.deps.json").exists());
    }

    #[test]
    fn the_retired_root_skills_are_dropped() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("skills/bgi-assistant")).unwrap();
        fs::write(target.join("skills/bgi-assistant/SKILL.md"), b"old").unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("plugins/bgi/skills/bgi-assistant/SKILL.md", b"new"),
        ]);

        migrate(&archive, &target).unwrap();

        assert!(!target.join("skills").exists());
    }

    #[test]
    fn a_payload_that_still_ships_root_skills_keeps_them() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("skills")).unwrap();
        fs::write(target.join("skills/a.md"), b"old").unwrap();
        let archive = archive(&[
            ("bridge/BgiBridge.Injector.exe", b"exe"),
            ("skills/a.md", b"skill"),
        ]);

        migrate(&archive, &target).unwrap();

        assert_eq!(fs::read(target.join("skills/a.md")).unwrap(), b"old");
    }

    #[test]
    fn migration_never_replaces_an_existing_bridge_config() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(target.join("bridge")).unwrap();
        fs::write(target.join("bridge.config.json"), b"flat").unwrap();
        fs::write(target.join("bridge/bridge.config.json"), b"nested").unwrap();
        let archive = archive(&[("bridge/BgiBridge.Injector.exe", b"exe")]);

        migrate(&archive, &target).unwrap();

        assert_eq!(
            fs::read(target.join("bridge/bridge.config.json")).unwrap(),
            b"nested"
        );
    }

    #[test]
    fn a_flat_payload_is_left_flat() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("bridge.config.json"), b"token").unwrap();
        fs::write(target.join("BgiBridge.dll"), b"old").unwrap();
        // 平铺的包配平铺的桥，配置留在组件旁边。
        let archive = archive(&[
            ("BgiBridge.Injector.exe", b"exe"),
            ("bridge.config.json", b"template"),
        ]);

        migrate(&archive, &target).unwrap();

        assert_eq!(
            fs::read(target.join("bridge.config.json")).unwrap(),
            b"token"
        );
        assert_eq!(fs::read(target.join("BgiBridge.dll")).unwrap(), b"old");
    }

    #[test]
    fn overwritten_files_are_not_rolled_back() {
        let root = tempfile::tempdir().unwrap();
        let staging = Staging::create().unwrap();
        let target = root.path().join("Sleepy Doll");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("sleepy-doll.exe"), b"old").unwrap();
        let archive = archive(&[("sleepy-doll.exe", b"new"), ("skills/a.md", b"skill")]);
        extract(&staging.0, &archive).unwrap();

        let mut written = Written::new(&target);
        propagate(&staging.0, &target, &archive, &mut written, &mut |_, _| {}).unwrap();
        written.rollback();

        assert_eq!(written.created.len(), 1);
        assert!(!target.join("skills").exists());
        assert!(target.is_dir(), "安装目录原本就有，回滚不该删掉它");
        assert_eq!(fs::read(target.join("sleepy-doll.exe")).unwrap(), b"new");
    }

    #[test]
    fn rollback_reports_what_it_could_not_undo() {
        let root = tempfile::tempdir().unwrap();
        let mut written = Written::new(root.path());
        written.created.push(root.path().join("bridge/x.dll"));
        written.replaced.push(root.path().join("sleepy-doll.exe"));

        let text = written.describe(Error::message("磁盘已满")).to_string();

        assert!(text.starts_with("磁盘已满"), "{text}");
        assert!(text.contains("新建的 1 个文件已经删除"), "{text}");
        assert!(text.contains("sleepy-doll.exe"), "{text}");
    }

    #[test]
    fn nested_manifest_paths_become_native_ones() {
        assert_eq!(
            relative("bridge/BgiBridge.dll"),
            PathBuf::from("bridge").join("BgiBridge.dll")
        );
    }
}
