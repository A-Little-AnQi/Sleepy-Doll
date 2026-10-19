use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};

pub const NAME: &str = "data";
pub const LEGACY_NAME: &str = ".sleepy-doll";

pub fn path(user_directory: &Path) -> PathBuf {
    user_directory.join(NAME)
}

pub fn relocated_path(user_directory: &Path, original: &Path) -> PathBuf {
    relative_path(&user_directory.join(LEGACY_NAME), original)
        .map(|relative| path(user_directory).join(relative))
        .unwrap_or_else(|| original.to_path_buf())
}

pub fn relative_path(directory: &Path, original: &Path) -> Option<PathBuf> {
    let directory = ordinary_path(directory);
    let original = ordinary_path(original);
    let mut actual = original.components();
    for expected in directory.components() {
        let next = actual.next()?;
        #[cfg(windows)]
        let same = expected
            .as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(&next.as_os_str().to_string_lossy());
        #[cfg(not(windows))]
        let same = expected == next;
        if !same {
            return None;
        }
    }
    Some(actual.as_path().to_path_buf())
}

fn ordinary_path(original: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(raw) = original.to_str() {
        if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{unc}"));
        }
        if let Some(disk) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(disk);
        }
    }
    original.to_path_buf()
}

/// 整目录移动，数据库的 WAL、附件、浏览器状态及回滚记录一起保留。
pub fn migrate(user_directory: &Path, database: &Path) -> Result<()> {
    let previous = user_directory.join(LEGACY_NAME);
    let metadata = match fs::symlink_metadata(&previous) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(Error::Config(
            "旧数据目录不是普通目录，未迁移，请核对 .sleepy-doll 路径".into(),
        ));
    }
    let migration_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(user_directory.join("data-migration.lock"))?;
    migration_lock
        .try_lock()
        .map_err(|_| Error::Conflict("数据迁移正在进行，请稍后重新启动".into()))?;
    if fs::symlink_metadata(&previous)
        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        return Ok(());
    }
    let destination = path(user_directory);
    match fs::symlink_metadata(&destination) {
        Ok(destination_metadata) => {
            if destination_metadata.is_dir()
                && !is_link(&destination_metadata)
                && relative_path(&destination, database).is_some()
                && database.is_file()
                && legacy_has_only_auxiliary_state(&previous)?
            {
                // 回退运行旧版会重新生成界面和更新缓存，不代表出现第二份业务数据库。
                log::warn!(
                    "旧版界面缓存与已迁移数据共存，继续使用 data；旧版状态保留，未覆盖任何文件"
                );
                return Ok(());
            }
            return Err(Error::Config(
                "发现 .sleepy-doll 与 data 两份数据目录，未覆盖任何内容；请先核对并合并旧数据"
                    .into(),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut guards: Vec<(PathBuf, fs::File)> = Vec::new();
    for lock_path in [
        previous.join("sleepy-doll.runtime.lock"),
        database.with_extension("runtime.lock"),
    ] {
        if lock_path.parent().is_some_and(Path::is_dir)
            && !guards.iter().any(|(path, _)| {
                relative_path(path, &lock_path).is_some_and(|suffix| suffix.as_os_str().is_empty())
            })
        {
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)?;
            file.try_lock().map_err(|_| {
                Error::Conflict("旧数据目录正在使用，请关闭旧版 Sleepy Doll 后重新启动".into())
            })?;
            guards.push((lock_path, file));
        }
    }
    // Windows 不能重命名包含已锁文件的目录；内部句柄检查后关闭，目录外的锁继续保护迁移。
    drop(guards);
    fs::rename(&previous, &destination).map_err(|error| {
        Error::Config(format!(
            "旧数据目录迁移到 data 失败，数据仍保留在原位置；请关闭占用它的程序后重试：{error}"
        ))
    })?;
    log::info!("用户数据目录已从 .sleepy-doll 迁移到 data");
    Ok(())
}

fn legacy_has_only_auxiliary_state(directory: &Path) -> Result<bool> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        let name = entry.file_name();
        let allowed = if metadata.is_dir() {
            matches!(
                name.to_str(),
                Some("webview2" | "updates" | "config-changes")
            )
        } else {
            metadata.is_file() && name == "window-placement.json"
        };
        if !allowed || is_link(&metadata) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use serde_json::json;

    fn legacy(name: &str) -> (PathBuf, PathBuf) {
        let root = crate::test_root(name);
        fs::create_dir_all(root.join(LEGACY_NAME)).unwrap();
        let config = root.join("config.json");
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../sleepy-doll.config.example.json")).unwrap();
        value["storage"]["database"] = json!("./.sleepy-doll/sleepy-doll.db");
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        (root, config)
    }

    #[test]
    fn migration_keeps_wal_attachments_preferences_and_recovery_records() {
        let (root, config) = legacy("data-migration");
        for (name, content) in [
            ("sleepy-doll.db", "database"),
            ("sleepy-doll.db-wal", "wal"),
            ("sleepy-doll.db-shm", "shm"),
            ("sleepy-doll.artifacts/sha256/artifact", "evidence"),
            ("updates/preferences.json", "preferences"),
            ("config-changes/record.json", "recovery"),
            ("webview2/profile", "browser"),
            ("window-placement.json", "window"),
        ] {
            let original = root.join(LEGACY_NAME).join(name);
            fs::create_dir_all(original.parent().unwrap()).unwrap();
            fs::write(original, content).unwrap();
        }
        let loaded = AppConfig::load(&config).unwrap();
        assert!(loaded.storage.database.ends_with("data/sleepy-doll.db"));
        for (name, content) in [
            ("sleepy-doll.db", "database"),
            ("sleepy-doll.db-wal", "wal"),
            ("sleepy-doll.db-shm", "shm"),
            ("sleepy-doll.artifacts/sha256/artifact", "evidence"),
            ("updates/preferences.json", "preferences"),
            ("config-changes/record.json", "recovery"),
            ("webview2/profile", "browser"),
            ("window-placement.json", "window"),
        ] {
            assert_eq!(fs::read_to_string(path(&root).join(name)).unwrap(), content);
        }
        assert!(!root.join(LEGACY_NAME).exists());
        assert_eq!(
            AppConfig::load(&config).unwrap().storage.database,
            loaded.storage.database
        );
    }

    #[test]
    fn migrated_database_retains_conversations_and_runs() {
        let (root, config) = legacy("data-migration-journal");
        let journal = crate::runtime::store::journal::Journal::open(
            &root.join(LEGACY_NAME).join("sleepy-doll.db"),
        )
        .unwrap();
        let run = journal
            .create("原始用户目标", "conversation", "client", 3600, None)
            .unwrap();
        drop(journal);
        let loaded = AppConfig::load(&config).unwrap();
        let journal =
            crate::runtime::store::journal::Journal::open(&loaded.storage.database).unwrap();
        assert_eq!(journal.get(&run.id).unwrap().prompt, "原始用户目标");
    }

    #[test]
    fn recreated_legacy_caches_do_not_block_an_existing_canonical_database() {
        let (root, config) = legacy("data-migration-legacy-cache");
        fs::create_dir_all(path(&root).join("webview2")).unwrap();
        fs::write(path(&root).join("webview2/profile"), "canonical profile").unwrap();
        let database = path(&root).join("sleepy-doll.db");
        let journal = crate::runtime::store::journal::Journal::open(&database).unwrap();
        let run = journal
            .create("保留已有会话", "conversation", "client", 3600, None)
            .unwrap();
        drop(journal);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["storage"]["database"] = json!("./data/sleepy-doll.db");
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        for (name, content) in [
            ("webview2/profile", "legacy profile"),
            ("updates/preferences.json", "legacy preferences"),
            ("window-placement.json", "legacy window"),
        ] {
            let file = root.join(LEGACY_NAME).join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }
        let original = fs::read(&config).unwrap();
        let loaded = AppConfig::load(&config).unwrap();
        assert_eq!(
            loaded.storage.database.canonicalize().unwrap(),
            database.canonicalize().unwrap()
        );
        assert_eq!(fs::read(&config).unwrap(), original);
        assert_eq!(
            fs::read_to_string(path(&root).join("webview2/profile")).unwrap(),
            "canonical profile"
        );
        assert_eq!(
            fs::read_to_string(root.join(LEGACY_NAME).join("webview2/profile")).unwrap(),
            "legacy profile"
        );
        let journal = crate::runtime::store::journal::Journal::open(&database).unwrap();
        assert_eq!(journal.get(&run.id).unwrap().prompt, "保留已有会话");
    }

    #[test]
    fn a_second_legacy_database_is_still_a_conflict() {
        let (root, config) = legacy("data-migration-two-databases");
        fs::create_dir_all(path(&root)).unwrap();
        fs::write(path(&root).join("sleepy-doll.db"), "canonical database").unwrap();
        fs::write(
            root.join(LEGACY_NAME).join("sleepy-doll.db"),
            "legacy database",
        )
        .unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["storage"]["database"] = json!("./data/sleepy-doll.db");
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let original = fs::read(&config).unwrap();
        assert!(AppConfig::load(&config).is_err());
        assert_eq!(fs::read(&config).unwrap(), original);
        assert_eq!(
            fs::read_to_string(path(&root).join("sleepy-doll.db")).unwrap(),
            "canonical database"
        );
        assert_eq!(
            fs::read_to_string(root.join(LEGACY_NAME).join("sleepy-doll.db")).unwrap(),
            "legacy database"
        );
    }

    #[test]
    fn conflicting_directories_and_active_client_do_not_move_or_rewrite() {
        let (root, config) = legacy("data-migration-conflict");
        fs::create_dir_all(path(&root)).unwrap();
        fs::write(path(&root).join("keep"), "new").unwrap();
        fs::write(root.join(LEGACY_NAME).join("keep"), "old").unwrap();
        let original = fs::read(&config).unwrap();
        assert!(AppConfig::load(&config).is_err());
        assert_eq!(fs::read(&config).unwrap(), original);
        assert_eq!(fs::read_to_string(path(&root).join("keep")).unwrap(), "new");
        assert_eq!(
            fs::read_to_string(root.join(LEGACY_NAME).join("keep")).unwrap(),
            "old"
        );

        let (root, config) = legacy("data-migration-locked");
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(LEGACY_NAME).join("sleepy-doll.runtime.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let original = fs::read(&config).unwrap();
        assert!(AppConfig::load(&config).is_err());
        assert!(root.join(LEGACY_NAME).is_dir());
        assert!(!path(&root).exists());
        assert_eq!(fs::read(&config).unwrap(), original);
    }

    #[test]
    fn custom_database_location_and_secret_references_stay_unchanged() {
        let (root, config) = legacy("data-migration-custom");
        fs::create_dir_all(root.join("archive")).unwrap();
        fs::write(root.join("archive/history.db"), "custom").unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["storage"]["database"] = json!("./archive/history.db");
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let original = fs::read(&config).unwrap();
        let loaded = AppConfig::load(&config).unwrap();
        assert!(loaded.storage.database.ends_with("archive/history.db"));
        assert_eq!(fs::read(&config).unwrap(), original);
        assert_eq!(
            fs::read_to_string(loaded.storage.database).unwrap(),
            "custom"
        );
    }

    #[test]
    fn interrupted_config_update_is_completed_on_next_load() {
        let (root, config) = legacy("data-migration-resume");
        fs::write(root.join(LEGACY_NAME).join("sleepy-doll.db"), "original").unwrap();
        fs::rename(root.join(LEGACY_NAME), path(&root)).unwrap();
        let loaded = AppConfig::load(&config).unwrap();
        assert_eq!(
            fs::read_to_string(loaded.storage.database).unwrap(),
            "original"
        );
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(
            Path::new(value["storage"]["database"].as_str().unwrap()),
            Path::new("data/sleepy-doll.db")
        );
    }

    #[test]
    fn absolute_database_and_other_data_paths_follow_the_moved_directory() {
        let (root, config) = legacy("data-migration-absolute");
        let previous = root.join(LEGACY_NAME);
        fs::write(previous.join("history.db"), "absolute database").unwrap();
        fs::create_dir_all(previous.join("catalog")).unwrap();
        fs::create_dir_all(previous.join("skills")).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        value["storage"]["database"] = json!(previous.join("history.db"));
        value["runtime"]["catalogDirectory"] = json!("./.sleepy-doll/catalog");
        value["agent"]["skillDirectories"] = json!(["./.sleepy-doll/skills"]);
        fs::write(&config, serde_json::to_vec(&value).unwrap()).unwrap();
        let loaded = AppConfig::load(&config).unwrap();
        assert_eq!(
            fs::read_to_string(loaded.storage.database).unwrap(),
            "absolute database"
        );
        assert!(loaded.runtime.catalog_directory.ends_with("data/catalog"));
        assert!(loaded.agent.skill_directories[0].ends_with("data/skills"));
    }

    #[test]
    fn developer_migration_preserves_external_paths_and_existing_destination() {
        let (root, _) = legacy("dev-data-migration");
        let source = root.join(".sleepy-doll/dev");
        fs::create_dir_all(source.join(".sleepy-doll")).unwrap();
        fs::create_dir_all(root.join(".sleepy-doll/catalog")).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../sleepy-doll.config.example.json")).unwrap();
        value["runtime"]["catalogDirectory"] = json!("../catalog");
        value["storage"]["database"] = json!(source.join(".sleepy-doll/sleepy-doll.db"));
        fs::write(
            source.join("config.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        fs::write(
            source.join(".sleepy-doll/sleepy-doll.db"),
            "developer state",
        )
        .unwrap();
        let destination = root.join("target/dev/user/config.json");
        crate::config::migrate_developer_configuration(&source.join("config.json"), &destination)
            .unwrap();
        assert!(!source.exists());
        let loaded = AppConfig::load(&destination).unwrap();
        assert_eq!(
            fs::read_to_string(loaded.storage.database).unwrap(),
            "developer state"
        );
        assert_eq!(
            loaded.runtime.catalog_directory.canonicalize().unwrap(),
            root.join(".sleepy-doll/catalog").canonicalize().unwrap()
        );
        let before = fs::read(&destination).unwrap();
        crate::config::migrate_developer_configuration(&root.join("config.json"), &destination)
            .unwrap();
        assert_eq!(fs::read(&destination).unwrap(), before);
    }
}
