//! 将已有工具注册表作为标准 MCP 服务发布；不绕过原工具的参数校验与执行逻辑。
use std::sync::Arc;

use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        ListToolsResult, PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerConfig,
        Tool,
    },
    service::RequestContext,
};
use serde_json::json;

use crate::error::Error;
use crate::extension::{ToolEffect, ToolRegistry};

pub(crate) const EXECUTION_META: &str = "io.sleepy-doll/execution";
pub(crate) const INTERNAL_META: &str = "io.sleepy-doll/internal";
pub(crate) const ERROR_META: &str = "io.sleepy-doll/error";

pub(crate) fn encode_error(error: &Error) -> serde_json::Value {
    let (kind, message) = match error {
        Error::Http(s) => ("http", s.clone()),
        Error::Timeout(s) => ("timeout", s.clone()),
        Error::Stale(s) => ("stale", s.clone()),
        Error::Conflict(s) => ("conflict", s.clone()),
        Error::Cancelled => ("cancelled", String::new()),
        _ => ("tool", error.to_string()),
    };
    json!({"kind":kind,"message":message})
}

pub(crate) fn decode_error(value: &serde_json::Value) -> Error {
    let message = value["message"]
        .as_str()
        .unwrap_or("MCP 执行结果无效")
        .to_owned();
    match value["kind"].as_str() {
        Some("http") => Error::Http(message),
        Some("timeout") => Error::Timeout(message),
        Some("stale") => Error::Stale(message),
        Some("conflict") => Error::Conflict(message),
        Some("cancelled") => Error::Cancelled,
        _ => Error::Tool(message),
    }
}

#[derive(Clone)]
pub struct RegistryServer {
    registry: Arc<ToolRegistry>,
    info: ServerConfig,
    internal: Arc<ToolRegistry>,
}

impl RegistryServer {
    pub fn new(registry: ToolRegistry, name: &str, instructions: &str) -> Self {
        Self {
            registry: Arc::new(registry),
            internal: Arc::new(ToolRegistry::default()),
            info: ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                .with_protocol_version(ProtocolVersion::V_2025_11_25)
                .with_server_info(Implementation::new(
                    name.to_owned(),
                    env!("CARGO_PKG_VERSION"),
                ))
                .with_instructions(instructions),
        }
    }

    pub(crate) fn with_internal(mut self, registry: ToolRegistry) -> Self {
        self.internal = Arc::new(registry);
        self
    }

    fn tools(&self) -> Vec<Tool> {
        let definitions = self
            .registry
            .definitions()
            .into_iter()
            .map(|d| (d, false))
            .chain(self.internal.definitions().into_iter().map(|d| (d, true)));
        definitions.map(|(definition, internal)| {
            // MCP 的 _meta 承载产品扩展；其他客户端仍可只按标准字段使用这些工具。
            serde_json::from_value(json!({
                "name": definition.name,
                "title": definition.label,
                "description": definition.description,
                "inputSchema": definition.input_schema,
                "outputSchema": definition.output_schema,
                "annotations": {"readOnlyHint": definition.execution.effect == ToolEffect::ReadOnly},
                "_meta": {EXECUTION_META: definition.execution, INTERNAL_META: internal},
            })).expect("已注册工具可转换为 MCP 工具")
        }).collect()
    }
}

impl ServerHandler for RegistryServer {
    fn get_info(&self) -> ServerConfig {
        self.info.clone()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        if request.and_then(|p| p.cursor).is_some() {
            return Err(ErrorData::invalid_params("无效的工具分页游标", None));
        }
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools().into_iter().find(|tool| tool.name == name)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if self.get_tool(&request.name).is_none() {
            return Err(ErrorData::invalid_params("工具未注册", None));
        }
        let arguments = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or_else(|| json!({}));
        let registry = if self
            .internal
            .definitions()
            .iter()
            .any(|d| d.name == request.name)
        {
            &self.internal
        } else {
            &self.registry
        };
        let result = registry
            .call_async(&request.name, &arguments, context.ct)
            .await;
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(error) => {
                let mut result = CallToolResult::error(vec![ContentBlock::text(error.to_string())]);
                result.meta = Some(
                    serde_json::from_value(json!({ERROR_META:encode_error(&error)}))
                        .expect("MCP 错误元数据"),
                );
                result
            }
        }
        .into())
    }
}
