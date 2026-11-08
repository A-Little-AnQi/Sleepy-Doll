//! 受限的 Windows 定时任务插件；模型只提交时间与已发布任务引用。
pub mod calendar;
mod clock;
mod defaults;
mod history;
pub mod model;
mod service;
mod store;
pub mod system;
mod tools;

pub use service::Scheduler;
pub use service::run_cli_if_requested;

pub(crate) fn handle(
    service: &Scheduler,
    method: &str,
    args: &serde_json::Value,
) -> crate::Result<serde_json::Value> {
    tools::dispatch(service, method, args)
}

pub fn digest(value: &impl serde::Serialize) -> crate::Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value)?)))
}
