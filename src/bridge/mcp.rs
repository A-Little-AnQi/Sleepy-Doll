//! BGI 领域工具的 MCP 服务。宿主桥仍负责来源校验、反射与原生任务。
use std::{path::PathBuf, sync::Arc};

use futures_util::{StreamExt, stream::BoxStream};
use rmcp::ServiceExt;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    bridge::{BgiClient, register_tools},
    error::{Error, Result},
    extension::{Tool, ToolDefinition, ToolExecution, ToolRegistry, mcp_server::RegistryServer},
};

pub fn server(client: Arc<BgiClient>, enabled: bool) -> Result<RegistryServer> {
    let mut registry = ToolRegistry::default();
    let mut internal = ToolRegistry::default();
    if enabled {
        register_tools(&mut registry, client.clone())?;
        let native = Arc::new(crate::runtime::host::bridge::Bridge::new(
            client.config.clone(),
        )?);
        internal = transport_tools(native)?;
    }
    Ok(RegistryServer::new(registry, "sleepy-doll-bgi",
        "BetterGI 的资源、设置和任务工具。接口 methodId 不是 Agent 工具名；通过 bgi.api.read/invoke 的 {methodId, arguments} 调用。实例、目录与资源版本由执行器核验，不写入持久快捷绑定。保存快捷入口不执行目标、不要求游戏就绪；状态接口能响应即已连接。普通启动默认后台交接，收到接纳证据后结束，不由模型轮询 Job；启动不等于业务完成，取消请求不等于已停止。")
        .with_internal(internal))
}

pub(crate) fn transport_tools(
    native: Arc<crate::runtime::host::bridge::Bridge>,
) -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::default();
    let events = Arc::new(tokio::sync::Mutex::new(EventCursor::default()));
    for event_mode in [false, true] {
        registry.register(HostTransport {
            native: native.clone(),
            events: events.clone(),
            event_mode,
        })?;
    }
    Ok(registry)
}

#[derive(Default)]
struct EventCursor {
    stream: Option<BoxStream<'static, Result<Vec<u8>>>>,
    buffer: Vec<u8>,
    cursor: Option<String>,
}

struct HostTransport {
    native: Arc<crate::runtime::host::bridge::Bridge>,
    events: Arc<tokio::sync::Mutex<EventCursor>>,
    event_mode: bool,
}

impl Tool for HostTransport {
    fn definition(&self) -> ToolDefinition {
        let (name, schema) = if self.event_mode {
            (
                "bgi.host.events",
                json!({"type":"object","properties":{"cursor":{"type":["string","null"]}},"additionalProperties":false}),
            )
        } else {
            (
                "bgi.host.request",
                json!({"type":"object","properties":{"method":{"enum":["GET","POST"]},"path":{"type":"string","pattern":"^/bridge/v1/"},"body":{"type":["object","null"]}},"required":["method","path"],"additionalProperties":false}),
            )
        };
        ToolDefinition {
            name: name.into(),
            label: "BGI 原生连接".into(),
            description: "执行器内部连接；不作为模型操作入口。".into(),
            input_schema: schema,
            output_schema: None,
            source: "core:bgi".into(),
            provider_version: None,
            execution: ToolExecution::default(),
        }
    }

    fn call(&self, _: &Value) -> Result<Value> {
        Err(Error::Tool("内部连接仅支持异步调用".into()))
    }

    fn call_async(
        self: Arc<Self>,
        arguments: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            if self.event_mode {
                return self.next_cursor(&arguments, &cancel).await;
            }
            let path = arguments["path"].as_str().unwrap_or_default();
            let body = arguments.get("body").filter(|v| !v.is_null());
            // 错误类别属于执行器契约：保留“不确定结果”与可恢复版本冲突的区别。
            Ok(
                match self
                    .native
                    .request(
                        arguments["method"].as_str().unwrap_or("GET"),
                        path,
                        body,
                        &cancel,
                    )
                    .await
                {
                    Ok(value) => json!({"ok":true,"value":value}),
                    Err(error) => {
                        json!({"ok":false,"error":crate::extension::mcp_server::encode_error(&error)})
                    }
                },
            )
        })
    }
}

impl HostTransport {
    async fn next_cursor(&self, arguments: &Value, cancel: &CancellationToken) -> Result<Value> {
        let mut state = tokio::select! { _ = cancel.cancelled() => return Err(Error::Cancelled), state = self.events.lock() => state };
        let cursor = arguments["cursor"].as_str();
        if state.cursor.as_deref() != cursor {
            state.stream = None;
            state.buffer.clear();
            state.cursor = cursor.map(str::to_owned);
        }
        if state.stream.is_none() {
            state.stream = Some(self.native.open_events(cursor, cancel).await?);
        }
        let read = tokio::select! {
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            chunk = tokio::time::timeout(std::time::Duration::from_millis(500), state.stream.as_mut().unwrap().next()) => chunk,
        };
        match read {
            Ok(Some(Ok(bytes))) => {
                state.buffer.extend(bytes);
                while let Some(end) = state.buffer.iter().position(|b| *b == b'\n') {
                    let line = state.buffer.drain(..=end).collect::<Vec<_>>();
                    if let Some(id) = String::from_utf8_lossy(&line).trim().strip_prefix("id:") {
                        state.cursor = Some(id.trim().into());
                    }
                }
                if state.buffer.len() > 1024 * 1024 {
                    state.stream = None;
                    state.buffer.clear();
                }
            }
            Ok(_) => {
                state.stream = None;
                state.buffer.clear();
            }
            Err(_) => {}
        }
        Ok(json!({"cursor":state.cursor}))
    }
}

pub(crate) fn decode_reply(reply: Value) -> Result<Value> {
    if reply["ok"] == true {
        Ok(reply["value"].clone())
    } else {
        Err(crate::extension::mcp_server::decode_error(&reply["error"]))
    }
}

/// 桌面与开发入口共享此模式，不初始化窗口、数据库、插件或模型。
pub fn run_stdio_if_requested() -> Result<bool> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--mcp-bgi")) {
        return Ok(false);
    }
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err(Error::Config(
            "用法：--mcp-bgi --config <已有配置文件>".into(),
        ));
    }
    let path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| Error::Config("缺少 MCP 配置文件路径".into()))?;
    if args.next().is_some() {
        return Err(Error::Config("MCP 启动参数过多".into()));
    }
    // MCP 模式只读取连接配置，不触发主程序的配置迁移与备份写入。
    let mut value: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    crate::config::expand_env(&mut value["bridge"])?;
    let bridge: crate::config::BridgeConfig = serde_json::from_value(value["bridge"].clone())?;
    let disabled = value["plugins"]["disabled"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id == "bgi"));
    if disabled || !bridge.enabled {
        return Err(Error::Config("请先启用 BetterGI 插件与连接".into()));
    }
    let server = server(Arc::new(BgiClient::new(bridge)), true)?;
    crate::runtime::executor().block_on(async {
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|_| Error::Tool("BGI MCP 服务初始化失败".into()))?;
        service
            .waiting()
            .await
            .map_err(|_| Error::Tool("BGI MCP 服务异常退出".into()))?;
        Ok::<(), Error>(())
    })?;
    Ok(true)
}
