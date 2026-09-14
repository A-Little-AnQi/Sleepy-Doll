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
    /// 该模型的上下文窗口（token）。运行时的上下文预算由它推导，而不是写死一个
    /// 与模型无关的常数 —— 200k 窗口的模型和 32k 窗口的模型不该共用一个上限。
    #[serde(default = "default_context_window")]
    pub context_window: u64,
}

/// 当前主流模型的窗口量级。配置里按实际模型改。
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
/// 一轮里允许的调用数。列一个目录再逐个读文件是很自然的批次（11 个配置组
/// 就是 11 次读），上限压得太低会把一个批次劈成两轮，模型只能重发一遍。
/// 单次运行的总量由 `runtime.maxTools` 约束。
const fn default_max_tools() -> usize {
    16
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
    pub hooks: Vec<crate::runtime::host::hooks::HttpHookConfig>,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path)?)?;
        // 按文件声明的版本判断，而不是按前一步刚改成的版本 —— 否则 v1 配置在同
        // 一次加载里会被连续降级两次，把显式写下的选择一起冲掉。
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
        }
        // 版本 2 之前默认逐项审批，那是写进文件里的旧默认值，不是用户的选择。
        // 产品默认已经改成「按实际影响确认」，这里跟着迁移一次；显式选择逐项
        // 审批的用户把 permissionMode 改回 askEach 即可，之后不会再被覆盖。
        if declared == 2 && value["runtime"]["permissionMode"] == serde_json::json!("askEach") {
            value["version"] = serde_json::json!(3);
            value["runtime"]["permissionMode"] = serde_json::json!("standard");
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
        if !(1..=3).contains(&self.version) {
            return Err(Error::Config("version must be 1, 2 or 3".into()));
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

    /// 写入审批级别。用户随时可改，立即生效下一轮工具调用。
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
            if let Some(model) = existing {
                if let Some(timeout) = timeout {
                    model["options"]["timeoutMs"] = serde_json::json!(timeout);
                }
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
                    "options": {"timeoutMs":timeout.unwrap_or(120_000)}
                }));
            }
            Ok(())
        })
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

/// Outcome of resolving the configuration path from process arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The configuration file to load or seed.
    Config(PathBuf),
    /// `--help` was requested; the caller prints [`USAGE`].
    Help,
    /// `--version` was requested; the caller prints [`VERSION`].
    Version,
}

pub const VERSION: &str = concat!("Sleepy Doll ", env!("CARGO_PKG_VERSION"));

/// Resolves the configuration file. An explicit argument or `SLEEPY_DOLL_CONFIG`
/// always wins; otherwise the file lives in the `user/` directory of the
/// installation. Relative paths are resolved against the executable's own
/// directory, and the working directory is never used, so a stray command line
/// argument can no longer redirect user data into wherever the process happened
/// to start.
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

/// The per-user data root used when the installation directory is read-only.
fn user_data_root() -> Option<PathBuf> {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| {
            env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
        })
}

/// Pure resolution: no environment access, no filesystem writes. `is_writable`
/// is injected so the ordering rules can be tested without touching either.
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
    // Read-only installation locations fall back to the per-user data root
    // rather than refusing to start.
    if let Some(directory) = data_root.map(|root| root.join("Sleepy Doll").join("user"))
        && is_writable(&directory)
    {
        return Ok(Resolved::Config(directory.join("config.json")));
    }
    // No working-directory fallback: silently writing user data next to whatever
    // directory the process started in is exactly the behaviour this module
    // promises to avoid.
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

/// Reports whether `directory` can hold user data, without creating anything.
/// A missing directory is acceptable as long as its nearest existing ancestor
/// is writable, because `seed` creates the directory afterwards.
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

pub(crate) fn atomic_write(path: &Path, value: &serde_json::Value) -> Result<()> {
    use std::io::Write;
    // The suffix is appended rather than substituted so the name still ends in
    // `.json.<id>.tmp`, which is what the ignore rules match on.
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

    /// Resolution rules are exercised through the pure `resolve_from`, so these
    /// tests never read the process environment, the executable path or the
    /// working directory.
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
        // A relative argument must never be anchored to the working directory:
        // that is what let a stray argument create a `--help` file plus an
        // entire data tree wherever the process happened to start.
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
        // Not a `.json` and not an existing file.
        assert!(resolve(Some("notes.txt"), None, None, None, &[]).is_err());
        // A `.json` name that does not exist yet is still a usable target,
        // because the first run seeds it.
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

        // A read-only installation directory falls through to the data root.
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
        // Nothing is writable and no per-user root exists: the old code returned
        // the relative path `user/config.json`, silently writing user data into
        // whatever directory the process was started from.
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
        // Resolution may not have side effects; `seed` creates the directory.
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
        };
        assert!(config.validate().is_err());
    }
}
