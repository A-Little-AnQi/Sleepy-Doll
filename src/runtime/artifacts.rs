use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    root: PathBuf,
    max_bytes: u64,
}

impl ArtifactStore {
    pub fn new(root: impl AsRef<Path>, max_bytes: u64) -> Result<Self> {
        if max_bytes == 0 || max_bytes > 1024 * 1024 * 1024 {
            return Err(Error::Config("artifact size limit is invalid".into()));
        }
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("sha256"))?;
        Ok(Self { root, max_bytes })
    }

    pub fn put(&self, content: &[u8]) -> Result<String> {
        if content.len() as u64 > self.max_bytes {
            return Err(Error::Config(
                "artifact exceeds configured size limit".into(),
            ));
        }
        let hash = format!("{:x}", Sha256::digest(content));
        let destination = self.path(&hash)?;
        if destination.exists() {
            let existing = fs::read(&destination)?;
            if existing != content {
                return Err(Error::Conflict("artifact hash collision".into()));
            }
            return Ok(hash);
        }
        let parent = destination
            .parent()
            .ok_or_else(|| Error::Config("invalid artifact path".into()))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".{}.{}.tmp", hash, uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        if let Err(error) = fs::rename(&temporary, &destination) {
            let _ = fs::remove_file(&temporary);
            if destination.exists() && fs::read(&destination)? == content {
                return Ok(hash);
            }
            return Err(error.into());
        }
        Ok(hash)
    }

    pub fn get(&self, id: &str) -> Result<Vec<u8>> {
        let path = self.path(id)?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > self.max_bytes {
            return Err(Error::Conflict(
                "stored artifact exceeds configured limit".into(),
            ));
        }
        let content = fs::read(path)?;
        let current = format!("{:x}", Sha256::digest(&content));
        if current != id {
            return Err(Error::Conflict(
                "stored artifact failed integrity check".into(),
            ));
        }
        Ok(content)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.path(id).is_ok_and(|path| path.is_file())
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::Config("invalid artifact id".into()));
        }
        Ok(self.root.join("sha256").join(&id[..2]).join(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_are_content_addressed_and_verified() {
        let directory = tempfile::tempdir().unwrap();
        let store = ArtifactStore::new(directory.path(), 1024).unwrap();
        let first = store.put(b"same").unwrap();
        let second = store.put(b"same").unwrap();
        assert_eq!(first, second);
        assert_eq!(store.get(&first).unwrap(), b"same");
        assert!(store.get("../invalid").is_err());
    }
}
