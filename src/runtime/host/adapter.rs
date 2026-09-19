use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::oneshot,
};
use tokio_util::sync::CancellationToken;

use crate::runtime::{
    operation::kernel::{DiagnosticFinding, MutationPlan, ResourceDescriptor, ResourceSnapshot},
    store::artifacts::ArtifactStore,
};
use crate::{
    error::{Error, Result},
    extension::plugins::AdapterManifest,
};

type Reply = std::result::Result<Value, String>;
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Reply>>>>;

pub struct AdapterClient {
    pub plugin_id: String,
    pub adapter_id: String,
    pub version: String,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    pending: Pending,
    child: Mutex<Child>,
    next_id: Mutex<u64>,
    timeout: Duration,
    hook_events: std::sync::RwLock<HashSet<String>>,
    _constraint: crate::runtime::host::process::ProcessConstraint,
}

impl AdapterClient {
    pub fn provider_id(&self) -> String {
        format!("{}/{}", self.plugin_id, self.adapter_id)
    }

    pub fn start(plugin_id: &str, manifest: &AdapterManifest) -> Result<Arc<Self>> {
        let _entered = crate::runtime::executor().enter();
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
        let stdin = Arc::new(tokio::sync::Mutex::new(
            child
                .stdin
                .take()
                .ok_or_else(|| Error::Tool("Adapter 的 stdin 不可用".into()))?,
        ));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Tool("Adapter 的 stdout 不可用".into()))?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let dispatcher = pending.clone();
        let writer = stdin.clone();
        let max_frame = manifest.max_response_bytes.clamp(1024, 16 * 1024 * 1024);
        crate::runtime::executor().spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut frame = Vec::new();
            loop {
                let bytes = match reader.fill_buf().await { Ok(bytes) if !bytes.is_empty() => bytes, _ => break };
                let length = bytes.iter().position(|byte| *byte == b'\n').map(|index| index + 1).unwrap_or(bytes.len());
                if frame.len() + length > max_frame { break; }
                frame.extend_from_slice(&bytes[..length]);
                reader.consume(length);
                if frame.last() != Some(&b'\n') { continue; }
                let Ok(message) = serde_json::from_slice::<Value>(&frame) else { break };
                frame.clear();
                if message.get("method").is_some() {
                    if message.get("id").is_some() {
                        let rejection = json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Adapter 不能请求宿主工具或文件系统"}});
                        let mut input = writer.lock().await;
                        if input.write_all(format!("{rejection}\n").as_bytes()).await.is_err() { break; }
                    }
                    continue;
                }
                if let Some(id) = message["id"].as_u64()
                    && let Some(sender) = dispatcher.lock().unwrap().remove(&id)
                {
                    let reply = if message["error"].is_null() { Ok(message["result"].clone()) } else { Err("Adapter 拒绝了请求".into()) };
                    let _ = sender.send(reply);
                }
            }
            for (_, sender) in dispatcher.lock().unwrap().drain() {
                let _ = sender.send(Err("Adapter 已退出或发出了无效的协议数据".into()));
            }
        });
        let client = Arc::new(Self {
            plugin_id: plugin_id.into(),
            adapter_id: manifest.id.clone(),
            version: manifest.version.clone(),
            stdin,
            pending,
            child: Mutex::new(child),
            next_id: Mutex::new(1),
            timeout: Duration::from_millis(manifest.timeout_ms.clamp(100, 300_000)),
            hook_events: std::sync::RwLock::new(HashSet::new()),
            _constraint: constraint,
        });
        let initialized = client.request(
            "initialize",
            json!({
                "protocolVersion":"sleepy-adapter/1",
                "client":{"name":"sleepy-doll","version":env!("CARGO_PKG_VERSION")},
                "permissions":{"filesystem":"brokered","hostTools":false,"secrets":false}
            }),
        )?;
        if initialized["protocolVersion"] != "sleepy-adapter/1" {
            return Err(Error::Tool("Adapter 协议版本不受支持".into()));
        }
        let health = client.request("health", json!({}))?;
        if health["status"] != "ready" {
            return Err(Error::Tool("Adapter 健康检查未通过".into()));
        }
        Ok(client)
    }

    pub fn capabilities(&self) -> Result<Value> {
        self.request("capabilities/list", json!({}))
    }

    pub fn set_hook_events(&self, events: impl IntoIterator<Item = String>) {
        *self.hook_events.write().unwrap() = events.into_iter().collect();
    }

    pub fn accepts_hook(&self, event: &str) -> bool {
        self.hook_events.read().unwrap().contains(event)
    }

    pub async fn handle_hook(
        &self,
        event: &crate::runtime::host::hooks::HookEvent,
        cancel: CancellationToken,
    ) -> Result<Value> {
        self.request_async("hooks/handle", json!({"event":event}), cancel)
            .await
    }

    pub fn discover(&self, input: Value) -> Result<Vec<ResourceDescriptor>> {
        let value = self.request("resources/discover", input)?;
        Ok(serde_json::from_value(value["resources"].clone())?)
    }

    pub fn read_request(&self, resource: &ResourceDescriptor, request: Value) -> Result<Value> {
        self.request(
            "resources/readRequest",
            json!({"resource":resource,"request":request}),
        )
    }

    pub fn inspect(
        &self,
        snapshot: &ResourceSnapshot,
        content: &[u8],
        request: Value,
    ) -> Result<Value> {
        self.request(
            "resources/inspect",
            json!({"snapshot":snapshot,"contentBase64":STANDARD.encode(content),"request":request}),
        )
    }

    pub fn plan(
        &self,
        snapshots: &[(ResourceSnapshot, Vec<u8>)],
        request: Value,
        artifacts: &ArtifactStore,
    ) -> Result<MutationPlan> {
        let snapshots = snapshots
            .iter()
            .map(|(snapshot, content)| {
                json!({"snapshot":snapshot,"contentBase64":STANDARD.encode(content)})
            })
            .collect::<Vec<_>>();
        let response = self.request(
            "mutations/plan",
            json!({"snapshots":snapshots,"request":request}),
        )?;
        let mut plan_value = response
            .get("plan")
            .cloned()
            .unwrap_or_else(|| response.clone());
        let outputs = response
            .get("stagedOutputs")
            .or_else(|| plan_value.get("stagedOutputs"))
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Tool("Adapter 的 MutationPlan 缺少 stagedOutputs".into()))?;
        let mut materialized = Vec::new();
        for output in outputs {
            if output["kind"] == "replaceResource" {
                let encoded = output["contentBase64"].as_str().ok_or_else(|| {
                    Error::Tool("Adapter 的 replaceResource 缺少 contentBase64".into())
                })?;
                let content = STANDARD
                    .decode(encoded)
                    .map_err(|_| Error::Tool("Adapter 返回的暂存内容无法解码".into()))?;
                let artifact = artifacts.put(&content)?;
                materialized.push(json!({
                    "kind":"replaceResource",
                    "resourceId":output["resourceId"],
                    "contentArtifact":artifact,
                    "expectedHash":output["expectedHash"]
                }));
            } else if output["kind"] == "brokeredAction" {
                materialized.push(output.clone());
            } else {
                return Err(Error::Tool("Adapter 返回了不支持的暂存输出".into()));
            }
        }
        plan_value["stagedOutputs"] = Value::Array(materialized);
        let plan: MutationPlan = serde_json::from_value(plan_value)?;
        if plan.provider_id != self.provider_id() {
            return Err(Error::Tool(
                "Adapter MutationPlan 使用了错误的 Provider 标识".into(),
            ));
        }
        Ok(plan)
    }

    pub fn verify(
        &self,
        snapshots: &[(ResourceSnapshot, Vec<u8>)],
        request: Value,
    ) -> Result<Value> {
        let snapshots = snapshots
            .iter()
            .map(|(snapshot, content)| {
                json!({"snapshot":snapshot,"contentBase64":STANDARD.encode(content)})
            })
            .collect::<Vec<_>>();
        self.request(
            "mutations/verify",
            json!({"snapshots":snapshots,"request":request}),
        )
    }

    pub fn diagnose(&self, input: Value) -> Result<Vec<DiagnosticFinding>> {
        let value = self.request("diagnostics/analyze", input)?;
        Ok(serde_json::from_value(value["findings"].clone())?)
    }

    pub fn validate_strategy(&self, input: Value) -> Result<Value> {
        self.request("strategy/validate", input)
    }

    pub fn shutdown(&self) -> Result<()> {
        let _ = crate::runtime::executor().block_on(async {
            tokio::time::timeout(
                Duration::from_secs(1),
                self.request_async("shutdown", json!({}), CancellationToken::new()),
            )
            .await
        });
        let _ = self.child.lock().unwrap().start_kill();
        Ok(())
    }

    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        crate::runtime::executor().block_on(self.request_async(
            method,
            params,
            CancellationToken::new(),
        ))
    }

    pub async fn request_async(
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
        let payload = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        let sent = tokio::select! {
            _ = cancel.cancelled() => Err(Error::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(5), self.write(payload)) => result.unwrap_or_else(|_| Err(Error::Tool("Adapter 输入通道停滞".into())))
        };
        if let Err(error) = sent {
            self.pending.lock().unwrap().remove(&id);
            let _ = self.child.lock().unwrap().start_kill();
            return Err(error);
        }
        let response = tokio::select! {
            _ = cancel.cancelled() => None,
            result = tokio::time::timeout(self.timeout, receiver) => result.ok().and_then(|reply| reply.ok())
        };
        match response {
            Some(Ok(value)) => Ok(value),
            Some(Err(error)) => Err(Error::Tool(error)),
            None => {
                self.pending.lock().unwrap().remove(&id);
                Err(Error::Conflict(
                    "Adapter 调用已停止，若已产生外部效果则需要核对".into(),
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

impl Drop for AdapterClient {
    fn drop(&mut self) {
        if let Ok(input) = self.stdin.try_lock() {
            drop(input);
        }
        let _ = self.child.get_mut().unwrap().start_kill();
    }
}
