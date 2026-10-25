//! 官方 MCP SDK 客户端：stdio、Streamable HTTP 与应用内标准 JSON-RPC 字节流。
use crate::{
    error::{Error, Result},
    extension::{
        Tool, ToolDefinition, ToolExecution,
        mcp_server::{EXECUTION_META, RegistryServer},
        plugins::{McpServerManifest, McpTransport},
    },
};
use rmcp::{
    ClientLifecycleMode, ClientServiceExt, RoleClient, ServiceExt,
    model::{ClientCapabilities, ClientConfig, ClientRequest, Implementation, ProtocolVersion},
    service::{PeerRequestOptions, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::process::{Child, Command};
use tokio_util::sync::CancellationToken;

const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;

pub struct McpClient {
    session: RunningService<RoleClient, ClientConfig>,
    timeout: Duration,
    trusted_local: bool,
    _child: Option<Mutex<Child>>,
    _constraint: Option<crate::runtime::host::process::ProcessConstraint>,
    server_task: Option<tokio::task::JoinHandle<()>>,
}

impl McpClient {
    pub fn start(manifest: &McpServerManifest) -> Result<Arc<Self>> {
        manifest.validate()?;
        crate::runtime::executor().block_on(async {
            let timeout = Duration::from_millis(manifest.timeout_ms);
            let (session, child, constraint) = match manifest.transport {
                McpTransport::Stdio => {
                    let mut command = Command::new(&manifest.command);
                    command
                        .args(&manifest.args)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .kill_on_drop(true);
                    crate::runtime::host::process::isolate_environment(&mut command);
                    crate::runtime::host::process::hide_console(&mut command);
                    let mut child = command.spawn()?;
                    let constraint = crate::runtime::host::process::constrain_process(&child)?;
                    let input = child
                        .stdin
                        .take()
                        .ok_or_else(|| Error::Tool("MCP stdin 不可用".into()))?;
                    let output = child
                        .stdout
                        .take()
                        .ok_or_else(|| Error::Tool("MCP stdout 不可用".into()))?;
                    let session = tokio::time::timeout(
                        timeout,
                        client_config()
                            .serve_with_lifecycle((output, input), ClientLifecycleMode::Initialize),
                    )
                    .await
                    .map_err(|_| Error::Tool("MCP 握手超时".into()))?
                    .map_err(|_| Error::Tool("MCP 握手失败".into()))?;
                    (session, Some(Mutex::new(child)), Some(constraint))
                }
                McpTransport::StreamableHttp => {
                    if rustls::crypto::CryptoProvider::get_default().is_none() {
                        let _ = rustls::crypto::ring::default_provider().install_default();
                    }
                    let mut config = StreamableHttpClientTransportConfig::with_uri(
                        manifest.url.clone().unwrap(),
                    );
                    config.max_sse_event_size = MAX_RESULT_BYTES;
                    for (name, value) in &manifest.headers {
                        config.custom_headers.insert(
                            name.parse()
                                .map_err(|_| Error::Config("MCP HTTP 标头名称无效".into()))?,
                            super::plugins::expand_env(value)
                                .parse()
                                .map_err(|_| Error::Config("MCP HTTP 标头值无效".into()))?,
                        );
                    }
                    let transport = StreamableHttpClientTransport::from_config(config);
                    let session = tokio::time::timeout(
                        timeout,
                        client_config()
                            .serve_with_lifecycle(transport, ClientLifecycleMode::Initialize),
                    )
                    .await
                    .map_err(|_| Error::Tool("MCP HTTP 握手超时".into()))?
                    .map_err(|_| Error::Tool("MCP HTTP 握手失败".into()))?;
                    (session, None, None)
                }
                McpTransport::InProcess => {
                    return Err(Error::Config("本地 MCP 服务必须由应用提供".into()));
                }
            };
            Ok(Arc::new(Self {
                session,
                timeout,
                trusted_local: false,
                _child: child,
                _constraint: constraint,
                server_task: None,
            }))
        })
    }

    pub fn in_process(manifest: &McpServerManifest, server: RegistryServer) -> Result<Arc<Self>> {
        manifest.validate()?;
        crate::runtime::executor().block_on(async {
            // 管道传输 SDK 编解码的 MCP JSON-RPC，不直接调用服务端 Rust 方法。
            let (client_io, server_io) = tokio::io::duplex(64 * 1024);
            let task = tokio::spawn(async move {
                if let Ok(service) = server.serve(server_io).await {
                    let _ = service.waiting().await;
                }
            });
            let timeout = Duration::from_millis(manifest.timeout_ms);
            let result = tokio::time::timeout(
                timeout,
                client_config().serve_with_lifecycle(client_io, ClientLifecycleMode::Initialize),
            )
            .await;
            match result {
                Ok(Ok(session)) => Ok(Arc::new(Self {
                    session,
                    timeout,
                    trusted_local: true,
                    _child: None,
                    _constraint: None,
                    server_task: Some(task),
                })),
                _ => {
                    task.abort();
                    Err(Error::Tool("本地 MCP 服务握手失败".into()))
                }
            }
        })
    }

    pub fn instructions(&self) -> Option<String> {
        self.session
            .peer_info()
            .and_then(|info| info.instructions.clone())
    }

    pub fn list_tools(
        self: &Arc<Self>,
        plugin_id: &str,
        server_id: &str,
        plugin_version: &str,
        execution: &HashMap<String, ToolExecution>,
    ) -> Result<Vec<Arc<dyn Tool>>> {
        let mut tools = Vec::new();
        let mut cursor = None::<String>;
        let mut seen = HashSet::new();
        loop {
            let result = self.request(
                "tools/list",
                cursor
                    .as_ref()
                    .map(|c| json!({"cursor":c}))
                    .unwrap_or_else(|| json!({})),
            )?;
            for value in result["tools"]
                .as_array()
                .ok_or_else(|| Error::Tool("MCP 未返回 tools".into()))?
            {
                if self.trusted_local && value["_meta"][super::mcp_server::INTERNAL_META] == true {
                    continue;
                }
                let remote_name = value["name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .ok_or_else(|| Error::Tool("MCP 工具缺少 name".into()))?
                    .to_owned();
                let output_schema = value.get("outputSchema").filter(|s| !s.is_null()).cloned();
                if let Some(schema) = &output_schema {
                    jsonschema::validator_for(schema)
                        .map_err(|_| Error::Tool("MCP outputSchema 无效".into()))?;
                }
                let policy = execution
                    .get(&remote_name)
                    .cloned()
                    .or_else(|| {
                        // 远端 annotations 和 _meta 不授予执行权限；仅应用内注册表可传回策略。
                        self.trusted_local
                            .then(|| {
                                serde_json::from_value(value["_meta"][EXECUTION_META].clone()).ok()
                            })
                            .flatten()
                    })
                    .unwrap_or_default();
                let definition = ToolDefinition {
                    name: if self.trusted_local {
                        remote_name.clone()
                    } else {
                        sanitize(&format!("{plugin_id}.{server_id}.{remote_name}"))
                    },
                    label: value["title"]
                        .as_str()
                        .or_else(|| value["annotations"]["title"].as_str())
                        .unwrap_or_default()
                        .into(),
                    description: value["description"].as_str().unwrap_or("MCP 工具").into(),
                    input_schema: value["inputSchema"].clone(),
                    output_schema: self.trusted_local.then(|| output_schema.clone()).flatten(),
                    source: format!("plugin:{plugin_id}:mcp:{server_id}"),
                    provider_version: Some(plugin_version.into()),
                    execution: policy,
                };
                tools.push(Arc::new(McpTool {
                    client: self.clone(),
                    remote_name,
                    output_schema,
                    definition,
                }) as Arc<dyn Tool>);
            }
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if let Some(c) = &cursor {
                if !seen.insert(c.clone()) || seen.len() > 100 {
                    return Err(Error::Tool("MCP 分页游标成环".into()));
                }
            } else {
                break;
            }
        }
        Ok(tools)
    }

    fn request(&self, method: &str, params: Value) -> Result<Value> {
        crate::runtime::executor().block_on(self.request_async(
            method,
            params,
            CancellationToken::new(),
        ))
    }

    pub(crate) async fn call_tool(
        &self,
        name: &str,
        arguments: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        normalize_output(
            self.request_async(
                "tools/call",
                json!({"name":name,"arguments":arguments}),
                cancel,
            )
            .await?,
            None,
            self.trusted_local,
        )
    }

    /// 内置服务适配器只读取指定工具的当前参数契约，不向主模型暴露远端目录。
    pub(crate) async fn tool_schema(&self, name: &str, cancel: CancellationToken) -> Result<Value> {
        let mut cursor = None;
        let mut seen = HashSet::new();
        for _ in 0..100 {
            let result = self
                .request_async("tools/list", json!({"cursor":cursor}), cancel.clone())
                .await?;
            if let Some(tool) = result["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|tool| tool["name"] == name)
            {
                return Ok(tool["inputSchema"].clone());
            }
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
            if !seen.insert(cursor.clone()) {
                return Err(Error::Tool("搜索服务的目录游标重复".into()));
            }
        }
        Err(Error::Tool("搜索服务没有提供当前搜索工具".into()))
    }

    async fn request_async(
        &self,
        method: &str,
        params: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let request: ClientRequest =
            serde_json::from_value(json!({"method":method,"params":params}))?;
        let handle = self
            .session
            .send_cancellable_request(request, PeerRequestOptions::with_timeout(self.timeout))
            .await
            .map_err(request_error)?;
        let id = handle.id.clone();
        let result = tokio::select! {
            biased;
            result = handle.await_response() => result.map_err(request_error),
            _ = cancel.cancelled() => {
                let notification = serde_json::from_value(json!({"method":"notifications/cancelled","params":{"requestId":id,"reason":"cancelled"}}))?;
                let _ = tokio::time::timeout(Duration::from_secs(1), self.session.send_notification(notification)).await;
                return Err(Error::Conflict("MCP 调用已请求取消，执行结果仍需核对".into()));
            }
        }?;
        let value = serde_json::to_value(result)?;
        if value.to_string().len() > MAX_RESULT_BYTES {
            return Err(Error::Conflict(
                "MCP 响应超过大小限制，执行结果仍需核对".into(),
            ));
        }
        Ok(value)
    }
}

fn request_error(error: rmcp::ServiceError) -> Error {
    match error {
        rmcp::ServiceError::Timeout { .. } => {
            Error::Timeout("MCP 请求超时，执行结果仍需核对".into())
        }
        rmcp::ServiceError::McpError(error) => Error::Tool(format!(
            "MCP 请求被拒绝：{}",
            error.message.chars().take(2000).collect::<String>()
        )),
        rmcp::ServiceError::TransportClosed
        | rmcp::ServiceError::TransportSend(_)
        | rmcp::ServiceError::UnexpectedResponse => {
            Error::Http("MCP 连接异常，无法确认执行结果".into())
        }
        _ => Error::Conflict("MCP 请求中断，执行结果仍需核对".into()),
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        if let Some(task) = &self.server_task {
            task.abort();
        }
    }
}

fn client_config() -> ClientConfig {
    ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("sleepy-doll", env!("CARGO_PKG_VERSION")),
    )
    .with_protocol_version(ProtocolVersion::V_2025_11_25)
}

struct McpTool {
    client: Arc<McpClient>,
    remote_name: String,
    output_schema: Option<Value>,
    definition: ToolDefinition,
}
impl Tool for McpTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }
    fn call(&self, args: &Value) -> Result<Value> {
        normalize_output(
            self.client.request(
                "tools/call",
                json!({"name":self.remote_name,"arguments":args}),
            )?,
            self.output_schema.as_ref(),
            self.client.trusted_local,
        )
    }
    fn call_async(
        self: Arc<Self>,
        args: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            let result = self
                .client
                .request_async(
                    "tools/call",
                    json!({"name":self.remote_name,"arguments":args}),
                    cancel,
                )
                .await?;
            normalize_output(
                result,
                self.output_schema.as_ref(),
                self.client.trusted_local,
            )
        })
    }
}

fn normalize_output(value: Value, schema: Option<&Value>, trusted_local: bool) -> Result<Value> {
    if value["isError"] == true {
        if trusted_local && value["_meta"][super::mcp_server::ERROR_META].is_object() {
            return Err(super::mcp_server::decode_error(
                &value["_meta"][super::mcp_server::ERROR_META],
            ));
        }
        let detail = value["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .take(2000)
            .collect::<String>();
        return Err(Error::Tool(format!("MCP 工具返回失败：{detail}")));
    }
    if let Some(schema) = schema {
        let structured = value
            .get("structuredContent")
            .ok_or_else(|| Error::Tool("MCP 工具未返回 structuredContent".into()))?;
        if !crate::extension::validate(structured, schema, "$.structuredContent").is_empty() {
            return Err(Error::Tool("MCP structuredContent 不符合输出契约".into()));
        }
    }
    // 带图片、音频或资源的结果必须保留整个标准结果，不能因同时带结构化字段而丢内容。
    let has_media = value["content"]
        .as_array()
        .is_some_and(|content| content.iter().any(|item| item["type"] != "text"));
    if has_media {
        Ok(value)
    } else {
        Ok(value.get("structuredContent").cloned().unwrap_or(value))
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
