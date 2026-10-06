//! Cleanup is performed by a separate system process after this installer exits.
use std::path::PathBuf;

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", content = "path", rename_all = "camelCase")]
pub enum Removal {
    File(PathBuf),
    Tree(PathBuf),
    Empty(PathBuf),
}

impl Removal {
    fn path(&self) -> &std::path::Path {
        match self {
            Self::File(path) | Self::Tree(path) | Self::Empty(path) => path,
        }
    }
}

pub fn cleanup_after_exit(removals: &[Removal]) -> Result<(), super::Error> {
    if removals.is_empty() {
        return Ok(());
    }
    #[cfg(windows)]
    spawn_cleanup(removals, std::process::id())?;
    #[cfg(not(windows))]
    validate(removals)?;
    Ok(())
}

fn validate(removals: &[Removal]) -> Result<(), super::Error> {
    use std::path::Component;
    for removal in removals {
        let path = removal.path();
        if !path.is_absolute()
            || path.file_name().is_none()
            || path
                .components()
                .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
        {
            return Err(super::Error::message(format!(
                "拒绝清理不安全的路径：{}",
                path.display()
            )));
        }
        for ancestor in path.ancestors().skip(1) {
            let Ok(metadata) = std::fs::symlink_metadata(ancestor) else {
                continue;
            };
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err(super::Error::message(
                        "清理路径的上级目录是链接，未安排删除",
                    ));
                }
            }
            if metadata.file_type().is_symlink() {
                return Err(super::Error::message(
                    "清理路径的上级目录是链接，未安排删除",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn spawn_cleanup(
    removals: &[Removal],
    owner: u32,
) -> Result<std::process::Child, super::Error> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use std::os::windows::process::CommandExt;
    validate(removals)?;
    let targets = serde_json::to_vec(removals)
        .map_err(|error| super::Error::message(format!("无法生成清理清单：{error}")))?;
    let script = format!(
        "$ownerProcessId={owner}\n$removalsBase64='{}'\n{}",
        STANDARD.encode(targets),
        include_str!("cleanup.ps1")
    );
    let encoded = STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    if encoded.len() > 28_000 {
        return Err(super::Error::message("清理清单过长，未安排删除"));
    }
    let windows = std::env::var_os("SystemRoot")
        .ok_or_else(|| super::Error::message("找不到系统清理程序"))?;
    let windows = PathBuf::from(windows);
    std::process::Command::new(windows.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &encoded,
        ])
        .current_dir(windows)
        .stdin(std::process::Stdio::null())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(super::failed("无法启动卸载清理，请重试"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{Duration, Instant},
    };

    fn fixture(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/.tmp/uninstall-verification")
            .join(name);
        fs::create_dir_all(&root).unwrap();
        root
    }

    pub(super) fn wait(child: &mut std::process::Child) {
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "cleanup helper failed: {status}");
                return;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("cleanup helper timed out");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn uninstall_cleanup_removes_cache_trees_and_install_root_with_literal_paths() {
        let root = fixture("产品 %USERNAME% & (literal)");
        fs::create_dir_all(root.join(".cache/setup/webview2")).unwrap();
        fs::create_dir_all(root.join("bridge-cache/hash")).unwrap();
        fs::write(root.join("uninstall.exe"), b"fixture").unwrap();
        fs::write(root.join(".cache/setup/webview2/cache"), b"cache").unwrap();
        let dll = root.join("bridge-cache/hash/BgiBridge.dll");
        fs::write(&dll, b"cached bridge").unwrap();
        let mut perms = fs::metadata(&dll).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&dll, perms).unwrap();
        let mut child = spawn_cleanup(&[Removal::Tree(root.clone())], 0).unwrap();
        wait(&mut child);
        assert!(!root.exists());
    }

    #[test]
    fn uninstall_cleanup_waits_for_locked_file_and_removes_empty_root() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = fixture("locked-root");
        let path = root.join("uninstall.exe");
        fs::write(&path, b"fixture").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        let mut child = spawn_cleanup(
            &[Removal::File(path.clone()), Removal::Empty(root.clone())],
            0,
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(800));
        assert!(path.exists());
        assert!(child.try_wait().unwrap().is_none());
        drop(lock);
        wait(&mut child);
        assert!(!root.exists());
    }

    #[test]
    fn uninstall_cleanup_refuses_roots_and_relative_paths() {
        for path in ["D:\\", "C:\\", "relative", "D:\\parent\\..\\other"] {
            assert!(spawn_cleanup(&[Removal::Tree(PathBuf::from(path))], 0).is_err());
        }
    }

    #[test]
    fn uninstall_cleanup_waits_for_original_parent_exit() {
        use std::os::windows::process::CommandExt;
        let root = fixture("parent-exit");
        fs::write(root.join("uninstall.exe"), b"fixture").unwrap();
        let mut owner = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let mut child = spawn_cleanup(&[Removal::Tree(root.clone())], owner.id()).unwrap();
        std::thread::sleep(Duration::from_millis(800));
        let preserved = root.exists() && child.try_wait().unwrap().is_none();
        owner.kill().unwrap();
        owner.wait().unwrap();
        wait(&mut child);
        assert!(
            preserved,
            "cleanup ran while the uninstaller parent was still alive"
        );
        assert!(!root.exists());
    }

    #[test]
    fn uninstall_cleanup_does_not_follow_junctions() {
        use base64::{Engine, engine::general_purpose::STANDARD};
        use std::os::windows::process::CommandExt;
        let root = fixture("junction-install");
        let outside = fixture("junction-sentinel");
        fs::write(outside.join("keep.txt"), b"must survive").unwrap();
        let data = STANDARD.encode(
            serde_json::to_vec(&serde_json::json!({"link":root.join("linked"),"target":outside}))
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
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
            .creation_flags(0x08000000)
            .status()
            .unwrap();
        assert!(status.success());
        let mut child = spawn_cleanup(&[Removal::Tree(root.clone())], 0).unwrap();
        wait(&mut child);
        assert!(!root.exists());
        assert_eq!(fs::read(outside.join("keep.txt")).unwrap(), b"must survive");
    }

    #[test]
    fn uninstall_cleanup_reproduces_d_drive_cache_only_residue() {
        let root = fixture("cache-only-install");
        fs::create_dir_all(root.join(".cache/setup")).unwrap();
        fs::create_dir_all(root.join("bridge-cache/hash")).unwrap();
        fs::write(
            root.join("bridge-cache/hash/BgiBridge.dll"),
            b"runtime copy",
        )
        .unwrap();
        let archive = crate::setup::Archive::new(b"[]", vec![]).unwrap();
        assert!(crate::setup::uninstall::remove_files(&archive, &root, false, None).is_empty());
        let mut child =
            spawn_cleanup(&crate::setup::uninstall::cleanup_plan(&root, false), 0).unwrap();
        wait(&mut child);
        assert!(
            !root.exists(),
            "cache-only normal uninstall must remove the installation directory"
        );
    }

    #[test]
    fn uninstall_cleanup_preserves_selected_user_data_but_never_caches() {
        let root = fixture("keep-user-install");
        fs::create_dir_all(root.join("user")).unwrap();
        fs::create_dir_all(root.join(".cache/setup/webview2")).unwrap();
        fs::create_dir_all(root.join("bridge-cache/hash")).unwrap();
        fs::write(root.join("user/config.json"), b"preserved config").unwrap();
        fs::write(root.join("uninstall.exe"), b"fixture").unwrap();
        fs::write(root.join(".cache/setup/webview2/cache"), b"cache").unwrap();
        fs::write(
            root.join("bridge-cache/hash/BgiBridge.dll"),
            b"runtime copy",
        )
        .unwrap();
        let archive = crate::setup::Archive::new(b"[]", vec![]).unwrap();
        assert!(crate::setup::uninstall::remove_files(&archive, &root, false, None).is_empty());
        let mut child =
            spawn_cleanup(&crate::setup::uninstall::cleanup_plan(&root, false), 0).unwrap();
        wait(&mut child);
        assert_eq!(
            fs::read(root.join("user/config.json")).unwrap(),
            b"preserved config"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        // Selecting data removal subsequently removes the retained directory, too.
        assert!(crate::setup::uninstall::remove_files(&archive, &root, true, None).is_empty());
        let mut child =
            spawn_cleanup(&crate::setup::uninstall::cleanup_plan(&root, true), 0).unwrap();
        wait(&mut child);
        assert!(!root.exists());
    }
}
