use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::kernel::*;
use crate::runtime::store::artifacts::ArtifactStore;
use crate::{
    error::{Error, Result},
    extension::{RiskLevel, VerificationMode},
};

pub trait ResourceBroker: Send + Sync {
    fn read(&self, resource: &ResourceDescriptor) -> Result<Vec<u8>>;
    fn replace(
        &self,
        resource: &ResourceDescriptor,
        expected_hash: &str,
        content: &[u8],
    ) -> Result<()>;
}

pub trait MutationVerifier: Send + Sync {
    fn verify(&self, snapshots: &[(ResourceSnapshot, Vec<u8>)], request: Value) -> Result<Value>;
}

impl MutationVerifier for crate::runtime::host::adapter::AdapterClient {
    fn verify(&self, snapshots: &[(ResourceSnapshot, Vec<u8>)], request: Value) -> Result<Value> {
        crate::runtime::host::adapter::AdapterClient::verify(self, snapshots, request)
    }
}

#[derive(Default)]
pub struct BrokerRegistry {
    brokers: HashMap<String, Arc<dyn ResourceBroker>>,
}

impl BrokerRegistry {
    pub fn register(
        &mut self,
        provider_id: impl Into<String>,
        broker: Arc<dyn ResourceBroker>,
    ) -> Result<()> {
        let provider_id = provider_id.into();
        if provider_id.is_empty() || self.brokers.insert(provider_id.clone(), broker).is_some() {
            return Err(Error::Config(format!(
                "duplicate or empty broker: {provider_id}"
            )));
        }
        Ok(())
    }

    fn get(&self, provider_id: &str) -> Result<Arc<dyn ResourceBroker>> {
        self.brokers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| Error::Tool("资源 Provider 未提供提交 Broker".into()))
    }
}

pub struct FileResourceBroker {
    roots: Vec<PathBuf>,
    max_bytes: u64,
}

impl FileResourceBroker {
    pub fn new(roots: &[PathBuf], max_bytes: u64) -> Result<Self> {
        let roots = roots
            .iter()
            .map(|root| root.canonicalize())
            .collect::<std::io::Result<Vec<_>>>()?;
        if roots.is_empty() || max_bytes == 0 {
            return Err(Error::Config(
                "file broker requires canonical roots and a size limit".into(),
            ));
        }
        Ok(Self { roots, max_bytes })
    }

    fn resolve(&self, resource: &ResourceDescriptor) -> Result<PathBuf> {
        let path = resource.location["path"]
            .as_str()
            .ok_or_else(|| Error::Tool("文件资源缺少不透明路径绑定".into()))?;
        let path = Path::new(path).canonicalize()?;
        let root = self
            .roots
            .iter()
            .find(|root| path.starts_with(root))
            .ok_or_else(|| Error::Tool("文件资源超出 Adapter 授权根目录".into()))?;
        let mut current = path.as_path();
        while current.starts_with(root) && current != root {
            if fs::symlink_metadata(current)?.file_type().is_symlink() {
                return Err(Error::Tool("文件资源路径包含符号链接".into()));
            }
            current = current
                .parent()
                .ok_or_else(|| Error::Tool("文件资源路径无效".into()))?;
        }
        if fs::metadata(&path)?.len() > self.max_bytes {
            return Err(Error::Tool("文件资源超过 Broker 大小限制".into()));
        }
        Ok(path)
    }
}

impl ResourceBroker for FileResourceBroker {
    fn read(&self, resource: &ResourceDescriptor) -> Result<Vec<u8>> {
        Ok(fs::read(self.resolve(resource)?)?)
    }

    fn replace(
        &self,
        resource: &ResourceDescriptor,
        expected_hash: &str,
        content: &[u8],
    ) -> Result<()> {
        if content.len() as u64 > self.max_bytes {
            return Err(Error::Tool(
                "staged resource exceeds Broker size limit".into(),
            ));
        }
        let destination = self.resolve(resource)?;
        let current = fs::read(&destination)?;
        if format!("{:x}", Sha256::digest(&current)) != expected_hash {
            return Err(Error::Conflict("resource changed before commit".into()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| Error::Tool("resource has no parent directory".into()))?;
        let temporary = parent.join(format!(
            ".{}.{}.sleepy-stage",
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("resource"),
            uuid::Uuid::new_v4()
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        drop(file);
        let result = replace_file(&destination, &temporary);
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_file(destination: &Path, temporary: &Path) -> Result<()> {
    fs::rename(temporary, destination)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn replace_file(destination: &Path, temporary: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{REPLACEFILE_WRITE_THROUGH, ReplaceFileW};
    let destination_wide = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let temporary_wide = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let success = unsafe {
        ReplaceFileW(
            destination_wide.as_ptr(),
            temporary_wide.as_ptr(),
            std::ptr::null(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub struct OperationStore {
    connection: Mutex<Connection>,
}

impl OperationStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        crate::runtime::store::migrations::migrate(&mut connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn upsert_resource(&self, resource: &ResourceDescriptor) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_resources(id,provider_id,kind,version,payload,updated_at) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET provider_id=excluded.provider_id,kind=excluded.kind,version=excluded.version,payload=excluded.payload,updated_at=excluded.updated_at",
            params![resource.id, resource.provider_id, resource.kind, resource.version, serde_json::to_string(resource)?, crate::runtime::types::now()],
        )?;
        Ok(())
    }

    pub fn resource(&self, id: &str) -> Result<ResourceDescriptor> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_resources WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn resources(&self) -> Result<Vec<ResourceDescriptor>> {
        read_payloads(
            &self.connection,
            "SELECT payload FROM runtime_resources ORDER BY kind,id",
        )
    }

    pub fn save_snapshot(&self, snapshot: &ResourceSnapshot) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO runtime_snapshots(id,resource_id,content_hash,payload,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![snapshot.id, snapshot.resource_id, snapshot.content_hash, serde_json::to_string(snapshot)?, snapshot.created_at],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO runtime_artifact_refs(artifact_id,owner_kind,owner_id,purpose,created_at) VALUES(?1,'resourceSnapshot',?2,'content',?3)",
            params![snapshot.content_artifact,snapshot.id,snapshot.created_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn link_artifact(
        &self,
        artifact_id: &str,
        owner_kind: &str,
        owner_id: &str,
        purpose: &str,
    ) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO runtime_artifact_refs(artifact_id,owner_kind,owner_id,purpose,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![artifact_id,owner_kind,owner_id,purpose,crate::runtime::types::now()],
        )?;
        Ok(())
    }

    pub fn can_read_artifact(
        &self,
        artifact_id: &str,
        owner_kind: &str,
        owner_id: &str,
    ) -> Result<bool> {
        Ok(self.connection.lock().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM runtime_artifact_refs WHERE artifact_id=?1 AND owner_kind=?2 AND owner_id=?3)",
            params![artifact_id,owner_kind,owner_id],
            |row|row.get(0),
        )?)
    }

    pub fn snapshot(&self, id: &str) -> Result<ResourceSnapshot> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_snapshots WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn create(
        &self,
        run_id: Option<&str>,
        title: &str,
        plan: MutationPlan,
    ) -> Result<Operation> {
        plan.execution.validate()?;
        if title.trim().is_empty()
            || plan.id.trim().is_empty()
            || plan.provider_id.trim().is_empty()
        {
            return Err(Error::Config(
                "operation title, plan id and provider are required".into(),
            ));
        }
        let stamp = crate::runtime::types::now();
        let id = uuid::Uuid::new_v4().to_string();
        let checkpoint = OperationCheckpoint {
            operation_id: id.clone(),
            state: OperationState::Draft,
            completed_steps: vec![],
            pending_step: None,
            held_leases: vec![],
            unresolved_attempts: vec![],
            pending_approval: None,
            revision: 0,
            updated_at: stamp.clone(),
        };
        let operation = Operation {
            id: id.clone(),
            run_id: run_id.map(str::to_owned),
            provider_id: plan.provider_id.clone(),
            title: title.trim().into(),
            state: OperationState::Draft,
            revision: 0,
            risk: plan.execution.risk,
            plan,
            checkpoint,
            created_at: stamp.clone(),
            updated_at: stamp.clone(),
            result: None,
            error: None,
        };
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO runtime_operations(id,run_id,provider_id,state,revision,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,0,?5,?6,?6)",
            params![operation.id, operation.run_id, operation.provider_id, serde_json::to_string(&operation.state)?, serde_json::to_string(&operation)?, stamp],
        )?;
        for output in &operation.plan.staged_outputs {
            if let StagedOutput::ReplaceResource {
                content_artifact, ..
            } = output
            {
                tx.execute(
                    "INSERT OR IGNORE INTO runtime_artifact_refs(artifact_id,owner_kind,owner_id,purpose,created_at) VALUES(?1,'operation',?2,'stagedOutput',?3)",
                    params![content_artifact,operation.id,stamp],
                )?;
            }
        }
        insert_event(&tx, &operation.id, "operation.created", &json!(operation))?;
        tx.commit()?;
        Ok(operation)
    }

    pub fn get(&self, id: &str) -> Result<Operation> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_operations WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn list(&self) -> Result<Vec<Operation>> {
        read_payloads(
            &self.connection,
            "SELECT payload FROM runtime_operations ORDER BY updated_at DESC LIMIT 200",
        )
    }

    pub fn transition(&self, operation: &mut Operation, next: OperationState) -> Result<()> {
        if !operation.state.permits(next) {
            return Err(Error::Conflict(format!(
                "invalid operation transition {:?} -> {:?}",
                operation.state, next
            )));
        }
        let previous = operation.revision;
        let mut candidate = operation.clone();
        candidate.state = next;
        candidate.revision += 1;
        candidate.updated_at = crate::runtime::types::now();
        candidate.checkpoint.state = next;
        candidate.checkpoint.revision = candidate.revision;
        candidate.checkpoint.updated_at = candidate.updated_at.clone();
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction()?;
        if tx.execute(
            "UPDATE runtime_operations SET state=?1,revision=?2,payload=?3,updated_at=?4 WHERE id=?5 AND revision=?6",
            params![serde_json::to_string(&next)?, candidate.revision, serde_json::to_string(&candidate)?, candidate.updated_at, candidate.id, previous],
        )? != 1 {
            return Err(Error::Conflict("stale operation revision".into()));
        }
        insert_event(&tx, &candidate.id, "operation.changed", &json!(candidate))?;
        tx.commit()?;
        *operation = candidate;
        Ok(())
    }

    pub fn acquire(&self, scope: &str, operation_id: &str) -> Result<bool> {
        Ok(self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO runtime_resource_leases(scope,operation_id,acquired_at) VALUES(?1,?2,?3)",
            params![scope, operation_id, crate::runtime::types::now()],
        )? == 1)
    }

    pub fn release(&self, scope: &str, operation_id: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM runtime_resource_leases WHERE scope=?1 AND operation_id=?2",
            params![scope, operation_id],
        )?;
        Ok(())
    }

    pub fn release_operation(&self, operation_id: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM runtime_resource_leases WHERE operation_id=?1",
            [operation_id],
        )?;
        Ok(())
    }

    pub fn record_evidence(
        &self,
        operation_id: &str,
        kind: &str,
        payload: Value,
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_evidence(id,operation_id,kind,payload,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![id, operation_id, kind, payload.to_string(), crate::runtime::types::now()],
        )?;
        Ok(id)
    }

    pub fn recoverable(&self) -> Result<Vec<Operation>> {
        let all = self.list()?;
        Ok(all
            .into_iter()
            .filter(|operation| !operation.state.terminal())
            .collect())
    }

    pub fn save_workflow(
        &self,
        workflow: &crate::runtime::operation::workflow::Workflow,
    ) -> Result<()> {
        workflow.validate()?;
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_workflows(id,revision,payload,created_at) VALUES(?1,?2,?3,?4)",
            params![
                workflow.id,
                workflow.revision,
                serde_json::to_string(workflow)?,
                workflow.created_at
            ],
        )?;
        Ok(())
    }

    pub fn workflow(&self, id: &str) -> Result<crate::runtime::operation::workflow::Workflow> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_workflows WHERE id=?1 ORDER BY revision DESC LIMIT 1",
            [id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn workflows(&self) -> Result<Vec<crate::runtime::operation::workflow::Workflow>> {
        read_payloads(
            &self.connection,
            "SELECT w.payload FROM runtime_workflows w JOIN (SELECT id,MAX(revision) revision FROM runtime_workflows GROUP BY id) latest ON latest.id=w.id AND latest.revision=w.revision ORDER BY w.created_at DESC",
        )
    }

    pub fn record_workflow_run(
        &self,
        workflow_id: &str,
        revision: u64,
        run_id: &str,
    ) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_workflow_runs(workflow_id,workflow_revision,run_id,created_at) VALUES(?1,?2,?3,?4)",
            params![workflow_id, revision, run_id, crate::runtime::types::now()],
        )?;
        Ok(())
    }

    pub fn save_preference(&self, preference: &PreferenceRecord) -> Result<()> {
        if !preference.explicitly_provided
            || preference.key.trim().is_empty()
            || preference.scope.trim().is_empty()
        {
            return Err(Error::Config("只保存用户明确提供且带作用域的偏好".into()));
        }
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_preferences(key,scope,payload) VALUES(?1,?2,?3) ON CONFLICT(key,scope) DO UPDATE SET payload=excluded.payload",
            params![preference.key, preference.scope, serde_json::to_string(preference)?],
        )?;
        Ok(())
    }

    pub fn preferences(&self, scope: &str) -> Result<Vec<PreferenceRecord>> {
        let db = self.connection.lock().unwrap();
        let mut statement =
            db.prepare("SELECT payload FROM runtime_preferences WHERE scope=?1 ORDER BY key")?;
        let rows = statement
            .query_map([scope], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .collect()
    }

    pub fn delete_preference(&self, key: &str, scope: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM runtime_preferences WHERE key=?1 AND scope=?2",
            params![key, scope],
        )?;
        Ok(())
    }

    pub fn record_metric(&self, metric: &MetricEvent) -> Result<()> {
        if metric.name.trim().is_empty() || !metric.value.is_finite() {
            return Err(Error::Config("metric name or value is invalid".into()));
        }
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_metrics(name,recorded_at,payload) VALUES(?1,?2,?3)",
            params![
                metric.name,
                metric.recorded_at,
                serde_json::to_string(metric)?
            ],
        )?;
        Ok(())
    }

    pub fn metrics(&self, limit: usize) -> Result<Vec<MetricEvent>> {
        let db = self.connection.lock().unwrap();
        let mut statement =
            db.prepare("SELECT payload FROM runtime_metrics ORDER BY id DESC LIMIT ?1")?;
        let rows = statement
            .query_map([limit.clamp(1, 1000)], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .collect()
    }

    pub fn save_diagnostic(&self, finding: &DiagnosticFinding) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT OR REPLACE INTO runtime_diagnostics(id,provider_id,kind,payload,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![finding.id, finding.provider_id, serde_json::to_string(&finding.kind)?, serde_json::to_string(finding)?, finding.created_at],
        )?;
        Ok(())
    }

    pub fn diagnostics(&self) -> Result<Vec<DiagnosticFinding>> {
        read_payloads(
            &self.connection,
            "SELECT payload FROM runtime_diagnostics ORDER BY created_at DESC LIMIT 200",
        )
    }

    pub fn save_grant(
        &self,
        grant: &crate::runtime::operation::permissions::TrustGrant,
    ) -> Result<()> {
        crate::runtime::operation::permissions::PermissionEngine::validate_grant(grant)?;
        self.connection.lock().unwrap().execute(
            "INSERT OR REPLACE INTO runtime_trust_grants(id,payload,expires_at,revoked_at) VALUES(?1,?2,?3,NULL)",
            params![grant.id, serde_json::to_string(grant)?, grant.expires_at],
        )?;
        Ok(())
    }

    pub fn grants(&self) -> Result<Vec<crate::runtime::operation::permissions::TrustGrant>> {
        read_payloads(
            &self.connection,
            "SELECT payload FROM runtime_trust_grants WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at>unixepoch()) ORDER BY rowid DESC",
        )
    }

    pub fn revoke_grant(&self, id: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_trust_grants SET revoked_at=?1 WHERE id=?2",
            params![crate::runtime::types::now(), id],
        )?;
        Ok(())
    }

    pub fn notify(
        &self,
        kind: &str,
        title: &str,
        message: &str,
        target: Option<Value>,
    ) -> Result<NotificationRecord> {
        if kind.trim().is_empty() || title.trim().is_empty() || message.trim().is_empty() {
            return Err(Error::Config(
                "notification kind, title and message are required".into(),
            ));
        }
        let notification = NotificationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            kind: kind.into(),
            title: title.into(),
            message: message.into(),
            target,
            created_at: crate::runtime::types::now(),
            read_at: None,
        };
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_notifications(id,kind,payload,created_at) VALUES(?1,?2,?3,?4)",
            params![
                notification.id,
                notification.kind,
                serde_json::to_string(&notification)?,
                notification.created_at
            ],
        )?;
        Ok(notification)
    }

    pub fn notifications(&self, unread_only: bool) -> Result<Vec<NotificationRecord>> {
        let sql = if unread_only {
            "SELECT payload FROM runtime_notifications WHERE read_at IS NULL ORDER BY created_at DESC LIMIT 100"
        } else {
            "SELECT payload FROM runtime_notifications ORDER BY created_at DESC LIMIT 100"
        };
        read_payloads(&self.connection, sql)
    }

    pub fn mark_notification_read(&self, id: &str) -> Result<()> {
        let stamp = crate::runtime::types::now();
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction()?;
        let payload: String = tx.query_row(
            "SELECT payload FROM runtime_notifications WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        let mut notification: NotificationRecord = serde_json::from_str(&payload)?;
        notification.read_at = Some(stamp.clone());
        tx.execute(
            "UPDATE runtime_notifications SET payload=?1,read_at=?2 WHERE id=?3",
            params![serde_json::to_string(&notification)?, stamp, id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

pub struct OperationEngine {
    pub store: Arc<OperationStore>,
    pub artifacts: Arc<ArtifactStore>,
    brokers: Arc<Mutex<BrokerRegistry>>,
    verifiers: Arc<Mutex<HashMap<String, Arc<dyn MutationVerifier>>>>,
}

impl OperationEngine {
    pub fn new(store: Arc<OperationStore>, artifacts: Arc<ArtifactStore>) -> Self {
        Self {
            store,
            artifacts,
            brokers: Arc::new(Mutex::new(BrokerRegistry::default())),
            verifiers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_broker(
        &self,
        provider_id: impl Into<String>,
        broker: Arc<dyn ResourceBroker>,
    ) -> Result<()> {
        self.brokers.lock().unwrap().register(provider_id, broker)
    }

    pub fn configure_file_brokers(&self, specs: &[(String, Vec<PathBuf>)]) -> Result<()> {
        let mut registry = BrokerRegistry::default();
        for (provider, roots) in specs {
            registry.register(
                provider.clone(),
                Arc::new(FileResourceBroker::new(roots, 64 * 1024 * 1024)?),
            )?;
        }
        *self.brokers.lock().unwrap() = registry;
        Ok(())
    }

    pub fn configure_verifiers(
        &self,
        adapters: &[Arc<crate::runtime::host::adapter::AdapterClient>],
    ) {
        *self.verifiers.lock().unwrap() = adapters
            .iter()
            .map(|adapter| {
                (
                    adapter.provider_id(),
                    adapter.clone() as Arc<dyn MutationVerifier>,
                )
            })
            .collect();
    }

    pub fn snapshot(&self, resource: &ResourceDescriptor) -> Result<ResourceSnapshot> {
        let broker = self.brokers.lock().unwrap().get(&resource.provider_id)?;
        let content = broker.read(resource)?;
        let content_hash = format!("{:x}", Sha256::digest(&content));
        let artifact = self.artifacts.put(&content)?;
        let snapshot = ResourceSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            resource_id: resource.id.clone(),
            resource_version: resource.version.clone(),
            content_artifact: artifact,
            content_hash,
            metadata: Value::Null,
            created_at: crate::runtime::types::now(),
        };
        self.store.save_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub fn recover(&self) -> Result<()> {
        for mut operation in self.store.recoverable()? {
            operation.error = Some("应用中断后已根据最后一个持久检查点恢复".into());
            match operation.state {
                OperationState::Draft | OperationState::Prepared => {
                    self.store.release_operation(&operation.id)?;
                    self.store
                        .transition(&mut operation, OperationState::Cancelled)?;
                }
                OperationState::AwaitingAuthorization => {}
                OperationState::Validating | OperationState::Preparing => {
                    self.store.release_operation(&operation.id)?;
                    self.store
                        .transition(&mut operation, OperationState::Failed)?;
                }
                OperationState::Committing
                | OperationState::Verifying
                | OperationState::RollingBack
                | OperationState::Cancelling
                | OperationState::Recovering => {
                    if operation.state != OperationState::Recovering {
                        self.store
                            .transition(&mut operation, OperationState::Recovering)?;
                    }
                    self.store
                        .transition(&mut operation, OperationState::NeedsReview)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub fn execute_pre_authorized(&self, operation_id: &str) -> Result<Operation> {
        let mut operation = self.store.get(operation_id)?;
        if operation.state != OperationState::AwaitingAuthorization {
            return Err(Error::Conflict("操作尚未进入等待授权状态".into()));
        }
        let broker = match self.brokers.lock().unwrap().get(&operation.provider_id) {
            Ok(broker) => broker,
            Err(error) => {
                operation.error = Some(error.user_message());
                self.store
                    .transition(&mut operation, OperationState::Failed)?;
                return Ok(operation);
            }
        };
        self.store
            .transition(&mut operation, OperationState::Preparing)?;
        let preparation = (|| {
            let mut snapshots = HashMap::new();
            for precondition in &operation.plan.resources {
                let resource = self.store.resource(&precondition.resource_id)?;
                if resource.provider_id != operation.provider_id
                    || resource.version != precondition.expected_version
                {
                    return Err(Error::Conflict("资源 Provider 或版本已经变化".into()));
                }
                let snapshot = self.snapshot(&resource)?;
                if snapshot.content_hash != precondition.expected_hash {
                    return Err(Error::Conflict("资源内容已经变化，未执行修改".into()));
                }
                snapshots.insert(resource.id.clone(), snapshot);
            }
            Ok(snapshots)
        })();
        let snapshots = match preparation {
            Ok(snapshots) => snapshots,
            Err(error) => {
                operation.error = Some(error.user_message());
                self.store.release_operation(&operation.id)?;
                self.store
                    .transition(&mut operation, OperationState::Failed)?;
                return Ok(operation);
            }
        };
        operation.plan.compensation.restore_snapshots = snapshots
            .values()
            .map(|snapshot| snapshot.id.clone())
            .collect();
        let scope = operation
            .plan
            .execution
            .lease_scope
            .clone()
            .unwrap_or_else(|| format!("provider:{}", operation.provider_id));
        if !self.store.acquire(&scope, &operation.id)? {
            operation.error = Some("资源正在被另一项操作使用".into());
            self.store
                .transition(&mut operation, OperationState::Failed)?;
            return Ok(operation);
        }
        operation.checkpoint.held_leases.push(scope.clone());
        self.store
            .transition(&mut operation, OperationState::Prepared)?;
        self.store
            .transition(&mut operation, OperationState::Committing)?;
        let mut committed_resources = Vec::new();
        let result = (|| {
            for output in &operation.plan.staged_outputs {
                match output {
                    StagedOutput::ReplaceResource {
                        resource_id,
                        content_artifact,
                        expected_hash,
                    } => {
                        let resource = self.store.resource(resource_id)?;
                        let content = self.artifacts.get(content_artifact)?;
                        broker.replace(&resource, expected_hash, &content)?;
                        committed_resources.push(resource_id.clone());
                    }
                    StagedOutput::BrokeredAction { .. } => {
                        return Err(Error::Tool("该系统操作尚未注册受信执行器".into()));
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            operation.error = Some(error.user_message());
            self.store
                .transition(&mut operation, OperationState::RollingBack)?;
            let restored = self.restore_snapshots(&operation, &broker, &committed_resources);
            self.store.transition(
                &mut operation,
                if restored.is_ok() {
                    OperationState::RolledBack
                } else {
                    OperationState::NeedsReview
                },
            )?;
            self.store.release(&scope, &operation.id)?;
            return Ok(operation);
        }
        self.store
            .transition(&mut operation, OperationState::Verifying)?;
        let mut evidence = Vec::new();
        let mut verified_snapshots = Vec::new();
        for output in &operation.plan.staged_outputs {
            if let StagedOutput::ReplaceResource {
                resource_id,
                content_artifact,
                ..
            } = output
            {
                let resource = self.store.resource(resource_id)?;
                let actual = broker.read(&resource)?;
                let expected = self.artifacts.get(content_artifact)?;
                if actual != expected {
                    operation.error = Some("提交后的资源与计划结果不一致".into());
                    self.store
                        .transition(&mut operation, OperationState::NeedsReview)?;
                    return Ok(operation);
                }
                evidence.push(self.store.record_evidence(&operation.id, "contentHash", json!({"resourceId":resource_id,"hash":format!("{:x}",Sha256::digest(&actual))}))?);
                verified_snapshots.push((self.snapshot(&resource)?, actual));
            }
        }
        if operation.plan.verification.provider_id != operation.provider_id {
            operation.error = Some("验证请求使用了错误的 Provider".into());
            self.store
                .transition(&mut operation, OperationState::NeedsReview)?;
            return Ok(operation);
        }
        let verifier = self
            .verifiers
            .lock()
            .unwrap()
            .get(&operation.provider_id)
            .cloned();
        if operation.plan.execution.verification == VerificationMode::Plugin {
            let Some(verifier) = verifier else {
                operation.error = Some("领域验证 Adapter 当前不可用".into());
                self.store
                    .transition(&mut operation, OperationState::NeedsReview)?;
                return Ok(operation);
            };
            let verification = match verifier.verify(
                &verified_snapshots,
                json!({"method":operation.plan.verification.method,"input":operation.plan.verification.input}),
            ) {
                Ok(verification) => verification,
                Err(error) => {
                    operation.error = Some(format!(
                        "领域验证未完成：{}",
                        error.user_message()
                    ));
                    self.store
                        .transition(&mut operation, OperationState::NeedsReview)?;
                    return Ok(operation);
                }
            };
            match verification["status"].as_str() {
                Some("succeeded") => {
                    evidence.push(self.store.record_evidence(
                        &operation.id,
                        "pluginVerification",
                        verification,
                    )?);
                }
                Some("failed") => {
                    operation.error = Some("领域验证未通过，正在恢复原资源".into());
                    self.store
                        .transition(&mut operation, OperationState::RollingBack)?;
                    let restored =
                        self.restore_snapshots(&operation, &broker, &committed_resources);
                    self.store.transition(
                        &mut operation,
                        if restored.is_ok() {
                            OperationState::RolledBack
                        } else {
                            OperationState::NeedsReview
                        },
                    )?;
                    self.store.release(&scope, &operation.id)?;
                    return Ok(operation);
                }
                _ => {
                    operation.error = Some("领域验证结果未知，需要核对".into());
                    self.store
                        .transition(&mut operation, OperationState::NeedsReview)?;
                    return Ok(operation);
                }
            }
        }
        operation.result = Some(json!({"verified":true,"evidenceIds":evidence}));
        self.store
            .transition(&mut operation, OperationState::Succeeded)?;
        self.store.release(&scope, &operation.id)?;
        Ok(operation)
    }

    pub fn request_authorization(&self, operation_id: &str) -> Result<Operation> {
        let mut operation = self.store.get(operation_id)?;
        self.store
            .transition(&mut operation, OperationState::Validating)?;
        if let Err(error) = validate_plan(&operation.plan) {
            operation.error = Some(error.user_message());
            self.store
                .transition(&mut operation, OperationState::Failed)?;
            return Ok(operation);
        }
        self.store
            .transition(&mut operation, OperationState::AwaitingAuthorization)?;
        Ok(operation)
    }

    pub fn rollback(&self, operation_id: &str) -> Result<Operation> {
        let mut operation = self.store.get(operation_id)?;
        if operation.state != OperationState::NeedsReview {
            return Err(Error::Conflict("只有待核对操作可以请求安全回退".into()));
        }
        self.store
            .transition(&mut operation, OperationState::RollingBack)?;
        let broker = self.brokers.lock().unwrap().get(&operation.provider_id)?;
        let committed = operation
            .plan
            .staged_outputs
            .iter()
            .filter_map(|output| match output {
                StagedOutput::ReplaceResource { resource_id, .. } => Some(resource_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let restored = self.restore_snapshots(&operation, &broker, &committed);
        let next = match restored {
            Ok(()) => OperationState::RolledBack,
            Err(error) => {
                operation.error = Some(error.user_message());
                OperationState::NeedsReview
            }
        };
        self.store.transition(&mut operation, next)?;
        if operation.state == OperationState::RolledBack {
            self.store.release_operation(&operation.id)?;
        }
        Ok(operation)
    }

    pub fn cancel(&self, operation_id: &str) -> Result<Operation> {
        let mut operation = self.store.get(operation_id)?;
        if operation.state.terminal() {
            return Ok(operation);
        }
        if operation.state == OperationState::NeedsReview {
            return Err(Error::Conflict(
                "效果尚未确认，不能用取消代替核对或回退".into(),
            ));
        }
        let effects_may_exist = matches!(
            operation.state,
            OperationState::Committing
                | OperationState::Verifying
                | OperationState::RollingBack
                | OperationState::Recovering
        );
        self.store
            .transition(&mut operation, OperationState::Cancelling)?;
        self.store.transition(
            &mut operation,
            if effects_may_exist {
                OperationState::NeedsReview
            } else {
                OperationState::Cancelled
            },
        )?;
        if operation.state == OperationState::Cancelled {
            self.store.release_operation(&operation.id)?;
        }
        Ok(operation)
    }

    pub fn permission_decision(
        &self,
        operation: &Operation,
        mode: crate::runtime::operation::permissions::PermissionMode,
        grants: &[crate::runtime::operation::permissions::TrustGrant],
    ) -> Result<crate::runtime::operation::permissions::PermissionDecision> {
        let resource_ids = operation
            .plan
            .resources
            .iter()
            .map(|resource| resource.resource_id.clone())
            .collect::<Vec<_>>();
        let resource_kinds = resource_ids
            .iter()
            .map(|id| self.store.resource(id).map(|resource| resource.kind))
            .collect::<Result<Vec<_>>>()?;
        Ok(
            crate::runtime::operation::permissions::PermissionEngine::decide(
                mode,
                &crate::runtime::operation::permissions::PermissionRequest {
                    provider_id: &operation.provider_id,
                    resource_ids: &resource_ids,
                    resource_kinds: &resource_kinds,
                    effect: operation.plan.execution.effect,
                    risk: operation.plan.execution.risk,
                    unattended: operation.plan.execution.unattended,
                },
                grants,
            ),
        )
    }

    fn restore_snapshots(
        &self,
        operation: &Operation,
        broker: &Arc<dyn ResourceBroker>,
        committed_resources: &[String],
    ) -> Result<()> {
        for id in operation.plan.compensation.restore_snapshots.iter().rev() {
            let snapshot = self.store.snapshot(id)?;
            if !committed_resources.contains(&snapshot.resource_id) {
                continue;
            }
            let resource = self.store.resource(&snapshot.resource_id)?;
            let current = broker.read(&resource)?;
            let current_hash = format!("{:x}", Sha256::digest(&current));
            let staged_artifact = operation
                .plan
                .staged_outputs
                .iter()
                .find_map(|output| match output {
                    StagedOutput::ReplaceResource {
                        resource_id,
                        content_artifact,
                        ..
                    } if resource_id == &snapshot.resource_id => Some(content_artifact),
                    _ => None,
                })
                .ok_or_else(|| Error::Conflict("缺少已提交资源的 staged artifact".into()))?;
            let staged_hash = format!("{:x}", Sha256::digest(self.artifacts.get(staged_artifact)?));
            if current_hash != staged_hash {
                return Err(Error::Conflict("资源在提交后再次变化，不能安全回退".into()));
            }
            let content = self.artifacts.get(&snapshot.content_artifact)?;
            broker.replace(&resource, &current_hash, &content)?;
        }
        Ok(())
    }
}

fn validate_plan(plan: &MutationPlan) -> Result<()> {
    plan.execution.validate()?;
    if plan.resources.is_empty() || plan.staged_outputs.is_empty() {
        return Err(Error::Config(
            "mutation plan must bind resources and staged outputs".into(),
        ));
    }
    if plan.execution.risk == RiskLevel::Irreversible
        && !plan.compensation.restore_snapshots.is_empty()
    {
        return Err(Error::Config(
            "irreversible plans cannot claim snapshot compensation".into(),
        ));
    }
    if plan.execution.verification == VerificationMode::None {
        return Err(Error::Config("mutations require verification".into()));
    }
    Ok(())
}

fn insert_event(db: &Connection, operation_id: &str, kind: &str, payload: &Value) -> Result<()> {
    db.execute(
        "INSERT INTO runtime_operation_events(operation_id,kind,payload,created_at) VALUES(?1,?2,?3,?4)",
        params![operation_id, kind, payload.to_string(), crate::runtime::types::now()],
    )?;
    Ok(())
}

fn read_payloads<T: serde::de::DeserializeOwned>(
    connection: &Mutex<Connection>,
    sql: &str,
) -> Result<Vec<T>> {
    let db = connection.lock().unwrap();
    let mut statement = db.prepare(sql)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::{CompensationMode, ToolEffect};

    struct MemoryBroker(Mutex<HashMap<String, Vec<u8>>>);
    impl ResourceBroker for MemoryBroker {
        fn read(&self, resource: &ResourceDescriptor) -> Result<Vec<u8>> {
            self.0
                .lock()
                .unwrap()
                .get(&resource.id)
                .cloned()
                .ok_or_else(|| Error::Tool("missing".into()))
        }
        fn replace(
            &self,
            resource: &ResourceDescriptor,
            expected_hash: &str,
            content: &[u8],
        ) -> Result<()> {
            let mut values = self.0.lock().unwrap();
            let current = values
                .get(&resource.id)
                .ok_or_else(|| Error::Tool("missing".into()))?;
            if format!("{:x}", Sha256::digest(current)) != expected_hash {
                return Err(Error::Conflict("changed".into()));
            }
            values.insert(resource.id.clone(), content.to_vec());
            Ok(())
        }
    }

    struct FailingBroker {
        values: Mutex<HashMap<String, Vec<u8>>>,
        fail_on: String,
    }
    impl ResourceBroker for FailingBroker {
        fn read(&self, resource: &ResourceDescriptor) -> Result<Vec<u8>> {
            self.values
                .lock()
                .unwrap()
                .get(&resource.id)
                .cloned()
                .ok_or_else(|| Error::Tool("missing".into()))
        }
        fn replace(
            &self,
            resource: &ResourceDescriptor,
            expected_hash: &str,
            content: &[u8],
        ) -> Result<()> {
            if resource.id == self.fail_on {
                return Err(Error::Tool("injected failure".into()));
            }
            let mut values = self.values.lock().unwrap();
            let current = values
                .get(&resource.id)
                .ok_or_else(|| Error::Tool("missing".into()))?;
            if format!("{:x}", Sha256::digest(current)) != expected_hash {
                return Err(Error::Conflict("changed".into()));
            }
            values.insert(resource.id.clone(), content.to_vec());
            Ok(())
        }
    }

    #[test]
    fn opaque_resource_mutation_is_snapshotted_committed_and_verified() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.db");
        let store = Arc::new(OperationStore::open(&database).unwrap());
        let artifacts =
            Arc::new(ArtifactStore::new(directory.path().join("artifacts"), 1024).unwrap());
        let engine = OperationEngine::new(store.clone(), artifacts.clone());
        let broker = Arc::new(MemoryBroker(Mutex::new(HashMap::from([(
            "resource".into(),
            b"old".to_vec(),
        )]))));
        engine.register_broker("plugin", broker.clone()).unwrap();
        let resource = ResourceDescriptor {
            id: "resource".into(),
            provider_id: "plugin".into(),
            kind: "opaque".into(),
            display_name: "test".into(),
            version: "1".into(),
            location: Value::Null,
            capabilities: vec!["replace".into()],
            presentation: None,
        };
        store.upsert_resource(&resource).unwrap();
        let old_hash = format!("{:x}", Sha256::digest(b"old"));
        let artifact = artifacts.put(b"new").unwrap();
        let execution = crate::extension::ToolExecution {
            effect: ToolEffect::LocalWrite,
            compensation: CompensationMode::SnapshotRestore,
            verification: VerificationMode::State,
            ..Default::default()
        };
        let plan = MutationPlan {
            id: "plan".into(),
            provider_id: "plugin".into(),
            resources: vec![ResourcePrecondition {
                resource_id: "resource".into(),
                expected_version: "1".into(),
                expected_hash: old_hash.clone(),
            }],
            staged_outputs: vec![StagedOutput::ReplaceResource {
                resource_id: "resource".into(),
                content_artifact: artifact,
                expected_hash: old_hash,
            }],
            execution,
            verification: VerificationRequest {
                provider_id: "plugin".into(),
                method: "verify".into(),
                input: Value::Null,
            },
            compensation: CompensationPlan {
                restore_snapshots: vec![],
                provider_request: None,
            },
        };
        let operation = store.create(None, "test", plan).unwrap();
        engine.request_authorization(&operation.id).unwrap();
        let completed = engine.execute_pre_authorized(&operation.id).unwrap();
        assert_eq!(completed.state, OperationState::Succeeded);
        assert_eq!(broker.0.lock().unwrap()["resource"], b"new");
    }

    #[test]
    fn operation_state_machine_rejects_terminal_reentry() {
        assert!(OperationState::Draft.permits(OperationState::Validating));
        assert!(!OperationState::Succeeded.permits(OperationState::Recovering));
    }

    #[test]
    fn file_broker_enforces_roots_and_compare_before_replace() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = root.path().join("opaque.data");
        let outside_target = outside.path().join("outside.data");
        fs::write(&target, b"old").unwrap();
        fs::write(&outside_target, b"private").unwrap();
        let broker = FileResourceBroker::new(&[root.path().to_path_buf()], 1024).unwrap();
        let resource = ResourceDescriptor {
            id: "r".into(),
            provider_id: "p".into(),
            kind: "opaque".into(),
            display_name: "r".into(),
            version: "1".into(),
            location: json!({"path":target}),
            capabilities: vec!["replace".into()],
            presentation: None,
        };
        let old_hash = format!("{:x}", Sha256::digest(b"old"));
        broker.replace(&resource, &old_hash, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(broker.replace(&resource, &old_hash, b"again").is_err());
        let outside_resource = ResourceDescriptor {
            location: json!({"path":outside_target}),
            ..resource
        };
        assert!(broker.read(&outside_resource).is_err());
    }

    #[test]
    fn partial_commit_restores_only_resources_written_by_this_operation() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(OperationStore::open(&directory.path().join("test.db")).unwrap());
        let artifacts =
            Arc::new(ArtifactStore::new(directory.path().join("artifacts"), 1024).unwrap());
        let engine = OperationEngine::new(store.clone(), artifacts.clone());
        let broker = Arc::new(FailingBroker {
            values: Mutex::new(HashMap::from([
                ("a".into(), b"old-a".to_vec()),
                ("b".into(), b"old-b".to_vec()),
            ])),
            fail_on: "b".into(),
        });
        engine.register_broker("plugin", broker.clone()).unwrap();
        for id in ["a", "b"] {
            store
                .upsert_resource(&ResourceDescriptor {
                    id: id.into(),
                    provider_id: "plugin".into(),
                    kind: "opaque".into(),
                    display_name: id.into(),
                    version: "1".into(),
                    location: Value::Null,
                    capabilities: vec!["replace".into()],
                    presentation: None,
                })
                .unwrap();
        }
        let old_a = format!("{:x}", Sha256::digest(b"old-a"));
        let old_b = format!("{:x}", Sha256::digest(b"old-b"));
        let execution = crate::extension::ToolExecution {
            effect: ToolEffect::LocalWrite,
            compensation: CompensationMode::SnapshotRestore,
            verification: VerificationMode::State,
            ..Default::default()
        };
        let plan = MutationPlan {
            id: "p".into(),
            provider_id: "plugin".into(),
            resources: vec![
                ResourcePrecondition {
                    resource_id: "a".into(),
                    expected_version: "1".into(),
                    expected_hash: old_a.clone(),
                },
                ResourcePrecondition {
                    resource_id: "b".into(),
                    expected_version: "1".into(),
                    expected_hash: old_b.clone(),
                },
            ],
            staged_outputs: vec![
                StagedOutput::ReplaceResource {
                    resource_id: "a".into(),
                    content_artifact: artifacts.put(b"new-a").unwrap(),
                    expected_hash: old_a,
                },
                StagedOutput::ReplaceResource {
                    resource_id: "b".into(),
                    content_artifact: artifacts.put(b"new-b").unwrap(),
                    expected_hash: old_b,
                },
            ],
            execution,
            verification: VerificationRequest {
                provider_id: "plugin".into(),
                method: "verify".into(),
                input: Value::Null,
            },
            compensation: CompensationPlan {
                restore_snapshots: vec![],
                provider_request: None,
            },
        };
        let operation = store.create(None, "partial", plan).unwrap();
        engine.request_authorization(&operation.id).unwrap();
        let result = engine.execute_pre_authorized(&operation.id).unwrap();
        assert_eq!(result.state, OperationState::RolledBack);
        let values = broker.values.lock().unwrap();
        assert_eq!(values["a"], b"old-a");
        assert_eq!(values["b"], b"old-b");
    }

    #[test]
    fn recovery_preserves_authorization_requests_and_quarantines_unknown_commits() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("test.db");
        let store = Arc::new(OperationStore::open(&database).unwrap());
        let artifacts =
            Arc::new(ArtifactStore::new(directory.path().join("artifacts"), 1024).unwrap());
        let engine = OperationEngine::new(store.clone(), artifacts.clone());
        let resource = ResourceDescriptor {
            id: "r".into(),
            provider_id: "p".into(),
            kind: "opaque".into(),
            display_name: "r".into(),
            version: "1".into(),
            location: Value::Null,
            capabilities: vec!["replace".into()],
            presentation: None,
        };
        store.upsert_resource(&resource).unwrap();
        let execution = crate::extension::ToolExecution {
            effect: ToolEffect::LocalWrite,
            verification: VerificationMode::State,
            ..Default::default()
        };
        let plan = MutationPlan {
            id: "plan".into(),
            provider_id: "p".into(),
            resources: vec![ResourcePrecondition {
                resource_id: "r".into(),
                expected_version: "1".into(),
                expected_hash: "hash".into(),
            }],
            staged_outputs: vec![StagedOutput::ReplaceResource {
                resource_id: "r".into(),
                content_artifact: "0".repeat(64),
                expected_hash: "hash".into(),
            }],
            execution,
            verification: VerificationRequest {
                provider_id: "p".into(),
                method: "verify".into(),
                input: Value::Null,
            },
            compensation: CompensationPlan {
                restore_snapshots: vec![],
                provider_request: None,
            },
        };
        let waiting = store.create(None, "waiting", plan.clone()).unwrap();
        engine.request_authorization(&waiting.id).unwrap();
        let mut committing = store.create(None, "committing", plan).unwrap();
        store
            .transition(&mut committing, OperationState::Validating)
            .unwrap();
        store
            .transition(&mut committing, OperationState::Preparing)
            .unwrap();
        store
            .transition(&mut committing, OperationState::Prepared)
            .unwrap();
        store
            .transition(&mut committing, OperationState::Committing)
            .unwrap();
        engine.recover().unwrap();
        assert_eq!(
            store.get(&waiting.id).unwrap().state,
            OperationState::AwaitingAuthorization
        );
        assert_eq!(
            store.get(&committing.id).unwrap().state,
            OperationState::NeedsReview
        );
    }

    #[test]
    fn missing_broker_fails_before_preparing_or_external_effects() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(OperationStore::open(&directory.path().join("test.db")).unwrap());
        let artifacts =
            Arc::new(ArtifactStore::new(directory.path().join("artifacts"), 1024).unwrap());
        let engine = OperationEngine::new(store.clone(), artifacts);
        let execution = crate::extension::ToolExecution {
            effect: ToolEffect::LocalWrite,
            verification: VerificationMode::State,
            ..Default::default()
        };
        let plan = MutationPlan {
            id: "p".into(),
            provider_id: "missing".into(),
            resources: vec![ResourcePrecondition {
                resource_id: "r".into(),
                expected_version: "1".into(),
                expected_hash: "hash".into(),
            }],
            staged_outputs: vec![StagedOutput::ReplaceResource {
                resource_id: "r".into(),
                content_artifact: "0".repeat(64),
                expected_hash: "hash".into(),
            }],
            execution,
            verification: VerificationRequest {
                provider_id: "missing".into(),
                method: "verify".into(),
                input: Value::Null,
            },
            compensation: CompensationPlan {
                restore_snapshots: vec![],
                provider_request: None,
            },
        };
        let operation = store.create(None, "missing", plan).unwrap();
        engine.request_authorization(&operation.id).unwrap();
        let result = engine.execute_pre_authorized(&operation.id).unwrap();
        assert_eq!(result.state, OperationState::Failed);
        assert!(result.checkpoint.held_leases.is_empty());
    }
}
