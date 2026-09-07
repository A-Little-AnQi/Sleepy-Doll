use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ModelProtocol {
    OpenaiResponses,
    OpenaiChat,
    AnthropicMessages,
    Gemini,
    OllamaChat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelOptions {
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default = "default_model_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}
impl Default for ModelOptions {
    fn default() -> Self {
        Self {
            temperature: None,
            max_output_tokens: None,
            timeout_ms: default_model_timeout(),
            reasoning_effort: None,
        }
    }
}

const fn default_model_timeout() -> u64 {
    120_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub protocol: ModelProtocol,
    pub model: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub options: ModelOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConfig {
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    #[serde(default = "default_max_tools")]
    pub max_tool_calls_per_turn: usize,
    pub system_prompt: String,
    #[serde(default)]
    pub skill_directories: Vec<PathBuf>,
    #[serde(default = "default_true")]
    pub auto_load_skills: bool,
    #[serde(default = "default_max_skills")]
    pub max_loaded_skills: usize,
    #[serde(default)]
    pub disabled_skills: Vec<String>,
    #[serde(default)]
    pub fallback_models: Vec<String>,
}

const fn default_max_turns() -> usize {
    16
}
const fn default_max_tools() -> usize {
    8
}
const fn default_max_skills() -> usize {
    4
}
const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeConfig {
    #[serde(default)]
    pub enabled: bool,
    pub base_url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default = "default_bridge_timeout")]
    pub timeout_ms: u64,
}

const fn default_bridge_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginsConfig {
    #[serde(default)]
    pub directories: Vec<PathBuf>,
    #[serde(default)]
    pub enabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageConfig {
    pub database: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppConfig {
    pub version: u32,
    pub active_model: String,
    pub models: Vec<ModelConfig>,
    pub agent: AgentConfig,
    pub bridge: BridgeConfig,
    #[serde(default)]
    pub plugins: PluginsConfig,
    pub storage: StorageConfig,
    #[serde(default)]
    pub runtime: crate::runtime::policy::RuntimeConfig,
    #[serde(default)]
    pub hooks: Vec<crate::runtime::hooks::HttpHookConfig>,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        if value["version"] == 1 {
            let backup = path.with_extension("v1.backup.json");
            if !backup.exists() {
                fs::copy(path, &backup)?;
            }
            value["version"] = serde_json::json!(2);
            if value["runtime"].is_null() {
                value["runtime"] =
                    serde_json::to_value(crate::runtime::policy::RuntimeConfig::default())?;
            }
            let candidate: Self = serde_json::from_value(value.clone())?;
            candidate.validate()?;
            atomic_write(path, &value)?;
        }
        expand_env(&mut value)?;
        let mut config: Self = serde_json::from_value(value)?;
        let base = path
            .canonicalize()?
            .parent()
            .ok_or_else(|| Error::Config("configuration path has no parent".into()))?
            .to_path_buf();
        for directory in &mut config.agent.skill_directories {
            *directory = absolute(&base, directory);
        }
        for directory in &mut config.plugins.directories {
            *directory = absolute(&base, directory);
        }
        config.storage.database = absolute(&base, &config.storage.database);
        config.runtime.catalog_directory = absolute(&base, &config.runtime.catalog_directory);
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        self.runtime.validate()?;
        if self.version != 1 && self.version != 2 {
            return Err(Error::Config("version must be 1 or 2".into()));
        }
        if self.models.is_empty() {
            return Err(Error::Config(
                "at least one model must be configured".into(),
            ));
        }
        let matching = self
            .models
            .iter()
            .filter(|model| model.id == self.active_model)
            .count();
        if matching != 1 {
            return Err(Error::Config(
                "activeModel must reference exactly one configured model".into(),
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for model in &self.models {
            if model.id.trim().is_empty()
                || model.model.trim().is_empty()
                || model.base_url.trim().is_empty()
            {
                return Err(Error::Config(
                    "model id, model, and baseUrl must not be empty".into(),
                ));
            }
            if !ids.insert(&model.id) {
                return Err(Error::Config(format!("duplicate model id: {}", model.id)));
            }
        }
        let configured = self
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut fallbacks = std::collections::HashSet::new();
        if self.agent.fallback_models.iter().any(|id| {
            id == &self.active_model || !configured.contains(id.as_str()) || !fallbacks.insert(id)
        }) {
            return Err(Error::Config(
                "fallbackModels must contain unique configured non-active models".into(),
            ));
        }
        if self.agent.max_turns == 0 || self.agent.max_turns > 128 {
            return Err(Error::Config(
                "agent.maxTurns must be between 1 and 128".into(),
            ));
        }
        if self.agent.max_tool_calls_per_turn == 0 || self.agent.max_tool_calls_per_turn > 64 {
            return Err(Error::Config(
                "agent.maxToolCallsPerTurn must be between 1 and 64".into(),
            ));
        }
        if self.bridge.enabled && self.bridge.token.as_deref().unwrap_or("").is_empty() {
            return Err(Error::Config("bridge is enabled but token is empty".into()));
        }
        Ok(())
    }

    pub fn active(&self) -> &ModelConfig {
        self.models
            .iter()
            .find(|model| model.id == self.active_model)
            .expect("validated active model")
    }

    pub fn set_active(path: impl AsRef<Path>, model_id: &str) -> Result<()> {
        let path = path.as_ref();
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        value["activeModel"] = serde_json::Value::String(model_id.to_owned());
        let candidate: Self = serde_json::from_value(value.clone())?;
        candidate.validate()?;
        atomic_write(path, &value)?;
        Ok(())
    }

    pub fn set_skill_enabled(path: impl AsRef<Path>, name: &str, enabled: bool) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            let disabled = value["agent"]["disabledSkills"]
                .as_array_mut()
                .ok_or_else(|| Error::Config("agent.disabledSkills must be an array".into()))?;
            disabled.retain(|entry| entry.as_str() != Some(name));
            if !enabled {
                disabled.push(serde_json::Value::String(name.to_owned()));
            }
            Ok(())
        })
    }

    pub fn set_plugin_enabled(path: impl AsRef<Path>, id: &str, enabled: bool) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            let entries = value["plugins"]["enabled"]
                .as_array_mut()
                .ok_or_else(|| Error::Config("plugins.enabled must be an array".into()))?;
            entries.retain(|entry| entry.as_str() != Some(id));
            if enabled {
                entries.push(serde_json::Value::String(id.to_owned()));
            }
            Ok(())
        })
    }

    pub fn save_model(path: impl AsRef<Path>, input: &serde_json::Value) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            let id = input["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Error::Config("model.id is required".into()))?;
            let models = value["models"]
                .as_array_mut()
                .ok_or_else(|| Error::Config("models must be an array".into()))?;
            let existing = models
                .iter_mut()
                .find(|model| model["id"].as_str() == Some(id));
            if let Some(model) = existing {
                for field in ["name", "protocol", "model", "baseUrl"] {
                    model[field] = input[field].clone();
                }
                if input["apiKey"].as_str().is_some_and(|key| !key.is_empty()) {
                    model["apiKey"] = input["apiKey"].clone();
                }
            } else {
                models.push(serde_json::json!({
                    "id": id,
                    "name": input["name"],
                    "protocol": input["protocol"],
                    "model": input["model"],
                    "baseUrl": input["baseUrl"],
                    "apiKey": input["apiKey"],
                    "headers": {},
                    "options": {}
                }));
            }
            Ok(())
        })
    }
}

/// Resolves the configuration file. An explicit argument or `SLEEPY_DOLL_CONFIG`
/// always wins; otherwise the file lives in the `user/` directory of the
/// installation. User configuration is never read from, or written to, the
/// working directory or the source tree.
pub fn resolve_path() -> PathBuf {
    env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .or_else(|| env::var_os("SLEEPY_DOLL_CONFIG").map(PathBuf::from))
        .unwrap_or_else(default_path)
}

pub fn default_path() -> PathBuf {
    if let Some(root) = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        let directory = root.join("user");
        if writable_directory(&directory) {
            return directory.join("config.json");
        }
    }
    // Read-only installation locations fall back to the per-user data root
    // rather than refusing to start.
    let fallback = env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sleepy Doll")
        .join("user");
    if writable_directory(&fallback) {
        return fallback.join("config.json");
    }
    PathBuf::from("user").join("config.json")
}

fn writable_directory(directory: &Path) -> bool {
    if fs::create_dir_all(directory).is_err() {
        return false;
    }
    let probe = directory.join(".write-probe");
    match fs::File::create(&probe) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// A first run materialises the shipped template in the user directory, so the
/// application never has to write user data into the source tree.
pub fn seed(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let directory = path
        .parent()
        .ok_or_else(|| Error::Config("configuration path has no parent".into()))?;
    fs::create_dir_all(directory)?;
    // The template keeps its Skill, Plugin and catalog roots next to itself.
    for child in ["skills", "plugins", "catalog", ".sleepy-doll"] {
        fs::create_dir_all(directory.join(child))?;
    }
    fs::write(path, TEMPLATE)?;
    Ok(())
}

const TEMPLATE: &str = include_str!("../sleepy-doll.config.example.json");

fn update_raw(
    path: &Path,
    change: impl FnOnce(&mut serde_json::Value) -> Result<()>,
) -> Result<()> {
    let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    if value["agent"]["disabledSkills"].is_null() {
        value["agent"]["disabledSkills"] = serde_json::json!([]);
    }
    change(&mut value)?;
    let candidate: AppConfig = serde_json::from_value(value.clone())?;
    candidate.validate()?;
    atomic_write(path, &value)?;
    Ok(())
}

fn atomic_write(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(format!("{}\n", serde_json::to_string_pretty(value)?).as_bytes())?;
    file.sync_all()?;
    drop(file);
    if let Err(e) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(e.into());
    }
    Ok(())
}

fn absolute(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn expand_env(value: &mut serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::String(text) => {
            if let Some(name) = text
                .strip_prefix("${ENV:")
                .and_then(|rest| rest.strip_suffix('}'))
            {
                *text = env::var(name).unwrap_or_default();
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                expand_env(item)?;
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values_mut() {
                expand_env(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_template_is_seedable_and_valid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("user").join("config.json");
        seed(&path).unwrap();
        assert!(path.exists());
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.active_model, "local-mock");
        // The template keeps its resource roots next to the configuration file.
        assert!(directory.path().join("user").join("skills").is_dir());
        assert!(directory.path().join("user").join(".sleepy-doll").is_dir());
    }

    #[test]
    fn rejects_missing_active_model() {
        let config = AppConfig {
            version: 1,
            active_model: "missing".into(),
            models: vec![],
            agent: AgentConfig {
                max_turns: 16,
                max_tool_calls_per_turn: 8,
                system_prompt: "test".into(),
                skill_directories: vec![],
                auto_load_skills: true,
                max_loaded_skills: 4,
                disabled_skills: vec![],
                fallback_models: vec![],
            },
            bridge: BridgeConfig {
                enabled: false,
                base_url: "http://localhost".into(),
                token: None,
                instance_id: None,
                timeout_ms: 1000,
            },
            plugins: PluginsConfig::default(),
            storage: StorageConfig {
                database: "test.db".into(),
            },
            runtime: Default::default(),
            hooks: vec![],
        };
        assert!(config.validate().is_err());
    }
}
