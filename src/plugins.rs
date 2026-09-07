use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::adapter::AdapterClient;
use crate::{
    error::{Error, Result},
    mcp::McpClient,
    skills::SkillRegistry,
    tools::{FunctionTool, ToolExecution, ToolRegistry},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpToolManifest {
    pub name: String,
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
    /// Policies are keyed by the MCP server's original tool name. Omitting an
    /// entry keeps the tool fail-closed as an unknown-effect, serial tool.
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginStatus {
    pub manifest: PluginManifest,
    pub status: String,
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
    pub fn load(
        &mut self,
        directories: &[PathBuf],
        enabled: &[String],
        tools: &mut ToolRegistry,
        skills: &mut SkillRegistry,
    ) -> Result<()> {
        let enabled = enabled.iter().cloned().collect::<HashSet<_>>();
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
                            status: "failed".into(),
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
                        status: "failed".into(),
                        error: Some("invalid schemaVersion, empty id, or duplicate id".into()),
                    });
                    continue;
                }
                if !enabled.contains(&manifest.id) {
                    self.plugins.push(PluginStatus {
                        manifest,
                        status: "disabled".into(),
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
                if result.is_ok() {
                    *tools = next_tools;
                    *skills = next_skills;
                } else {
                    self.mcp_clients.truncate(client_count);
                    self.adapters.truncate(adapter_count);
                    self.broker_specs.truncate(broker_count);
                }
                self.plugins.push(PluginStatus {
                    manifest,
                    status: if result.is_ok() { "enabled" } else { "failed" }.into(),
                    error: result.err().map(|error| error.to_string()),
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
                    format!("plugin:{}", manifest.id),
                )
            })
            .collect::<Vec<_>>();
        skills.load(&skill_paths)?;
        for definition in &manifest.http_tools {
            definition.execution.validate()?;
            let definition = definition.clone();
            let source = format!("plugin:{}:http", manifest.id);
            let name = format!("{}.http.{}", manifest.id, definition.name);
            let description = definition.description.clone();
            let schema = definition.input_schema.clone();
            let output_schema = definition.output_schema.clone();
            let execution = definition.execution.clone();
            tools.register(
                FunctionTool::new(name, description, schema, source, move |arguments| {
                    call_http(&definition, arguments)
                })
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
                    "Adapter id, version and command are required".into(),
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
                    .ok_or_else(|| Error::Tool("Adapter tool missing name".into()))?;
                let execution: ToolExecution =
                    serde_json::from_value(value.get("execution").cloned().unwrap_or_default())?;
                execution.validate()?;
                if execution.effect != crate::tools::ToolEffect::ReadOnly {
                    return Err(Error::Config(
                        "Adapter 工具只能读取、诊断或生成 MutationPlan；实际副作用必须由 Core Broker 提交"
                            .into(),
                    ));
                }
                let source = format!("plugin:{}:adapter:{}", manifest.id, adapter.id);
                let name = format!("{}.adapter.{}", manifest.id, remote_name);
                let description = value["description"]
                    .as_str()
                    .unwrap_or("Adapter capability")
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
        serde_json::json!(self.plugins.iter().map(|p|serde_json::json!({"id":p.manifest.id,"name":p.manifest.name,"version":p.manifest.version,"description":p.manifest.description,"status":p.status,"toolNames":p.manifest.http_tools.iter().map(|t|&t.name).collect::<Vec<_>>(),"adapterIds":p.manifest.adapters.iter().map(|a|&a.id).collect::<Vec<_>>()})).collect::<Vec<_>>())
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
            "unsupported plugin HTTP method: {}",
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
        version: "unknown".into(),
        description: String::new(),
        skills: vec![],
        http_tools: vec![],
        mcp_servers: vec![],
        adapters: vec![],
    }
}
