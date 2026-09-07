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

use super::{
    artifacts::ArtifactStore,
    kernel::{DiagnosticFinding, MutationPlan, ResourceDescriptor, ResourceSnapshot},
};
use crate::{
    error::{Error, Result},
    plugins::AdapterManifest,
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
    #[cfg(target_os = "windows")]
    _job: std::os::windows::io::OwnedHandle,
}

impl AdapterClient {
    pub fn provider_id(&self) -> String {
        format!("{}/{}", self.plugin_id, self.adapter_id)
    }

    pub fn start(plugin_id: &str, manifest: &AdapterManifest) -> Result<Arc<Self>> {
        let _entered = super::executor().enter();
        let mut command = Command::new(&manifest.command);
        command
            .args(&manifest.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .env_clear();
        for key in ["PATH", "SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000);
        let mut child = command.spawn()?;
        #[cfg(target_os = "windows")]
        let job = constrain_adapter_process(&child)?;
        let stdin =
            Arc::new(tokio::sync::Mutex::new(child.stdin.take().ok_or_else(
                || Error::Tool("Adapter stdin unavailable".into()),
            )?));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Tool("Adapter stdout unavailable".into()))?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let dispatcher = pending.clone();
        let writer = stdin.clone();
        let max_frame = manifest.max_response_bytes.clamp(1024, 16 * 1024 * 1024);
        super::executor().spawn(async move {
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
                        let rejection = json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32601,"message":"Adapter cannot request host tools or filesystem access"}});
                        let mut input = writer.lock().await;
                        if input.write_all(format!("{rejection}\n").as_bytes()).await.is_err() { break; }
                    }
                    continue;
                }
                if let Some(id) = message["id"].as_u64()
                    && let Some(sender) = dispatcher.lock().unwrap().remove(&id)
                {
                    let reply = if message["error"].is_null() { Ok(message["result"].clone()) } else { Err("Adapter request rejected".into()) };
                    let _ = sender.send(reply);
                }
            }
            for (_, sender) in dispatcher.lock().unwrap().drain() {
                let _ = sender.send(Err("Adapter exited or emitted invalid protocol data".into()));
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
            #[cfg(target_os = "windows")]
            _job: job,
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
            return Err(Error::Tool("Unsupported Adapter protocol version".into()));
        }
        let health = client.request("health", json!({}))?;
        if health["status"] != "ready" {
            return Err(Error::Tool("Adapter health check failed".into()));
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
        event: &super::hooks::HookEvent,
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
            .ok_or_else(|| Error::Tool("Adapter MutationPlan omitted staged outputs".into()))?;
        let mut materialized = Vec::new();
        for output in outputs {
            if output["kind"] == "replaceResource" {
                let encoded = output["contentBase64"].as_str().ok_or_else(|| {
                    Error::Tool("Adapter replaceResource omitted contentBase64".into())
                })?;
                let content = STANDARD
                    .decode(encoded)
                    .map_err(|_| Error::Tool("Adapter returned invalid staged content".into()))?;
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
                return Err(Error::Tool(
                    "Adapter returned an unsupported staged output".into(),
                ));
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
        let _ = super::executor().block_on(async {
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
        super::executor().block_on(self.request_async(method, params, CancellationToken::new()))
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
            result = tokio::time::timeout(Duration::from_secs(5), self.write(payload)) => result.unwrap_or_else(|_| Err(Error::Tool("Adapter input stalled".into())))
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

#[cfg(target_os = "windows")]
fn constrain_adapter_process(child: &Child) -> Result<std::os::windows::io::OwnedHandle> {
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    let process = child
        .raw_handle()
        .ok_or_else(|| Error::Tool("Adapter process handle unavailable".into()))?;
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let owned = unsafe { std::os::windows::io::OwnedHandle::from_raw_handle(job as RawHandle) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_PROCESS_MEMORY;
    limits.ProcessMemoryLimit = 512 * 1024 * 1024;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 || unsafe { AssignProcessToJobObject(job, process as _) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(owned)
}

impl Drop for AdapterClient {
    fn drop(&mut self) {
        if let Ok(input) = self.stdin.try_lock() {
            drop(input);
        }
        let _ = self.child.get_mut().unwrap().start_kill();
    }
}
