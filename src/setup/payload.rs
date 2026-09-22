//! 嵌入载荷的清单与切分。
//!
//! `payload.bin` 是全部文件按路径排序拼接后的裸 deflate 流，`payload.json` 是清单。
//! 解压由调用方完成，这里只处理解压之后的字节。

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
    /// 只报条目数与字节数。
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

/// 只接受 `a/b/c` 形式的相对路径：绝对路径、盘符和 `..` 会写到安装目录外面。
fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
