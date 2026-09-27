//! 从模型服务拉可用模型名。走本机 HTTP。

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::{
    config::{ModelConfig, ModelProtocol},
    error::{Error, Result},
};

use super::ProtocolModel;

const LIST_TIMEOUT: Duration = Duration::from_secs(20);

const COMPAT_SUFFIXES: &[&str] = &[
    "/api/claudecode",
    "/api/anthropic",
    "/apps/anthropic",
    "/api/coding",
    "/claudecode",
    "/anthropic",
    "/step_plan",
    "/coding",
    "/claude",
];

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub models: Vec<String>,
    pub context_windows: std::collections::BTreeMap<String, u64>,
}

pub fn list_remote_models(config: &ModelConfig, models_url: Option<&str>) -> Result<ModelCatalog> {
    if config.protocol != ModelProtocol::OllamaChat
        && config.api_key.as_deref().unwrap_or("").is_empty()
    {
        return Err(Error::Config("先填写 API Key，再获取模型。".into()));
    }
    if config.base_url.trim().is_empty() && models_url.unwrap_or("").trim().is_empty() {
        return Err(Error::Config("先填写 API 地址。".into()));
    }
    let urls = catalog_urls(&config.base_url, config.protocol, models_url);
    if urls.is_empty() {
        return Err(Error::Config("先填写 API 地址。".into()));
    }
    let headers = ProtocolModel::new(config.clone()).headers();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(LIST_TIMEOUT))
        .build()
        .new_agent();
    let mut last_empty = false;
    for url in urls {
        let mut request = agent.get(&url);
        request = request.header("accept", "application/json");
        for (name, value) in &headers {
            request = request.header(name, value);
        }
        let mut response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(code)) => {
                if matches!(code, 404 | 405) {
                    continue;
                }
                return Err(http_status_error(code));
            }
            Err(_) => {
                return Err(Error::Config(
                    "连不上这个地址，请检查网络和 API 地址。".into(),
                ));
            }
        };
        let code = u16::from(response.status());
        if matches!(code, 404 | 405) {
            continue;
        }
        if !(200..300).contains(&code) {
            return Err(http_status_error(code));
        }
        let Ok(body) = response.body_mut().read_json::<Value>() else {
            last_empty = true;
            continue;
        };
        let catalog = parse_catalog(&body);
        if catalog.models.is_empty() {
            last_empty = true;
            continue;
        }
        return Ok(catalog);
    }
    if last_empty {
        return Ok(ModelCatalog::default());
    }
    Err(Error::Config(
        "这个服务没有模型列表，请手动填写模型名称。".into(),
    ))
}

fn http_status_error(code: u16) -> Error {
    match code {
        401 | 403 => Error::Config("密钥无效或没有访问权限。".into()),
        429 => Error::Config("服务限流了，请稍后再试。".into()),
        _ => Error::Config(format!("服务返回 HTTP {code}，请核对地址和密钥。")),
    }
}

pub(crate) fn catalog_urls(
    base_url: &str,
    protocol: ModelProtocol,
    models_url: Option<&str>,
) -> Vec<String> {
    if let Some(url) = models_url.map(str::trim).filter(|url| !url.is_empty()) {
        return vec![url.to_string()];
    }
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Vec::new();
    }
    let mut urls = Vec::new();
    let mut push = |url: String| {
        if !urls.contains(&url) {
            urls.push(url);
        }
    };
    match protocol {
        ModelProtocol::OllamaChat => {
            push(format!("{base}/api/tags"));
            push(format!("{base}/v1/models"));
        }
        ModelProtocol::Gemini => push(format!("{base}/models")),
        _ => {
            if ends_with_version_segment(base) {
                push(format!("{base}/models"));
                if !base.ends_with("/v1") {
                    push(format!("{base}/v1/models"));
                }
            } else {
                push(format!("{base}/v1/models"));
                push(format!("{base}/models"));
            }
            let without_version = base
                .strip_suffix("/v1")
                .unwrap_or(base)
                .trim_end_matches('/');
            if let Some(root) = strip_compat_suffix(without_version) {
                push(format!("{root}/v1/models"));
                push(format!("{root}/models"));
            }
        }
    }
    urls
}

fn ends_with_version_segment(base: &str) -> bool {
    let Some(segment) = base.rsplit('/').next() else {
        return false;
    };
    let Some(digits) = segment.strip_prefix('v') else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn strip_compat_suffix(base: &str) -> Option<String> {
    for suffix in COMPAT_SUFFIXES {
        if let Some(root) = base.strip_suffix(suffix) {
            let root = root.trim_end_matches('/');
            if !root.is_empty() {
                return Some(root.to_string());
            }
        }
    }
    None
}

pub(crate) fn parse_catalog(body: &Value) -> ModelCatalog {
    let mut catalog = ModelCatalog::default();
    let mut push = |value: &str, item: &Value| {
        let id = value.trim().trim_start_matches("models/");
        if id.is_empty() {
            return;
        }
        if !catalog.models.iter().any(|existing| existing == id) {
            catalog.models.push(id.to_string());
        }
        let window = [
            &item["context_length"],
            &item["context_window"],
            &item["contextWindow"],
            &item["inputTokenLimit"],
        ]
        .into_iter()
        .filter_map(Value::as_u64)
        .find(|value| (8192..=2_000_000).contains(value));
        if let Some(window) = window {
            catalog
                .context_windows
                .entry(id.to_string())
                .or_insert(window);
        }
    };
    if let Some(items) = body["data"].as_array() {
        for item in items {
            if let Some(id) = item["id"].as_str() {
                push(id, item);
            }
        }
    }
    if let Some(items) = body["models"].as_array() {
        for item in items {
            if let Some(id) = item["id"].as_str().or_else(|| item["name"].as_str()) {
                push(id, item);
            }
        }
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_preserves_reported_capacity_and_ignores_invalid_limits() {
        let catalog = parse_catalog(&json!({"data":[
            {"id":"large","context_length":1_000_000,"top_provider":{"context_length":32000}},
            {"id":"small","context_window":32768},
            {"id":"unknown"}, {"id":"invalid","context_length":0},
            {"id":"large","context_length":128000}
        ],"models":[{"name":"models/gemini","inputTokenLimit":1048576}]}));
        assert_eq!(
            catalog.models,
            vec!["large", "small", "unknown", "invalid", "gemini"]
        );
        assert_eq!(catalog.context_windows["large"], 1_000_000);
        assert_eq!(catalog.context_windows["small"], 32768);
        assert_eq!(catalog.context_windows["gemini"], 1048576);
        assert!(!catalog.context_windows.contains_key("unknown"));
        assert!(!catalog.context_windows.contains_key("invalid"));
    }

    #[test]
    fn missing_capacity_uses_new_default_without_overwriting_explicit_limits() {
        let default: crate::config::ModelOptions = serde_json::from_value(json!({})).unwrap();
        let saved: crate::config::ModelOptions =
            serde_json::from_value(json!({"contextWindow":128000})).unwrap();
        assert_eq!(default.context_window, 256000);
        assert_eq!(saved.context_window, 128000);
    }
}
