use crate::{
    error::{Error, Result},
    extension::plugins::McpServerManifest,
    extension::{Tool, ToolDefinition, ToolExecution},
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::oneshot,
};
use tokio_util::sync::CancellationToken;

type PendingReplies = Arc<Mutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, String>>>>>;
pub struct McpClient {
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: PendingReplies,
    child: Mutex<Child>,
    next_id: Mutex<u64>,
    _constraint: crate::runtime::host::process::ProcessConstraint,
}
impl McpClient {
    pub fn start(manifest: &McpServerManifest) -> Result<Arc<Self>> {
        let _entered = crate::runtime::executor().enter();
        let mut command = Command::new(&manifest.command);
        command
            .args(&manifest.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // An MCP server is a plugin-supplied binary. Without this it would
        // inherit the whole environment, including any `${ENV:...}` model API
        // key or bridge token the configuration keeps there.
        crate::runtime::host::process::isolate_environment(&mut command);
        crate::runtime::host::process::hide_console(&mut command);
        let mut child = command.spawn()?;
        let constraint = crate::runtime::host::process::constrain_process(&child)?;
        let stdin = Arc::new(tokio::sync::Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| Error::Tool("MCP stdin unavailable".into()))?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Tool("MCP stdout unavailable".into()))?;
        let pending: PendingReplies = Arc::new(Mutex::new(HashMap::new()));
        let dispatcher = pending.clone();
        let writer = stdin.clone();
        crate::runtime::executor().spawn(async move {
            let mut reader=BufReader::new(stdout);
            let mut frame=Vec::new();
            loop {
                let bytes=match reader.fill_buf().await{Ok(bytes) if !bytes.is_empty()=>bytes,_=>break};
                let length=bytes.iter().position(|b|*b==b'\n').map(|n|n+1).unwrap_or(bytes.len());
                if frame.len()+length>4*1024*1024{break;}
                frame.extend_from_slice(&bytes[..length]);reader.consume(length);
                if frame.last()!=Some(&b'\n'){continue;}
                let Ok(message)=serde_json::from_slice::<Value>(&frame)else{break};frame.clear();
                if message.get("method").is_some(){
                    if message.get("id").is_some(){
                        let reply=json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Client-initiated sampling and filesystem access are not supported"}});
                        let mut writer=writer.lock().await;
                        if writer.write_all(format!("{reply}\n").as_bytes()).await.is_err(){break;}
                    }
                    continue;
                }
                if let Some(id)=message["id"].as_u64() && let Some(sender)=dispatcher.lock().unwrap().remove(&id){
                    let result=if message["error"].is_null(){Ok(message["result"].clone())}else{Err("MCP request rejected".into())};
                    let _=sender.send(result);
                }
            }
            for (_,sender) in dispatcher.lock().unwrap().drain(){let _=sender.send(Err("MCP server exited or emitted invalid JSON".into()));}
        });
        let client = Arc::new(Self {
            stdin,
            pending,
            child: Mutex::new(child),
            next_id: Mutex::new(1),
            _constraint: constraint,
        });
        let response=client.request("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"sleepy-doll","version":"0.2.0"}}))?;
        if !matches!(
            response["protocolVersion"].as_str(),
            Some("2025-06-18" | "2025-03-26" | "2024-11-05")
        ) {
            return Err(Error::Tool("Unsupported MCP protocol version".into()));
        }
        crate::runtime::executor().block_on(
            client.write(json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{}})),
        )?;
        Ok(client)
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
                .ok_or_else(|| Error::Tool("MCP omitted tools".into()))?
            {
                let remote_name = value["name"]
                    .as_str()
                    .ok_or_else(|| Error::Tool("MCP tool missing name".into()))?
                    .to_owned();
                let remote_output_schema = value
                    .get("outputSchema")
                    .filter(|schema| !schema.is_null())
                    .cloned();
                if let Some(schema) = &remote_output_schema {
                    jsonschema::validator_for(schema)
                        .map_err(|_| Error::Tool("MCP tool output schema is invalid".into()))?;
                }
                let definition = ToolDefinition {
                    name: sanitize(&format!("{plugin_id}.{server_id}.{remote_name}")),
                    description: value["description"].as_str().unwrap_or("MCP tool").into(),
                    input_schema: value["inputSchema"].clone(),
                    output_schema: None,
                    source: format!("plugin:{plugin_id}:mcp:{server_id}"),
                    provider_version: Some(plugin_version.into()),
                    execution: execution.get(&remote_name).cloned().unwrap_or_default(),
                };
                tools.push(Arc::new(McpTool {
                    client: self.clone(),
                    remote_name,
                    remote_output_schema,
                    definition,
                }) as Arc<dyn Tool>);
            }
            cursor = result["nextCursor"].as_str().map(str::to_owned);
            if let Some(c) = &cursor {
                if !seen.insert(c.clone()) || seen.len() > 100 {
                    return Err(Error::Tool("MCP pagination loop".into()));
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
    async fn request_async(
        &self,
        method: &str,
        params: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let id = {
            let mut next = self.next_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, sender);
        let sent = tokio::select! {
            _ = cancel.cancelled()=>Err(Error::Cancelled),
            result=tokio::time::timeout(Duration::from_secs(5),self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})))=>result.unwrap_or_else(|_|Err(Error::Tool("MCP input stalled".into())))
        };
        if let Err(e) = sent {
            self.pending.lock().unwrap().remove(&id);
            let _ = self.child.lock().unwrap().start_kill();
            return Err(e);
        }
        let response = tokio::select! {_ = cancel.cancelled()=>None,r=tokio::time::timeout(Duration::from_secs(30),receiver)=>r.ok().and_then(|r|r.ok())};
        match response {
            Some(Ok(value)) => {
                if value["isError"] == true {
                    Err(Error::Tool("MCP tool reported a failure".into()))
                } else {
                    Ok(value)
                }
            }
            Some(Err(e)) => Err(Error::Tool(e)),
            None => {
                self.pending.lock().unwrap().remove(&id);
                let _=tokio::time::timeout(Duration::from_secs(1),self.write(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":id,"reason":"cancelled or timed out"}}))).await;
                Err(Error::Conflict(
                    "MCP 调用已请求取消，执行结果仍需核对".into(),
                ))
            }
        }
    }
    async fn write(&self, value: Value) -> Result<()> {
        let mut input = self.stdin.lock().await;
        input.write_all(format!("{value}\n").as_bytes()).await?;
        input.flush().await?;
        Ok(())
    }
}
impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.get_mut().unwrap().start_kill();
    }
}
struct McpTool {
    client: Arc<McpClient>,
    remote_name: String,
    remote_output_schema: Option<Value>,
    definition: ToolDefinition,
}
impl Tool for McpTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }
    fn call(&self, args: &Value) -> Result<Value> {
        let value = self.client.request(
            "tools/call",
            json!({"name":self.remote_name,"arguments":args}),
        )?;
        validate_mcp_output(value, self.remote_output_schema.as_ref())
    }
    fn call_async(
        self: Arc<Self>,
        args: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            let value = self
                .client
                .request_async(
                    "tools/call",
                    json!({"name":self.remote_name,"arguments":args}),
                    cancel,
                )
                .await?;
            validate_mcp_output(value, self.remote_output_schema.as_ref())
        })
    }
}
fn validate_mcp_output(value: Value, schema: Option<&Value>) -> Result<Value> {
    if let Some(schema) = schema {
        let structured = value
            .get("structuredContent")
            .ok_or_else(|| Error::Tool("MCP tool omitted structured output".into()))?;
        if !crate::extension::validate(structured, schema, "$.structuredContent").is_empty() {
            return Err(Error::Tool(
                "MCP tool returned invalid structured output".into(),
            ));
        }
    }
    Ok(value)
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
