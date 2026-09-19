use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::runtime::host::adapter::AdapterClient;
use crate::{
    error::{Error, Result},
    extension::mcp::McpClient,
    extension::providers,
    extension::skills::{SkillRegistry, SkillSource},
    extension::{FunctionTool, ToolExecution, ToolRegistry},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpToolManifest {
    pub name: String,
    /// 面向用户的名字，界面在执行记录里显示它；省略时退回工具名。
    #[serde(default)]
    pub title: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub output_schema: Option<Value>,
    #[serde(default = "default_post")]
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub execution: ToolExecution,
}

fn default_post() -> String {
    "POST".into()
}
const fn default_timeout() -> u64 {
    30_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServerManifest {
    pub id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// 按 MCP 服务的原始工具名索引。
    #[serde(default)]
    pub tool_execution: HashMap<String, ToolExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterManifest {
    pub id: String,
    pub version: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_adapter_response")]
    pub max_response_bytes: usize,
    #[serde(default)]
    pub resource_roots: Vec<PathBuf>,
}

const fn default_adapter_response() -> usize {
    4 * 1024 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<PathBuf>,
    #[serde(default)]
    pub http_tools: Vec<HttpToolManifest>,
    #[serde(default)]
    pub mcp_servers: Vec<McpServerManifest>,
    #[serde(default)]
    pub adapters: Vec<AdapterManifest>,
}

/// 插件在一次加载之后的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// 清单里的技能与工具都已登记。
    Enabled,
    /// 配置里没启用，只读到了清单。
    Disabled,
    /// 清单无效，或装载中途失败，改动已回滚。
    Failed,
}

impl PluginState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Failed => "failed",
        }
    }
}

impl Serialize for PluginState {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub manifest: PluginManifest,
    pub state: PluginState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Default)]
pub struct PluginManager {
    plugins: Vec<PluginStatus>,
    mcp_clients: Vec<std::sync::Arc<McpClient>>,
    adapters: Vec<std::sync::Arc<AdapterClient>>,
    broker_specs: Vec<(String, Vec<PathBuf>)>,
}

impl PluginManager {
    /// 目录按顺序扫描，先出现的占住 id，重复 id 记失败。
    pub fn load(
        &mut self,
        directories: &[PathBuf],
        enabled: &[String],
        disabled: &[String],
        tools: &mut ToolRegistry,
        skills: &mut SkillRegistry,
    ) -> Result<()> {
        let mut ids = HashSet::new();
        for root in directories {
            if !root.exists() {
                continue;
            }
            for entry in fs::read_dir(root)? {
                let entry = entry?;
                if !entry.path().is_dir() {
                    continue;
                }
                let directory = entry.path();
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let path = directory.join(".sleepy-doll-plugin/plugin.json");
                if !path.exists() {
                    continue;
                }
                let manifest: PluginManifest = match fs::read_to_string(&path)
                    .map_err(Error::from)
                    .and_then(|text| serde_json::from_str(&text).map_err(Error::from))
                {
                    Ok(value) => value,
                    Err(error) => {
                        self.plugins.push(PluginStatus {
                            manifest: failed_manifest(&directory),
                            state: PluginState::Failed,
                            error: Some(error.to_string()),
                        });
                        continue;
                    }
                };
                if manifest.schema_version != 1
                    || manifest.id.is_empty()
                    || !ids.insert(manifest.id.clone())
                {
                    self.plugins.push(PluginStatus {
                        manifest,
                        state: PluginState::Failed,
                        error: Some("schemaVersion 无效、id 为空或 id 重复".into()),
                    });
                    continue;
                }
                if !providers::plugin_enabled(&manifest.id, enabled, disabled) {
                    log::info!("插件 {} 已停用，本次不加载", manifest.id);
                    self.plugins.push(PluginStatus {
                        manifest,
                        state: PluginState::Disabled,
                        error: None,
                    });
                    continue;
                }
                let mut next_tools = tools.clone();
                let mut next_skills = skills.clone();
                let client_count = self.mcp_clients.len();
                let adapter_count = self.adapters.len();
                let broker_count = self.broker_specs.len();
                let result = self.enable(&directory, &manifest, &mut next_tools, &mut next_skills);
                let added = (
                    next_skills.len().saturating_sub(skills.len()),
                    next_tools.len().saturating_sub(tools.len()),
                );
                if result.is_ok() {
                    *tools = next_tools;
                    *skills = next_skills;
                } else {
                    self.mcp_clients.truncate(client_count);
                    self.adapters.truncate(adapter_count);
                    self.broker_specs.truncate(broker_count);
                }
                let error = result.err().map(|error| error.to_string());
                match &error {
                    None => log::info!(
                        "插件 {} {} 已加载：{} 个技能，{} 个工具",
                        manifest.id,
                        manifest.version,
                        added.0,
                        added.1
                    ),
                    Some(error) => log::warn!("插件 {} 加载失败：{error}", manifest.id),
                }
                self.plugins.push(PluginStatus {
                    manifest,
                    state: if error.is_none() {
                        PluginState::Enabled
                    } else {
                        PluginState::Failed
                    },
                    error,
                });
            }
        }
        Ok(())
    }

    fn enable(
        &mut self,
        directory: &Path,
        manifest: &PluginManifest,
        tools: &mut ToolRegistry,
        skills: &mut SkillRegistry,
    ) -> Result<()> {
        let skill_paths = manifest
            .skills
            .iter()
            .map(|path| {
                (
                    if path.is_absolute() {
                        path.clone()
                    } else {
                        directory.join(path)
                    },
                    SkillSource::Plugin(manifest.id.clone()),
                )
            })
            .collect::<Vec<_>>();
        skills.load(&skill_paths)?;
        for definition in &manifest.http_tools {
            definition.execution.validate()?;
            let definition = definition.clone();
            let source = format!("plugin:{}:http", manifest.id);
            let name = format!("{}.http.{}", manifest.id, definition.name);
            let label = definition.title.clone();
            let description = definition.description.clone();
            let schema = definition.input_schema.clone();
            let output_schema = definition.output_schema.clone();
            let execution = definition.execution.clone();
            tools.register(
                FunctionTool::new(name, description, schema, source, move |arguments| {
                    call_http(&definition, arguments)
                })
                .with_label(label)
                .with_output_schema(output_schema)
                .with_execution(execution)
                .with_provider_version(Some(manifest.version.clone())),
            )?;
        }
        for server in &manifest.mcp_servers {
            for execution in server.tool_execution.values() {
                execution.validate()?;
            }
            let client = McpClient::start(server)?;
            for tool in client.list_tools(
                &manifest.id,
                &server.id,
                &manifest.version,
                &server.tool_execution,
            )? {
                tools.register_shared(tool)?;
            }
            self.mcp_clients.push(client);
        }
        for adapter in &manifest.adapters {
            if adapter.id.trim().is_empty()
                || adapter.version.trim().is_empty()
                || adapter.command.trim().is_empty()
            {
                return Err(Error::Config(
                    "Adapter 的 id、version 与 command 必填".into(),
                ));
            }
            let client = AdapterClient::start(&manifest.id, adapter)?;
            let capabilities = client.capabilities()?;
            client.set_hook_events(
                capabilities["hookEvents"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
            for value in capabilities["tools"].as_array().into_iter().flatten() {
                let remote_name = value["name"]
                    .as_str()
                    .ok_or_else(|| Error::Tool("Adapter 工具缺少 name".into()))?;
                let execution: ToolExecution =
                    serde_json::from_value(value.get("execution").cloned().unwrap_or_default())?;
                execution.validate()?;
                if execution.effect != crate::extension::ToolEffect::ReadOnly {
                    return Err(Error::Config(
                        "Adapter 工具只能读取、诊断或生成 MutationPlan，副作用由 Core Broker 提交"
                            .into(),
                    ));
                }
                let source = format!("plugin:{}:adapter:{}", manifest.id, adapter.id);
                let name = format!("{}.adapter.{}", manifest.id, remote_name);
                let label = value["title"].as_str().unwrap_or_default().to_owned();
                let description = value["description"]
                    .as_str()
                    .unwrap_or("Adapter 能力")
                    .to_owned();
                let input_schema = value["inputSchema"].clone();
                let output_schema = value
                    .get("outputSchema")
                    .filter(|schema| !schema.is_null())
                    .cloned();
                let adapter_client = client.clone();
                let remote_name = remote_name.to_owned();
                tools.register(
                    FunctionTool::new(name, description, input_schema, source, move |arguments| {
                        adapter_client.request(
                            "tools/call",
                            serde_json::json!({"name":remote_name,"arguments":arguments}),
                        )
                    })
                    .with_label(label)
                    .with_output_schema(output_schema)
                    .with_execution(execution)
                    .with_provider_version(Some(manifest.version.clone())),
                )?;
            }
            self.adapters.push(client);
            let roots = adapter
                .resource_roots
                .iter()
                .map(|root| {
                    if root.is_absolute() {
                        root.clone()
                    } else {
                        directory.join(root)
                    }
                })
                .collect::<Vec<_>>();
            if !roots.is_empty() {
                self.broker_specs
                    .push((format!("{}/{}", manifest.id, adapter.id), roots));
            }
        }
        Ok(())
    }

    pub fn list(&self) -> &[PluginStatus] {
        &self.plugins
    }
    pub fn public_list(&self) -> Value {
        serde_json::json!(self.plugins.iter().map(|p|serde_json::json!({"id":p.manifest.id,"name":p.manifest.name,"version":p.manifest.version,"description":p.manifest.description,"status":p.state.as_str(),"toolNames":p.manifest.http_tools.iter().map(|t|&t.name).collect::<Vec<_>>(),"adapterIds":p.manifest.adapters.iter().map(|a|&a.id).collect::<Vec<_>>()})).collect::<Vec<_>>())
    }

    pub fn adapters(&self) -> &[std::sync::Arc<AdapterClient>] {
        &self.adapters
    }
    pub fn broker_specs(&self) -> &[(String, Vec<PathBuf>)] {
        &self.broker_specs
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        for adapter in &self.adapters {
            let _ = adapter.shutdown();
        }
    }
}

fn call_http(definition: &HttpToolManifest, arguments: &Value) -> Result<Value> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(definition.timeout_ms)))
        .build()
        .new_agent();
    let headers = definition
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), expand_env(value)))
        .collect::<Vec<_>>();
    if definition.method.eq_ignore_ascii_case("GET") {
        let pairs = arguments
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, value)| {
                (
                    name.as_str(),
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                )
            })
            .collect::<Vec<_>>();
        let mut request = agent
            .get(&definition.url)
            .query_pairs(pairs.iter().map(|(name, value)| (*name, value.as_str())));
        for (name, value) in headers {
            request = request.header(name, value);
        }
        Ok(request.call()?.body_mut().read_json()?)
    } else if definition.method.eq_ignore_ascii_case("POST") {
        let mut request = agent.post(&definition.url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        Ok(request.send_json(arguments)?.body_mut().read_json()?)
    } else {
        Err(Error::Config(format!(
            "插件 HTTP 方法不支持：{}",
            definition.method
        )))
    }
}

fn expand_env(value: &str) -> String {
    value
        .strip_prefix("${ENV:")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(|name| std::env::var(name).unwrap_or_default())
        .unwrap_or_else(|| value.to_owned())
}

fn failed_manifest(directory: &Path) -> PluginManifest {
    let id = directory
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("invalid-plugin")
        .to_owned();
    PluginManifest {
        schema_version: 1,
        id: id.clone(),
        name: id,
        version: "未知".into(),
        description: String::new(),
        skills: vec![],
        http_tools: vec![],
        mcp_servers: vec![],
        adapters: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 产品自带的插件清单里，id 决定工具归哪个提供方，版本决定界面显示什么。
    #[test]
    fn the_shipped_plugin_matches_the_host_provider() {
        let manifest: PluginManifest = serde_json::from_str(include_str!(
            "../../plugins/bgi/.sleepy-doll-plugin/plugin.json"
        ))
        .unwrap();
        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.id, providers::HOST_PROVIDER);
        assert_eq!(manifest.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.skills, [PathBuf::from("./skills")]);
    }

    /// 产品自带的能力包是 `plugins\bgi`：技能随插件一起进出注册表。
    #[test]
    fn the_shipped_plugin_owns_its_skills() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let load = |disabled: &[String]| {
            let mut skills = SkillRegistry::default();
            let mut tools = ToolRegistry::default();
            PluginManager::default()
                .load(
                    std::slice::from_ref(&root),
                    &[],
                    disabled,
                    &mut tools,
                    &mut skills,
                )
                .expect("shipped plugin loads");
            skills
        };

        let skills = load(&[]);
        assert_eq!(skills.list().len(), 2);
        // 界面上只有插件，没有插件里的手册。
        assert!(skills.standalone().is_empty());
        for name in ["bgi-assistant", "bgi-operator"] {
            let skill = skills.get(name).expect(name);
            assert!(skill.always_load, "{name} 应当随插件装载自动加载");
            assert_eq!(skill.source.plugin(), Some(providers::HOST_PROVIDER));
        }

        assert!(
            load(&[providers::HOST_PROVIDER.to_owned()])
                .list()
                .is_empty()
        );
    }
}
