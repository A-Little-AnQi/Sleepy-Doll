use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fs,
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

/// 与配置里写的是同一串名字。
impl std::fmt::Display for ModelProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::OpenaiResponses => "openai-responses",
            Self::OpenaiChat => "openai-chat",
            Self::AnthropicMessages => "anthropic-messages",
            Self::Gemini => "gemini",
            Self::OllamaChat => "ollama-chat",
        })
    }
}

/// Anthropic / Gemini 的鉴权头。`auto`：密钥以 `sk-ant-` 开头用 `x-api-key`，
/// 否则用 `Authorization: Bearer`。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModelAuth {
    #[default]
    Auto,
    ApiKey,
    Bearer,
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
    /// 该模型的上下文窗口（token）。运行时的上下文预算由它推导。
    #[serde(default = "default_context_window")]
    pub context_window: u64,
    /// Anthropic 提示缓存断点。OpenAI / Gemini 由服务端自动缓存，此开关无效。
    /// 不支持 `cache_control` 的 Claude 中转要关掉，否则请求返回 400。
    #[serde(default = "default_true")]
    pub prompt_cache: bool,
}

/// 主流模型的窗口量级，配置里按实际模型改。
const fn default_context_window() -> u64 {
    200_000
}
impl Default for ModelOptions {
    fn default() -> Self {
        Self {
            temperature: None,
            max_output_tokens: None,
            timeout_ms: default_model_timeout(),
            reasoning_effort: None,
            context_window: default_context_window(),
            prompt_cache: true,
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
    pub auth: ModelAuth,
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
/// 一轮里允许的调用数。单次运行的总量由 `runtime.maxTools` 约束。
const fn default_max_tools() -> usize {
    16
}
const fn default_max_skills() -> usize {
    4
}
const fn default_true() -> bool {
    true
}

pub fn user_skill_directory(config_dir: &Path, config: &AppConfig) -> Option<PathBuf> {
    let user = config_dir.join("skills");
    config
        .agent
        .skill_directories
        .iter()
        .find(|path| *path == &user)
        .cloned()
}

/// 配置目录旁的 `skills/` 是用户导入的，其余随产品。
pub fn skill_directory_source(
    config_dir: &Path,
    path: &Path,
) -> crate::extension::skills::SkillSource {
    if path == config_dir.join("skills") {
        crate::extension::skills::SkillSource::User
    } else {
        crate::extension::skills::SkillSource::Product
    }
}

/// 用户插件落点：配置目录旁的 `plugins/`。
pub fn user_plugin_directory(config_dir: &Path, config: &AppConfig) -> Option<PathBuf> {
    let user = config_dir.join("plugins");
    config
        .plugins
        .directories
        .iter()
        .find(|path| *path == &user)
        .cloned()
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
    /// 随产品提供的宿主插件默认开启，写进这里才关掉。
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageConfig {
    pub database: PathBuf,
}

/// 托盘图标只由桌面壳消费。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrayConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
        }
    }
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
    pub hooks: Vec<crate::runtime::host::hooks::HttpHookConfig>,
    #[serde(default)]
    pub tray: TrayConfig,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        // 按文件声明的版本判断，而不是按前一步刚改成的版本。
        let declared = value["version"].as_u64().unwrap_or(1);
        if declared == 1 {
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
            log::info!("配置已从 1 迁移到 2，迁移前的文件保留为 config.v1.backup.json");
        }
        // v2 的旧默认值 askEach 迁移为 standard；版本号无条件推进。
        if declared == 2 {
            value["version"] = serde_json::json!(3);
            if value["runtime"]["permissionMode"] == serde_json::json!("askEach") {
                value["runtime"]["permissionMode"] = serde_json::json!("standard");
            }
            let candidate: Self = serde_json::from_value(value.clone())?;
            candidate.validate()?;
            atomic_write(path, &value)?;
            log::info!("配置已从 2 迁移到 3");
        }
        // 去掉旧的产品技能目录，补上插件目录里的产品根；用户自己的两个目录不动。
        if declared == 3 {
            value["version"] = serde_json::json!(4);
            if !value["plugins"].is_object() {
                value["plugins"] = serde_json::json!({});
            }
            if let Some(directories) = value["agent"]["skillDirectories"].as_array_mut() {
                directories.retain(|entry| !is_parent_directory(entry, "skills"));
            }
            if !value["plugins"]["directories"].is_array() {
                value["plugins"]["directories"] = serde_json::json!([]);
            }
            let directories = value["plugins"]["directories"]
                .as_array_mut()
                .ok_or_else(|| Error::Config("plugins.directories must be an array".into()))?;
            // 产品根排在前面，重名时随产品的插件先占住 id。
            if !directories
                .iter()
                .any(|entry| is_parent_directory(entry, "plugins"))
            {
                directories.insert(0, serde_json::json!("../plugins"));
            }
            let candidate: Self = serde_json::from_value(value.clone())?;
            candidate.validate()?;
            atomic_write(path, &value)?;
            log::info!("配置已从 3 迁移到 4，领域技能改由 plugins/bgi 提供");
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

    pub fn write_document(path: impl AsRef<Path>, content: &str) -> Result<()> {
        let value: serde_json::Value = serde_json::from_str(content)
            .map_err(|error| Error::Config(format!("JSON 无效：{error}")))?;
        if !value.is_object() {
            return Err(Error::Config("配置必须是 JSON 对象".into()));
        }
        let candidate: Self = serde_json::from_value(value.clone())
            .map_err(|error| Error::Config(error.to_string()))?;
        candidate.validate()?;
        atomic_write(path.as_ref(), &value)
    }

    pub fn validate(&self) -> Result<()> {
        self.runtime.validate()?;
        if !(1..=4).contains(&self.version) {
            return Err(Error::Config("version must be 1, 2, 3 or 4".into()));
        }
        if self.models.is_empty() {
            if !self.active_model.is_empty() {
                return Err(Error::Config(
                    "activeModel must be empty when no models are configured".into(),
                ));
            }
        } else {
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
        Ok(())
    }

    pub fn active(&self) -> Result<&ModelConfig> {
        self.models
            .iter()
            .find(|model| model.id == self.active_model)
            .ok_or_else(|| Error::Config("还没有配置模型。请先在设置里添加。".into()))
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

    /// 写入审批级别，下一轮工具调用生效。
    pub fn set_permission_mode(
        path: impl AsRef<Path>,
        mode: crate::runtime::operation::permissions::PermissionMode,
    ) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            if !value["runtime"].is_object() {
                value["runtime"] =
                    serde_json::to_value(crate::runtime::policy::RuntimeConfig::default())?;
            }
            value["runtime"]["permissionMode"] = serde_json::to_value(mode)?;
            Ok(())
        })
    }

    /// 写入托盘开关，桌面壳据此显隐图标。
    pub fn set_tray_enabled(path: impl AsRef<Path>, enabled: bool) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            if !value["tray"].is_object() {
                value["tray"] = serde_json::json!({});
            }
            value["tray"]["enabled"] = serde_json::Value::Bool(enabled);
            Ok(())
        })
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

    pub fn host_plugin_enabled(&self) -> bool {
        crate::extension::providers::host_plugin_enabled(&self.plugins.disabled)
    }

    pub fn plugin_enabled(&self, id: &str) -> bool {
        crate::extension::providers::plugin_enabled(
            id,
            &self.plugins.enabled,
            &self.plugins.disabled,
        )
    }

    pub fn set_plugin_enabled(path: impl AsRef<Path>, id: &str, enabled: bool) -> Result<()> {
        update_raw(path.as_ref(), |value| {
            if crate::extension::providers::is_host_provider(id) {
                if !value["plugins"].is_object() {
                    value["plugins"] = serde_json::json!({});
                }
                if !value["plugins"]["disabled"].is_array() {
                    value["plugins"]["disabled"] = serde_json::json!([]);
                }
                let disabled = value["plugins"]["disabled"]
                    .as_array_mut()
                    .ok_or_else(|| Error::Config("plugins.disabled must be an array".into()))?;
                disabled.retain(|entry| entry.as_str() != Some(id));
                if !enabled {
                    disabled.push(serde_json::Value::String(id.to_owned()));
                }
                return Ok(());
            }
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

    pub fn set_bridge(path: &Path, bridge: &BridgeConfig) -> Result<()> {
        update_raw(path, |value| {
            let raw_token = value["bridge"]["token"].clone();
            let mut resolved_token = raw_token.clone();
            expand_env(&mut resolved_token)?;
            value["bridge"] = serde_json::to_value(bridge)?;
            if resolved_token == value["bridge"]["token"] {
                value["bridge"]["token"] = raw_token;
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
            let timeout = input
                .get("timeoutMs")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|value| (1000..=600_000).contains(value))
                        .ok_or_else(|| Error::Config("响应超时必须在 1 到 600 秒之间".into()))
                })
                .transpose()?;
            let context_window = input
                .get("contextWindow")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|value| (8_192..=2_000_000).contains(value))
                        .ok_or_else(|| Error::Config("上下文窗口必须在 8k 到 2M token 之间".into()))
                })
                .transpose()?;
            let max_output = input
                .get("maxOutputTokens")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|value| (256..=128_000).contains(value))
                        .ok_or_else(|| {
                            Error::Config("最大输出必须在 256 到 128k token 之间".into())
                        })
                })
                .transpose()?;
            let auth = input
                .get("auth")
                .cloned()
                .filter(|value| matches!(value.as_str(), Some("auto" | "apiKey" | "bearer")));
            let prompt_cache = input
                .get("promptCache")
                .and_then(serde_json::Value::as_bool);
            if let Some(model) = existing {
                if let Some(timeout) = timeout {
                    model["options"]["timeoutMs"] = serde_json::json!(timeout);
                }
                if let Some(window) = context_window {
                    model["options"]["contextWindow"] = serde_json::json!(window);
                }
                if let Some(max_output) = max_output {
                    model["options"]["maxOutputTokens"] = serde_json::json!(max_output);
                }
                if let Some(prompt_cache) = prompt_cache {
                    model["options"]["promptCache"] = serde_json::json!(prompt_cache);
                }
                if let Some(auth) = auth {
                    model["auth"] = auth;
                }
                for field in ["name", "protocol", "model", "baseUrl"] {
                    model[field] = input[field].clone();
                }
                if input["apiKey"].as_str().is_some_and(|key| !key.is_empty()) {
                    model["apiKey"] = input["apiKey"].clone();
                }
            } else {
                let mut options = serde_json::json!({"timeoutMs":timeout.unwrap_or(120_000)});
                if let Some(window) = context_window {
                    options["contextWindow"] = serde_json::json!(window);
                }
                if let Some(max_output) = max_output {
                    options["maxOutputTokens"] = serde_json::json!(max_output);
                }
                if let Some(prompt_cache) = prompt_cache {
                    options["promptCache"] = serde_json::json!(prompt_cache);
                }
                let mut model = serde_json::json!({
                    "id": id,
                    "name": input["name"],
                    "protocol": input["protocol"],
                    "model": input["model"],
                    "baseUrl": input["baseUrl"],
                    "apiKey": input["apiKey"],
                    "headers": {},
                    "options": options
                });
                if let Some(auth) = auth {
                    model["auth"] = auth;
                }
                models.push(model);
            }
            if value["activeModel"].as_str().unwrap_or("").is_empty() {
                value["activeModel"] = serde_json::json!(id);
            }
            Ok(())
        })
    }

    /// 用户技能落点：配置文件旁的 `skills/`，没有就补上。
    pub fn ensure_user_skill_directory(path: impl AsRef<Path>) -> Result<PathBuf> {
        let path = path.as_ref();
        let config = AppConfig::load(path)?;
        let config_dir = path
            .parent()
            .ok_or_else(|| Error::Config("configuration path has no parent".into()))?;
        if let Some(existing) = user_skill_directory(config_dir, &config) {
            fs::create_dir_all(&existing)?;
            return Ok(existing);
        }
        let root = config_dir.join("skills");
        fs::create_dir_all(&root)?;
        update_raw(path, |value| {
            let dirs = value["agent"]["skillDirectories"]
                .as_array_mut()
                .ok_or_else(|| Error::Config("skillDirectories must be an array".into()))?;
            dirs.push(serde_json::json!("./skills"));
            Ok(())
        })?;
        Ok(root)
    }

    pub fn delete_model(path: impl AsRef<Path>, id: &str) -> Result<String> {
        let mut active = String::new();
        update_raw(path.as_ref(), |value| {
            let current_active = value["activeModel"].as_str().unwrap_or("").to_owned();
            let next_active = {
                let models = value["models"]
                    .as_array_mut()
                    .ok_or_else(|| Error::Config("models must be an array".into()))?;
                let before = models.len();
                models.retain(|model| model["id"].as_str() != Some(id));
                if models.len() == before {
                    return Err(Error::Config("模型配置不存在".into()));
                }
                if current_active == id {
                    models
                        .first()
                        .and_then(|model| model["id"].as_str())
                        .unwrap_or("")
                        .to_owned()
                } else {
                    current_active
                }
            };
            value["activeModel"] = serde_json::Value::String(next_active.clone());
            if let Some(fallbacks) = value["agent"]["fallbackModels"].as_array_mut() {
                fallbacks.retain(|entry| {
                    let current = entry.as_str();
                    current != Some(id) && current != Some(next_active.as_str())
                });
            }
            active = next_active;
            Ok(())
        })?;
        Ok(active)
    }
}

pub const USAGE: &str = "\
Sleepy Doll — 面向 BetterGI 的本地桌面 Agent

用法:
  sleepy-doll [配置文件]

配置解析顺序:
  1. 命令行第一个参数（相对路径相对于可执行文件所在目录）
  2. 环境变量 SLEEPY_DOLL_CONFIG
  3. <可执行文件目录>/user/config.json
  4. 安装目录不可写时退回 %APPDATA%\\Sleepy Doll\\user\\config.json

选项:
  -h, --help       显示本帮助
  -V, --version    显示版本";

/// 从进程参数解析配置路径的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// 要加载或生成模板的配置文件。
    Config(PathBuf),
    /// 请求了 `--help`，调用方打印 [`USAGE`]。
    Help,
    /// 请求了 `--version`，调用方打印 [`VERSION`]。
    Version,
}

pub const VERSION: &str = concat!("Sleepy Doll ", env!("CARGO_PKG_VERSION"));

/// 解析配置文件路径。显式参数或 `SLEEPY_DOLL_CONFIG` 优先，否则用安装目录下的
/// `user/`。相对路径以可执行文件所在目录为基准，不用工作目录。
pub fn resolve_path() -> Result<Resolved> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let explicit = env::args_os().nth(1);
    resolve_from(
        explicit.as_deref(),
        env::var_os("SLEEPY_DOLL_CONFIG").as_deref(),
        exe_dir.as_deref(),
        user_data_root(),
        &writable_directory,
    )
}

/// 漫游目录下留给本产品的那一层。安装目录不可写时用户数据落在这里。
const ROAMING_DIRECTORY: &str = "Sleepy Doll";

/// 安装目录不可写时用户数据落到的目录（`%APPDATA%\Sleepy Doll`）。卸载要按同一条
/// 规则找回来。
pub fn fallback_directory() -> Option<PathBuf> {
    user_data_root().map(|root| root.join(ROAMING_DIRECTORY))
}

/// 安装目录不可写时用的用户数据根。
fn user_data_root() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
}

/// 纯解析：不读环境变量，不写文件系统。`is_writable` 由调用方注入。
fn resolve_from(
    explicit: Option<&OsStr>,
    env_config: Option<&OsStr>,
    exe_dir: Option<&Path>,
    data_root: Option<PathBuf>,
    is_writable: &dyn Fn(&Path) -> bool,
) -> Result<Resolved> {
    if let Some(raw) = explicit {
        let text = raw.to_string_lossy();
        match text.as_ref() {
            "-h" | "--help" => return Ok(Resolved::Help),
            "-V" | "--version" => return Ok(Resolved::Version),
            _ if text.starts_with('-') => {
                return Err(Error::Config(format!("未知选项: {text}\n\n{USAGE}")));
            }
            _ => {}
        }
        return Ok(Resolved::Config(explicit_config(raw, exe_dir)?));
    }
    if let Some(raw) = env_config {
        return Ok(Resolved::Config(explicit_config(raw, exe_dir)?));
    }
    if let Some(directory) = exe_dir.map(|dir| dir.join("user"))
        && is_writable(&directory)
    {
        return Ok(Resolved::Config(directory.join("config.json")));
    }
    // 安装位置不可写时退回用户数据根。
    if let Some(directory) = data_root.map(|root| root.join(ROAMING_DIRECTORY).join("user"))
        && is_writable(&directory)
    {
        return Ok(Resolved::Config(directory.join("config.json")));
    }
    Err(Error::Config(
        "找不到可写的配置目录。请把 Sleepy Doll 安装到可写位置，或设置 SLEEPY_DOLL_CONFIG 指向一个可写的 .json 文件。".into(),
    ))
}

fn explicit_config(raw: &OsStr, exe_dir: Option<&Path>) -> Result<PathBuf> {
    let path = PathBuf::from(raw);
    let is_json = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !is_json && !path.exists() {
        return Err(Error::Config(format!(
            "配置文件必须是 .json 文件: {}",
            path.display()
        )));
    }
    if path.is_absolute() {
        return Ok(path);
    }
    let base = exe_dir
        .ok_or_else(|| Error::Config("无法解析相对配置路径：拿不到可执行文件所在目录。".into()))?;
    Ok(base.join(path))
}

/// 判断 `directory` 能否存放用户数据，不创建任何东西。目录不存在时可以接受，
/// 只要它最近的已存在祖先可写 —— 目录随后由 `seed` 创建。
fn writable_directory(directory: &Path) -> bool {
    let mut ancestor = directory;
    while !ancestor.exists() {
        match ancestor.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => ancestor = parent,
            _ => return false,
        }
    }
    let probe = ancestor.join(format!(".sleepy-doll-write-probe-{}", std::process::id()));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 首次运行时把随产品分发的模板写进用户目录。
pub fn seed(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let directory = path
        .parent()
        .ok_or_else(|| Error::Config("configuration path has no parent".into()))?;
    fs::create_dir_all(directory)?;
    // 模板把技能、插件与目录根建在自己旁边。
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

pub(crate) fn atomic_write(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    // 后缀是追加而不是替换，文件名仍以 `.json.<id>.tmp` 结尾，忽略规则按这个匹配。
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temporary = path.with_file_name(format!("{name}.{}.tmp", uuid::Uuid::new_v4()));
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

/// 配置里的这条相对目录是否就是 `../<name>`。按字面比较，不碰文件系统。
fn is_parent_directory(entry: &serde_json::Value, name: &str) -> bool {
    entry
        .as_str()
        .map(|text| text.trim().replace('\\', "/"))
        .is_some_and(|text| text.trim_end_matches('/') == format!("../{name}"))
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

    /// 解析规则经纯函数 `resolve_from` 测试，不读进程环境、可执行文件路径与工作目录。
    fn resolve(
        explicit: Option<&str>,
        env_config: Option<&str>,
        exe_dir: Option<&str>,
        data_root: Option<&str>,
        writable: &[&str],
    ) -> Result<Resolved> {
        let writable = writable.iter().map(PathBuf::from).collect::<Vec<_>>();
        resolve_from(
            explicit.map(OsStr::new),
            env_config.map(OsStr::new),
            exe_dir.map(Path::new),
            data_root.map(PathBuf::from),
            &|directory| writable.iter().any(|allowed| allowed == directory),
        )
    }

    fn config_path(result: Result<Resolved>) -> PathBuf {
        match result.unwrap() {
            Resolved::Config(path) => path,
            other => panic!("expected a configuration path, got {other:?}"),
        }
    }

    #[test]
    fn argument_wins_over_environment_and_joins_the_executable_directory() {
        // 相对参数不以工作目录为基准。
        let path = config_path(resolve(
            Some("dev.json"),
            Some("/env/config.json"),
            Some("/opt/sleepy-doll"),
            Some("/home/user/.local/share"),
            &[],
        ));
        assert_eq!(path, PathBuf::from("/opt/sleepy-doll").join("dev.json"));
    }

    #[test]
    fn absolute_arguments_are_taken_verbatim() {
        let path = config_path(resolve(
            Some("/etc/sleepy-doll/config.json"),
            None,
            Some("/opt/sleepy-doll"),
            None,
            &[],
        ));
        assert_eq!(path, PathBuf::from("/etc/sleepy-doll/config.json"));
    }

    #[test]
    fn environment_is_the_second_choice() {
        let path = config_path(resolve(
            None,
            Some("/env/config.json"),
            Some("/opt/sleepy-doll"),
            None,
            &[],
        ));
        assert_eq!(path, PathBuf::from("/env/config.json"));
    }

    #[test]
    fn help_and_version_are_reported_instead_of_treated_as_paths() {
        for flag in ["-h", "--help"] {
            assert_eq!(
                resolve(Some(flag), None, None, None, &[]).unwrap(),
                Resolved::Help
            );
        }
        for flag in ["-V", "--version"] {
            assert_eq!(
                resolve(Some(flag), None, None, None, &[]).unwrap(),
                Resolved::Version
            );
        }
        assert!(resolve(Some("--unknown"), None, None, None, &[]).is_err());
    }

    #[test]
    fn arguments_that_cannot_be_configuration_files_are_rejected() {
        // 既不是 .json 也不是已存在的文件。
        assert!(resolve(Some("notes.txt"), None, None, None, &[]).is_err());
        // 尚未存在的 .json 仍可作为目标，首次运行会生成模板。
        let path = config_path(resolve(
            Some("fresh.json"),
            None,
            Some("/opt/sleepy-doll"),
            None,
            &[],
        ));
        assert_eq!(path, PathBuf::from("/opt/sleepy-doll").join("fresh.json"));
    }

    #[test]
    fn installation_directory_is_preferred_then_the_per_user_root() {
        let exe = "/opt/sleepy-doll".to_string();
        let install_user = PathBuf::from(&exe).join("user");
        let data_user = PathBuf::from("/home/user/.local/share")
            .join("Sleepy Doll")
            .join("user");

        let path = config_path(resolve(
            None,
            None,
            Some(&exe),
            Some("/home/user/.local/share"),
            &[install_user.to_str().unwrap(), data_user.to_str().unwrap()],
        ));
        assert_eq!(path, install_user.join("config.json"));

        // 安装目录不可写时退到数据根。
        let path = config_path(resolve(
            None,
            None,
            Some(&exe),
            Some("/home/user/.local/share"),
            &[data_user.to_str().unwrap()],
        ));
        assert_eq!(path, data_user.join("config.json"));
    }

    #[test]
    fn resolution_fails_rather_than_falling_back_to_the_working_directory() {
        // 没有任何可写目录时解析失败。
        assert!(resolve(None, None, Some("/opt/sleepy-doll"), None, &[]).is_err());
        assert!(
            resolve(
                None,
                None,
                Some("/opt/sleepy-doll"),
                Some("/home/user/.local/share"),
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn writable_directory_does_not_create_the_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let candidate = directory.path().join("user");
        assert!(writable_directory(&candidate));
        // 解析不产生副作用，目录由 `seed` 创建。
        assert!(!candidate.exists());
        assert!(directory.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn shipped_template_is_seedable_and_valid() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("user").join("config.json");
        seed(&path).unwrap();
        assert!(path.exists());
        let config = AppConfig::load(&path).unwrap();
        assert_eq!(config.active_model, "");
        assert!(config.models.is_empty());
        // 底座规则编译在二进制里（`CORE_AGENT_POLICY`），这个槽位只留给用户自己的话。
        assert_eq!(config.agent.system_prompt, "");
        // 模板把资源根建在配置文件旁。
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
                max_tool_calls_per_turn: 16,
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
            tray: TrayConfig::default(),
        };
        assert!(config.validate().is_err());
    }

    fn write_models_config(directory: &Path, active: &str, models: usize) -> PathBuf {
        let path = directory.join("config.json");
        let entries: Vec<serde_json::Value> = (0..models)
            .map(|index| {
                let id = if index == 0 { "a" } else { "b" };
                serde_json::json!({
                    "id": id,
                    "name": id,
                    "protocol": "openai-chat",
                    "model": "m",
                    "baseUrl": "http://127.0.0.1"
                })
            })
            .collect();
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "activeModel": active,
                "models": entries,
                "agent": {
                    "systemPrompt": "test",
                    "fallbackModels": if models > 1 {
                        serde_json::json!(["b"])
                    } else {
                        serde_json::json!([])
                    }
                },
                "bridge": {"enabled": false, "baseUrl": "http://127.0.0.1"},
                "storage": {"database": directory.join("test.db")}
            }))
            .unwrap(),
        )
        .unwrap();
        path
    }

    #[test]
    fn deletes_the_last_model_and_clears_the_default() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_models_config(directory.path(), "a", 1);
        assert_eq!(AppConfig::delete_model(&path, "a").unwrap(), "");
        let config = AppConfig::load(&path).unwrap();
        assert!(config.models.is_empty());
        assert_eq!(config.active_model, "");
    }

    #[test]
    fn deleting_the_default_model_promotes_another() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_models_config(directory.path(), "a", 2);
        assert_eq!(AppConfig::delete_model(&path, "a").unwrap(), "b");
        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config["activeModel"], "b");
        assert_eq!(config["models"].as_array().unwrap().len(), 1);
        assert!(
            config["agent"]["fallbackModels"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn save_first_model_becomes_the_default() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("user").join("config.json");
        seed(&path).unwrap();
        AppConfig::save_model(
            &path,
            &serde_json::json!({
                "id":"primary","name":"主模型","protocol":"openai-chat","model":"m",
                "baseUrl":"http://127.0.0.1"
            }),
        )
        .unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.active_model, "primary");
        assert_eq!(loaded.models.len(), 1);
    }

    #[test]
    fn save_model_writes_window_and_auth() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_models_config(directory.path(), "a", 1);
        AppConfig::save_model(
            &path,
            &serde_json::json!({
                "id":"a","name":"a","protocol":"anthropic-messages","model":"claude",
                "baseUrl":"http://127.0.0.1","timeoutMs":60000,"contextWindow":32000,
                "maxOutputTokens":4096,"auth":"bearer","promptCache":false
            }),
        )
        .unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.models[0].options.context_window, 32_000);
        assert_eq!(loaded.models[0].options.max_output_tokens, Some(4096));
        assert_eq!(loaded.models[0].auth, ModelAuth::Bearer);
        assert!(!loaded.models[0].options.prompt_cache);
    }

    #[test]
    fn skill_directory_next_to_config_is_user() {
        use crate::extension::skills::SkillSource;
        let config_dir = PathBuf::from("/home/user/sleepy-doll");
        assert_eq!(
            skill_directory_source(&config_dir, &config_dir.join("skills")),
            SkillSource::User
        );
        assert_eq!(
            skill_directory_source(&config_dir, &PathBuf::from("/opt/Sleepy-Doll/skills")),
            SkillSource::Product
        );
    }

    /// v3 迁移：旧配置里产品技能目录还单独列着、产品插件目录还没列。
    #[test]
    fn version_three_moves_the_domain_skills_into_the_plugin() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "version": 3,
                "activeModel": "",
                "models": [],
                "agent": {
                    "systemPrompt": "test",
                    "skillDirectories": ["..\\skills", "./skills"],
                },
                "bridge": {"enabled": false, "baseUrl": "http://127.0.0.1"},
                "plugins": {"directories": ["./plugins"], "enabled": []},
                "storage": {"database": "./test.db"}
            }))
            .unwrap(),
        )
        .unwrap();

        // 载入后的形态：相对目录已经解析成绝对路径，形态由平台决定。
        let tail = |path: &Path| path.to_string_lossy().replace('\\', "/");
        let loaded = AppConfig::load(&path).unwrap();
        assert_eq!(loaded.version, 4);
        assert_eq!(loaded.agent.skill_directories.len(), 1);
        assert!(tail(&loaded.agent.skill_directories[0]).ends_with("/skills"));
        assert_eq!(loaded.plugins.directories.len(), 2);
        assert!(tail(&loaded.plugins.directories[1]).ends_with("/plugins"));

        // 写回文件的内容：产品根在前，用户根在后。
        let written: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(written["version"], serde_json::json!(4));
        assert_eq!(
            written["agent"]["skillDirectories"],
            serde_json::json!(["./skills"])
        );
        assert_eq!(
            written["plugins"]["directories"],
            serde_json::json!(["../plugins", "./plugins"])
        );

        // 迁移按文件声明的版本执行一次，第二次加载不再改动。
        let again = AppConfig::load(&path).unwrap();
        assert_eq!(again.plugins.directories, loaded.plugins.directories);
        assert_eq!(
            again.agent.skill_directories,
            loaded.agent.skill_directories
        );
    }
}
