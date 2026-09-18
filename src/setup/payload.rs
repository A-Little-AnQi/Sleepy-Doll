//! 嵌入载荷的清单与切分。
//!
//! `installer/pack-payload.ps1` 把交付目录里的文件按相对路径排序后依次拼接，
//! 整段用裸 deflate 压缩成 `payload.bin`，清单落在 `payload.json`。解压由调用方
//! 用 flate2 完成，这里只处理解压之后的字节。

use serde::{Deserialize, Serialize};

use crate::setup::Error;

/// 清单里的一条：相对路径与解压后的字节数。顺序与 `payload.bin` 里的拼接顺序一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub size: u64,
}

/// 解压后的载荷：清单与全部文件内容。
pub struct Archive {
    entries: Vec<Entry>,
    data: Vec<u8>,
}

impl std::fmt::Debug for Archive {
    /// 载荷有十几兆，整段打出来没有意义，只报条目数与字节数。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Archive")
            .field("entries", &self.entries.len())
            .field("bytes", &self.data.len())
            .finish()
    }
}

impl Archive {
    /// `manifest` 是 `payload.json` 的原始字节，`data` 是 `payload.bin` 解压后的结果。
    pub fn new(manifest: &[u8], data: Vec<u8>) -> Result<Self, Error> {
        let entries: Vec<Entry> = serde_json::from_slice(manifest)
            .map_err(|error| Error::message(format!("载荷清单无法解析：{error}")))?;
        for entry in &entries {
            if !is_safe_relative_path(&entry.path) {
                return Err(Error::message(format!(
                    "载荷清单里的路径不合法：{}",
                    entry.path
                )));
            }
        }
        let listed: u64 = entries.iter().map(|entry| entry.size).sum();
        if listed != data.len() as u64 {
            return Err(Error::message(format!(
                "载荷与清单对不上：清单共 {listed} 字节，解压得到 {} 字节",
                data.len()
            )));
        }
        Ok(Self { entries, data })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// 按清单顺序切出每个文件的相对路径与内容。
    pub fn files(&self) -> Vec<(&str, &[u8])> {
        let mut offset = 0;
        let mut files = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let end = offset + entry.size as usize;
            files.push((entry.path.as_str(), &self.data[offset..end]));
            offset = end;
        }
        files
    }
}

/// 只接受 `a/b/c` 形式的相对路径。绝对路径、盘符和 `..` 会把文件写到安装目录外面。
fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(manifest: &str, data: &[u8]) -> Result<Archive, Error> {
        Archive::new(manifest.as_bytes(), data.to_vec())
    }

    #[test]
    fn files_are_split_in_manifest_order() {
        let archive = archive(
            r#"[{"path":"sleepy-doll.exe","size":3},{"path":"skills/a.md","size":2}]"#,
            b"abcde",
        )
        .unwrap();
        assert_eq!(
            archive.files(),
            vec![
                ("sleepy-doll.exe", b"abc".as_slice()),
                ("skills/a.md", b"de")
            ]
        );
        assert_eq!(archive.entries().len(), 2);
    }

    #[test]
    fn empty_payload_is_accepted() {
        let archive = archive("[]", b"").unwrap();
        assert!(archive.files().is_empty());
    }

    #[test]
    fn mismatch_between_manifest_and_data_is_refused() {
        let error = archive(r#"[{"path":"a","size":9}]"#, b"ab").unwrap_err();
        assert!(error.to_string().contains("对不上"), "{error}");
    }

    #[test]
    fn paths_that_escape_the_target_are_refused() {
        for path in [
            "..\\escape.exe",
            "../escape.exe",
            "a/../../escape.exe",
            "/absolute.exe",
            "C:/windows/system32/evil.dll",
            "a//b",
            "",
        ] {
            let manifest = format!(r#"[{{"path":{path:?},"size":0}}]"#);
            assert!(archive(&manifest, b"").is_err(), "{path} 不该被接受");
        }
    }

    #[test]
    fn nested_paths_are_accepted() {
        let archive = archive(
            r#"[{"path":"bridge/BgiBridge.dll","size":1},{"path":"skills/x/y.md","size":1}]"#,
            b"ab",
        )
        .unwrap();
        assert_eq!(archive.files().len(), 2);
    }
}
