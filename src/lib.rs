pub mod app;
pub mod bridge;
pub mod config;
pub mod data_directory;
pub mod distribution;
pub mod error;
pub mod extension;
pub mod logging;
pub mod model;
pub mod runtime;
pub mod setup;
pub mod webview_runtime;

pub use config::{AppConfig, ModelConfig, ModelProtocol};
pub use error::{Error, Result};

#[cfg(test)]
pub(crate) fn test_root(name: &str) -> std::path::PathBuf {
    std::env::var_os("SLEEPY_DOLL_TEST_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/.tmp/agent-runtime")
        })
        .join(name)
        .join(uuid::Uuid::new_v4().to_string())
}

#[cfg(all(test, feature = "client-resources-test"))]
#[path = "protected_ui_assets.rs"]
mod protected_resources_validation;
