//! 文件日志：把记录写进用户目录下的 `log/sleepy-doll.log`。
//!
//! 库只通过 `log` 门面发记录，写到哪里由外壳在启动时用 [`init`] 决定。

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use log::{LevelFilter, Log, Metadata, Record};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// 日志目录名，位于用户目录下。
pub const DIRECTORY: &str = "log";

const FILE_NAME: &str = "sleepy-doll.log";

/// 单个文件的上限。超过就轮转一次，磁盘上最多留两份。
const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// 在 `user_directory` 下准备日志目录并接管全局日志与 panic。
pub fn init(user_directory: &Path) -> Result<(), std::io::Error> {
    let directory = user_directory.join(DIRECTORY);
    fs::create_dir_all(&directory)?;
    let path = directory.join(FILE_NAME);
    rotate(&path)?;
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    // 已经被占用（测试或嵌入场景）时保留既有实现，不自作主张替换。
    let _ = log::set_boxed_logger(Box::new(Sink {
        state: Mutex::new(State {
            file,
            written,
            path,
        }),
    }));
    log::set_max_level(LevelFilter::Info);
    install_panic_hook();
    Ok(())
}

/// 记录一次 panic 再交给原来的处理过程。
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_else(|| "unknown".into());
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        log::error!(target: "panic", "{payload} ({location})");
        previous(info);
    }));
}

/// 超过上限时把当前文件挪成 `.1`，覆盖上一轮的那份。
fn rotate(path: &Path) -> Result<(), std::io::Error> {
    let oversized = fs::metadata(path).is_ok_and(|meta| meta.len() >= MAX_BYTES);
    if !oversized {
        return Ok(());
    }
    let previous = path.with_extension("log.1");
    let _ = fs::remove_file(&previous);
    fs::rename(path, previous)
}

struct Sink {
    state: Mutex<State>,
}

struct State {
    file: File,
    written: u64,
    path: PathBuf,
}

impl Log for Sink {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "----------T--:--:--Z".into());
        let target = record.target();
        let line = format!(
            "{timestamp} {:<5} [{target}] {}\n",
            record.level(),
            record.args()
        );
        // 写日志失败时只能丢掉。
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.written + line.len() as u64 > MAX_BYTES {
            // 轮转失败时继续往原文件写。
            let _ = rotate(&state.path);
            if let Ok(file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&state.path)
            {
                state.file = file;
                state.written = 0;
            }
        }
        if state.file.write_all(line.as_bytes()).is_ok() {
            state.written += line.len() as u64;
        }
    }

    fn flush(&self) {
        if let Ok(mut state) = self.state.lock() {
            let _ = state.file.flush();
        }
    }
}
