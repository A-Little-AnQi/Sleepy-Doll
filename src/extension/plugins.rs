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
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 应用注入的本地 MCP 服务标识，不允许从清单指定任意 Rust 实现。
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// 按 MCP 服务的原始工具名索引。
    #[serde(default)]
    pub tool_execution: HashMap<String, ToolExecution>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum McpTransport {
    #[default]
    Stdio,
    StreamableHttp,
    InProcess,
}

impl McpServerManifest {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() || self.timeout_ms == 0 {
            return Err(Error::Config(
                "MCP id 不能为空，timeoutMs 必须大于零".into(),
            ));
        }
        let valid = match self.transport {
            McpTransport::Stdio => {
                !self.command.trim().is_empty()
                    && self.url.is_none()
                    && self.server.is_none()
                    && self.headers.is_empty()
            }
            McpTransport::StreamableHttp => {
                self.command.is_empty()
                    && self.args.is_empty()
                    && self.server.is_none()
                    && self.url.as_deref().is_some_and(|url| {
                        url::Url::parse(url).is_ok_and(|url| {
                            matches!(url.scheme(), "http" | "https")
                                && url.host_str().is_some()
                                && url.username().is_empty()
                                && url.password().is_none()
                                && url.fragment().is_none()
                        })
                    })
            }
            McpTransport::InProcess => {
                self.command.is_empty()
                    && self.args.is_empty()
                    && self.url.is_none()
                    && self.headers.is_empty()
                    && self.server.as_deref().is_some_and(|id| !id.is_empty())
            }
        };
        if !valid {
            return Err(Error::Config("MCP 传输配置无效或混用了多种传输字段".into()));
        }
        Ok(())
    }
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
    mcp_servers: HashMap<String, crate::extension::mcp_server::RegistryServer>,
    adapters: Vec<std::sync::Arc<AdapterClient>>,
    broker_specs: Vec<(String, Vec<PathBuf>)>,
}

impl PluginManager {
    pub fn provide_mcp(&mut self, id: &str, server: crate::extension::mcp_server::RegistryServer) {
        self.mcp_servers.insert(id.into(), server);
    }

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
            let client = if server.transport == McpTransport::InProcess {
                let key = server.server.as_deref().unwrap_or_default();
                let implementation = self
                    .mcp_servers
                    .get(key)
                    .filter(|_| key == manifest.id)
                    .ok_or_else(|| Error::Config("插件引用的本地 MCP 服务不可用".into()))?;
                McpClient::in_process(server, implementation.clone())?
            } else {
                McpClient::start(server)?
            };
            if let Some(instructions) = client.instructions() {
                skills.load_mcp_instructions(&manifest.id, &server.id, &instructions)?;
            }
            tools.register_mcp(&manifest.id, &server.id, client.clone());
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
        .http_status_as_error(false)
        .build()
        .new_agent();
    let headers = definition
        .headers
        .iter()
        .map(|(name, value)| (name.as_str(), expand_env(value)))
        .collect::<Vec<_>>();
    let mut response = if definition.method.eq_ignore_ascii_case("GET") {
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
        request.call()?
    } else if definition.method.eq_ignore_ascii_case("POST") {
        let mut request = agent.post(&definition.url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        request.send_json(arguments)?
    } else {
        return Err(Error::Config(format!(
            "插件 HTTP 方法不支持：{}",
            definition.method
        )));
    };
    let status = response.status();
    if status.is_success() {
        Ok(response.body_mut().read_json()?)
    } else {
        let detail = response
            .body_mut()
            .with_config()
            .limit(16 * 1024)
            .read_to_string()
            .unwrap_or_default();
        // 不包含请求 URL 或标头；桥的结构化业务错误须保留，避免误判为断线。
        let message = format!(
            "插件 HTTP {}：{}",
            status.as_u16(),
            detail.chars().take(4096).collect::<String>()
        );
        // 只信明确的拒绝提交回执；HTTP 状态、executed:false 或普通错误均不足以
        // 证明服务没有接纳异步任务。未携带回执的写入错误仍按结果未知处理。
        let receipt = serde_json::from_str::<Value>(&detail).ok();
        let rejected = receipt.as_ref().is_some_and(|value| {
            value["submissionStatus"] == "notSubmitted"
                && value["error"].is_object()
                && value["accepted"] != true
                && value.get("jobId").is_none()
        });
        Err(if rejected {
            Error::NotSubmitted(
                receipt
                    .as_ref()
                    .and_then(|value| value.pointer("/error/message").and_then(Value::as_str))
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| text.chars().take(4096).collect())
                    .unwrap_or(message),
            )
        } else {
            Error::Tool(message)
        })
    }
}

pub(crate) fn expand_env(value: &str) -> String {
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
mod http_reply_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn reply(status: u16, body: &str) -> Result<Value> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_owned();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.extend(byte);
                assert!(request.len() < 8192);
            }
            write!(
                stream,
                "HTTP/1.1 {status} Reply\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let result = call_http(
            &HttpToolManifest {
                name: "fixture".into(),
                title: String::new(),
                description: String::new(),
                input_schema: serde_json::json!({"type":"object"}),
                output_schema: None,
                method: "GET".into(),
                url: format!("http://{address}/fixture"),
                headers: HashMap::new(),
                timeout_ms: 2000,
                execution: ToolExecution::read_only(),
            },
            &serde_json::json!({}),
        );
        server.join().unwrap();
        result
    }

    #[test]
    fn plugin_http_preserves_conflict_body() {
        let error = reply(
            409,
            r#"{"error":{"code":"revision_conflict","message":"配置已变化，请重新读取"}}"#,
        )
        .unwrap_err();
        let Error::Tool(message) = error else {
            panic!("业务拒绝应保留为工具错误");
        };
        assert!(message.contains("409"));
        assert!(message.contains("revision_conflict"));
        assert!(message.contains("配置已变化"));
    }

    #[test]
    fn plugin_http_requires_explicit_not_submitted_receipt() {
        assert!(matches!(
            reply(
                409,
                r#"{"submissionStatus":"notSubmitted","error":{"message":"已拒绝"}}"#
            ),
            Err(Error::NotSubmitted(_))
        ));
        for body in [
            r#"{"executed":false,"error":{"message":"结果未知"}}"#,
            r#"{"submissionStatus":"notSubmitted","accepted":true,"error":{}}"#,
            r#"{"submissionStatus":"notSubmitted","jobId":"pending","error":{}}"#,
            r#"{"submissionStatus":"notSubmitted"}"#,
        ] {
            assert!(matches!(reply(409, body), Err(Error::Tool(_))), "{body}");
        }
    }

    #[test]
    fn plugin_http_keeps_success_shape_and_bounds_error_body() {
        assert_eq!(
            reply(200, r#"{"accepted":true}"#).unwrap(),
            serde_json::json!({"accepted":true})
        );
        let error = reply(400, &"x".repeat(8000)).unwrap_err().to_string();
        assert!(error.len() < 5000);
    }
}
