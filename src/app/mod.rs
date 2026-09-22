use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    bridge::{BgiClient, register_tools},
    config::{AppConfig, ModelAuth, ModelConfig, ModelOptions, ModelProtocol},
    error::{Error, Result},
    extension::plugins::PluginManager,
    extension::skills::SkillRegistry,
    extension::{FunctionTool, ToolExecution, ToolRegistry},
    model::list_remote_models,
    runtime::types::Event,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelView<'a> {
    id: &'a str,
    name: &'a str,
    protocol: crate::config::ModelProtocol,
    model: &'a str,
    base_url: &'a str,
    active: bool,
    timeout_ms: u64,
    context_window: u64,
    max_output_tokens: Option<u64>,
    auth: crate::config::ModelAuth,
    prompt_cache: bool,
}

struct Extensions {
    skills: Arc<SkillRegistry>,
    plugins: Arc<PluginManager>,
    tools: Arc<ToolRegistry>,
}

pub struct AppController {
    _process_lock: std::fs::File,
    config_edit: Mutex<()>,
    config_path: PathBuf,
    config: Mutex<AppConfig>,
    extensions: RwLock<Extensions>,
    supervisor: Arc<crate::runtime::Supervisor>,
    operations: Arc<crate::runtime::operation::operations::OperationEngine>,
}

impl AppController {
    pub fn shutdown(&self) {
        self.supervisor.shutdown();
    }
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let config_path = path.as_ref().to_path_buf();
        let config = AppConfig::load(&config_path)?;
        if let Some(parent) = config.storage.database.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let process_lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(config.storage.database.with_extension("runtime.lock"))?;
        process_lock
            .try_lock()
            .map_err(|_| Error::Conflict("该数据目录已有运行中的 Sleepy Doll".into()))?;
        let mut skills = SkillRegistry::default();
        let config_dir = config_directory(&config_path);
        skills.load(&skill_directories(&config, &config_dir))?;
        let mut tools = ToolRegistry::default();
        let mut plugins = PluginManager::default();
        plugins.load(
            &config.plugins.directories,
            &config.plugins.enabled,
            &config.plugins.disabled,
            &mut tools,
            &mut skills,
        )?;
        let skills = Arc::new(skills);
        let plugins = Arc::new(plugins);
        let bridge = Arc::new(BgiClient::new(config.bridge.clone()));
        if config.host_plugin_enabled() && config.bridge.enabled {
            register_tools(&mut tools, bridge.clone())?;
        }
        register_builtin_tools(
            &mut tools,
            skills.clone(),
            plugins.clone(),
            Arc::new(crate::runtime::workspace::Workspace::from_config(
                &config_path,
                (config.host_plugin_enabled() && config.bridge.enabled).then(|| bridge.clone()),
            )),
        )?;
        let tools = Arc::new(tools);
        let operation_store = Arc::new(
            crate::runtime::operation::operations::OperationStore::open(&config.storage.database)?,
        );
        let artifact_root = config.storage.database.with_extension("artifacts");
        let artifacts = Arc::new(crate::runtime::store::artifacts::ArtifactStore::new(
            artifact_root,
            64 * 1024 * 1024,
        )?);
        let operations = Arc::new(crate::runtime::operation::operations::OperationEngine::new(
            operation_store,
            artifacts,
        ));
        operations.configure_file_brokers(plugins.broker_specs())?;
        operations.configure_verifiers(plugins.adapters());
        let supervisor = crate::runtime::Supervisor::new(
            config.clone(),
            skills.clone(),
            tools.clone(),
            operations.clone(),
            plugins.adapters().to_vec(),
        )?;
        Ok(Self {
            _process_lock: process_lock,
            config_edit: Mutex::new(()),
            config_path,
            config: Mutex::new(config),
            extensions: RwLock::new(Extensions {
                skills,
                plugins,
                tools,
            }),
            supervisor,
            operations,
        })
    }

    pub fn handle(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        _emit: Arc<dyn Fn(String, Event) + Send + Sync>,
    ) -> Result<Value> {
        let _edit = if matches!(
            method,
            "model.use"
                | "bridge.setEnabled"
                | "bridge.setLaunchSilently"
                | "bridge.restore"
                | "model.save"
                | "model.delete"
                | "skill.setEnabled"
                | "skill.install"
                | "plugin.setEnabled"
                | "plugin.install"
                | "plugin.remove"
                | "extensions.reload"
                | "strategy.extract"
                | "strategy.run"
                | "workflow.run"
                | "workflow.extract"
                | "permission.set"
                | "config.write"
        ) {
            Some(self.config_edit.lock().unwrap())
        } else {
            None
        };
        match method {
            "plugin.install" => {
                let config = self.config.lock().unwrap().clone();
                let id = crate::runtime::host::installation::install(
                    &config_directory(&self.config_path),
                    &config,
                    Path::new(required(&params, "path")?),
                )?;
                self.reload_extensions()?;
                Ok(json!({"id":id}))
            }
            "skill.install" => {
                let root = AppConfig::ensure_user_skill_directory(&self.config_path)?;
                *self.config.lock().unwrap() = AppConfig::load(&self.config_path)?;
                let name = crate::extension::skills::install(
                    &root,
                    Path::new(required(&params, "path")?),
                )?;
                self.reload_extensions()?;
                Ok(json!({"name":name}))
            }
            "plugin.remove" => {
                let id = required(&params, "id")?;
                if crate::extension::providers::is_host_provider(id) {
                    return Err(Error::Config("随产品提供的插件不能移除。".into()));
                }
                let config = self.config.lock().unwrap().clone();
                crate::runtime::host::installation::remove(
                    &config_directory(&self.config_path),
                    &config,
                    id,
                )?;
                self.reload_extensions()?;
                Ok(json!({"removed":true,"recoverable":true}))
            }
            "extensions.reload" => {
                self.reload_extensions()?;
                Ok(json!({"reloaded":true}))
            }
            "config.read" => Ok(json!({
                "path": self.config_path.display().to_string(),
                "content": std::fs::read_to_string(&self.config_path)?,
            })),
            "config.write" => {
                crate::config::AppConfig::write_document(
                    &self.config_path,
                    required(&params, "content")?,
                )?;
                self.reload_runtime()?;
                Ok(json!({"saved": true}))
            }
            "bootstrap" => self.bootstrap(),
            "task.submit" | "run.submit" => {
                let prompt = params["prompt"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| Error::Config("prompt is required".into()))?
                    .to_owned();
                let key = params["clientKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                Ok(crate::runtime::types::public_run(&self.supervisor.submit(
                    &prompt,
                    params["conversationId"].as_str(),
                    &key,
                    params["durationSec"].as_i64(),
                    params["modelId"].as_str(),
                )?))
            }
            "task.get" | "run.get" => Ok(crate::runtime::types::public_run(
                &self.supervisor.journal.get(required(&params, "id")?)?,
            )),
            "task.cancel" | "run.cancel" => Ok(crate::runtime::types::public_run(
                &self.supervisor.cancel(required(&params, "id")?)?,
            )),
            "task.resume" | "run.resume" => Ok(crate::runtime::types::public_run(
                &self
                    .supervisor
                    .resume(required(&params, "id")?, params["durationSec"].as_i64())?,
            )),
            "run.input" => {
                self.supervisor.journal.input_once(
                    required(&params, "id")?,
                    "supplement",
                    required(&params, "content")?,
                    params["clientKey"].as_str(),
                )?;
                Ok(json!({"accepted":true}))
            }
            "approval.respond" => Ok(serde_json::to_value(
                self.supervisor.journal.decide(
                    required(&params, "id")?,
                    params["approved"]
                        .as_bool()
                        .ok_or_else(|| Error::Config("approved is required".into()))?,
                )?,
            )?),
            "events.read" => {
                let conversation = required(&params, "conversationId")?;
                let after = params["after"].as_u64().unwrap_or(0);
                let wait = params["waitMs"].as_u64().unwrap_or(0).min(25000);
                // 等待 journal 的通知，不轮询 SQLite；先注册等待再读事件，唤醒不会丢。
                let notifier = self.supervisor.journal.notifier();
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
                loop {
                    let changed = notifier.notified();
                    tokio::pin!(changed);
                    changed.as_mut().enable();
                    let events = self.supervisor.journal.events(conversation, after)?;
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if !events.is_empty() || remaining.is_zero() {
                        // 游标过旧时要求重取快照。
                        let expired = self
                            .supervisor
                            .journal
                            .cursor_expired(conversation, after)?;
                        return Ok(json!({
                            "events":events,
                            "snapshotRequired":expired,
                        }));
                    }
                    let _ = crate::runtime::executor()
                        .block_on(async { tokio::time::timeout(remaining, changed).await });
                }
            }
            "conversation.get" => {
                let id = required(&params, "id")?;
                Ok(json!({"id":id,"messages":self.supervisor.journal.conversation_messages(id)?}))
            }
            "conversation.list" => Ok(json!(self.supervisor.conversations(
                &crate::runtime::store::journal::ConversationQuery {
                    search: params["search"].as_str().map(str::to_owned),
                    include_archived: params["includeArchived"].as_bool().unwrap_or(false),
                    offset: params["offset"].as_u64().unwrap_or(0) as usize,
                    limit: params["limit"].as_u64().unwrap_or(50) as usize,
                }
            )?)),
            "conversation.rename" => {
                self.supervisor
                    .journal
                    .rename_conversation(required(&params, "id")?, required(&params, "title")?)?;
                Ok(json!({"saved":true}))
            }
            "conversation.setPinned" => {
                self.supervisor.journal.set_conversation_pinned(
                    required(&params, "id")?,
                    params["pinned"]
                        .as_bool()
                        .ok_or_else(|| Error::Config("pinned 必须是布尔值".into()))?,
                )?;
                Ok(json!({"saved":true}))
            }
            "conversation.setArchived" => {
                self.supervisor.journal.set_conversation_archived(
                    required(&params, "id")?,
                    params["archived"]
                        .as_bool()
                        .ok_or_else(|| Error::Config("archived 必须是布尔值".into()))?,
                )?;
                Ok(json!({"saved":true}))
            }
            "conversation.groups.save" => {
                let layout: crate::runtime::store::journal::GroupLayout =
                    serde_json::from_value(params.clone())?;
                self.supervisor.journal.save_conversation_groups(&layout)?;
                Ok(json!({"saved":true}))
            }
            "conversation.setModel" => {
                let id = required(&params, "id")?;
                let model = required(&params, "modelId")?;
                if !self
                    .config
                    .lock()
                    .unwrap()
                    .models
                    .iter()
                    .any(|entry| entry.id == model)
                {
                    return Err(Error::Config("模型配置不存在".into()));
                }
                self.supervisor
                    .journal
                    .set_conversation_model(id, Some(model))?;
                Ok(json!({"saved":true,"modelId":model}))
            }
            "conversation.delete" => {
                let id = required(&params, "id")?;
                let summary = self
                    .supervisor
                    .journal
                    .conversation(id)
                    .map(|conversation| {
                        json!({
                            "title":conversation.title,
                            "taskCount":conversation.task_count,
                        })
                    })
                    .unwrap_or_else(|_| json!({"title":"","taskCount":0}));
                if params["confirmed"].as_bool() != Some(true) {
                    return Ok(json!({
                        "requiresConfirmation":true,
                        "affects":summary,
                        "keeps":"快捷任务与运行证据会保留，来源显示为已删除",
                    }));
                }
                Ok(self.supervisor.journal.delete_conversation(id)?)
            }
            "task.list" => Ok(json!(
                self.supervisor
                    .journal
                    .list()?
                    .iter()
                    .map(crate::runtime::types::public_run)
                    .collect::<Vec<_>>()
            )),
            "conversation.fork" => Ok(json!({"id":self.supervisor.journal.fork_conversation(
                required(&params,"id")?,params["title"].as_str()
            )?})),
            "strategy.extract" => Ok(serde_json::to_value(
                self.supervisor
                    .journal
                    .create_strategy(required(&params, "runId")?, required(&params, "name")?)?,
            )?),
            "strategy.run" => {
                let key = params["clientKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                Ok(crate::runtime::types::public_run(
                    &self.supervisor.submit_strategy(
                        required(&params, "id")?,
                        &key,
                        params["durationSec"].as_i64(),
                    )?,
                ))
            }
            "workflow.extract" => self.extract_task(&params),
            "workflow.draft.create" | "workflow.draft.update" => self.save_draft(&params),
            "workflow.validate" => {
                let task_id = required(&params, "id")?;
                let revision = match params["draftRevision"].as_u64() {
                    Some(revision) => self.supervisor.tasks.revision(task_id, revision)?,
                    None => self
                        .supervisor
                        .tasks
                        .published_revision(task_id)?
                        .ok_or_else(|| Error::Config("这个快捷任务还没有可校验的定义".into()))?,
                };
                Ok(json!({
                    "validation":revision.validation,
                    "publishable":revision.validation.publishable(),
                    "zeroToken":revision.model_usage.is_deterministic(),
                    "modelUsage":revision.model_usage,
                }))
            }
            "workflow.publish" => self.publish_task(&params),
            "workflow.get" => {
                let id = required(&params, "id")?;
                let definition = self.supervisor.tasks.definition(id)?;
                let conversations = self
                    .supervisor
                    .journal
                    .conversations()?
                    .into_iter()
                    .map(|conversation| conversation.id)
                    .collect::<std::collections::HashSet<_>>();
                let names = self
                    .supervisor
                    .tool_definitions()
                    .into_iter()
                    .map(|definition| definition.name)
                    .collect::<std::collections::HashSet<_>>();
                let is_available = |name: &str| names.contains(name);
                let revision = definition
                    .published_revision
                    .and_then(|revision| self.supervisor.tasks.revision(id, revision).ok());
                Ok(json!({
                    "summary":self.supervisor.tasks.summary(&definition, &is_available, &self.supervisor.introduced_providers(), &conversations),
                    "definition":definition,
                    "revision":revision,
                    "runs":self.supervisor.tasks.run_ids(id, 20)?,
                }))
            }
            "workflow.list" => Ok(json!(
                self.supervisor
                    .task_summaries(params["conversationId"].as_str())?
            )),
            "workflow.run" => {
                let key = params["clientKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                Ok(crate::runtime::types::public_run(
                    &self.supervisor.submit_workflow(
                        required(&params, "id")?,
                        params["expectedPublishedRevision"].as_u64(),
                        &key,
                        params["durationSec"].as_i64(),
                    )?,
                ))
            }
            "workflow.rename" | "workflow.pin" | "workflow.archive" | "workflow.restore" => {
                self.patch_task(&params, method)
            }
            "workflow.delete" => {
                let id = required(&params, "id")?;
                let mut definition = self.supervisor.tasks.definition(id)?;
                // 逻辑删除，运行中的实例继续用已固定快照。
                definition.deleted_at = Some(crate::runtime::types::now());
                definition.updated_at = crate::runtime::types::now();
                self.supervisor.tasks.save_definition(&definition)?;
                Ok(json!({
                    "deleted":true,
                    "activeRuns":self.supervisor.task_run_ids(id)?,
                    "historyKept":true,
                }))
            }
            "workflow.copy" => {
                let source = self.supervisor.tasks.definition(required(&params, "id")?)?;
                let id = uuid::Uuid::new_v4().to_string();
                let mut definition = source.clone();
                definition.id = id.clone();
                definition.name = format!("{} 的副本", source.name);
                definition.published_revision = None;
                definition.draft_revision = None;
                definition.last_run_id = None;
                definition.pinned = false;
                definition.archived_at = None;
                definition.deleted_at = None;
                definition.created_at = crate::runtime::types::now();
                definition.updated_at = definition.created_at.clone();
                // 生成新 ID，保留来源说明，不复用原活动运行与外部 Job。
                definition.source_message_id = None;
                self.supervisor.tasks.create_definition(&definition)?;
                if let Some(mut revision) = self.supervisor.tasks.latest_revision(&source.id)? {
                    revision.task_id = id.clone();
                    self.supervisor.tasks.save_revision(&revision)?;
                    definition.draft_revision = Some(revision.revision);
                    self.supervisor.tasks.save_definition(&definition)?;
                }
                Ok(json!({"id":id}))
            }
            "operation.list" => Ok(json!(self.operations.store.list()?)),
            "operation.get" => Ok(json!(self.operations.store.get(required(&params, "id")?)?)),
            "operation.create" => {
                let plan: crate::runtime::operation::kernel::MutationPlan =
                    serde_json::from_value(params["plan"].clone())?;
                let operation = self.operations.store.create(
                    params["runId"].as_str(),
                    required(&params, "title")?,
                    plan,
                )?;
                Ok(json!(self.operations.request_authorization(&operation.id)?))
            }
            "operation.execute" => Ok(json!(
                self.operations
                    .execute_pre_authorized(required(&params, "id")?)?
            )),
            "operation.rollback" => Ok(json!(self.operations.rollback(required(&params, "id")?)?)),
            "operation.cancel" => Ok(json!(self.operations.cancel(required(&params, "id")?)?)),
            "resource.list" => Ok(json!(self.operations.store.resources()?)),
            "resource.snapshot" => {
                let resource = self.operations.store.resource(required(&params, "id")?)?;
                Ok(json!(self.operations.snapshot(&resource)?))
            }
            "resource.inspect" => {
                let resource = self.operations.store.resource(required(&params, "id")?)?;
                let snapshot = self.operations.snapshot(&resource)?;
                let content = self.operations.artifacts.get(&snapshot.content_artifact)?;
                let extensions = self.extensions.read().unwrap();
                let adapter = extensions
                    .plugins
                    .adapters()
                    .iter()
                    .find(|adapter| adapter.provider_id() == resource.provider_id)
                    .ok_or_else(|| Error::Config("资源所属 Adapter 当前不可用".into()))?;
                let view = adapter.inspect(&snapshot, &content, params["request"].clone())?;
                Ok(json!({"snapshot":snapshot,"view":view}))
            }
            "run.checkpoint" => Ok(json!(
                self.supervisor
                    .journal
                    .checkpoint(required(&params, "id")?)?
            )),
            "diagnostics.list" => Ok(json!(self.operations.store.diagnostics()?)),
            "metrics.list" => {
                Ok(json!(self.operations.store.metrics(
                    params["limit"].as_u64().unwrap_or(200) as usize
                )?))
            }
            "preferences.list" => {
                Ok(json!(self.operations.store.preferences(
                    params["scope"].as_str().unwrap_or("global")
                )?))
            }
            "preferences.save" => {
                let preference: crate::runtime::operation::kernel::PreferenceRecord =
                    serde_json::from_value(params["preference"].clone())?;
                self.operations.store.save_preference(&preference)?;
                Ok(json!({"saved":true}))
            }
            "preferences.delete" => {
                self.operations
                    .store
                    .delete_preference(required(&params, "key")?, required(&params, "scope")?)?;
                Ok(json!({"deleted":true}))
            }
            "grant.list" => Ok(json!(self.operations.store.grants()?)),
            "grant.save" => {
                let grant: crate::runtime::operation::permissions::TrustGrant =
                    serde_json::from_value(params["grant"].clone())?;
                self.operations.store.save_grant(&grant)?;
                Ok(json!({"saved":true}))
            }
            "grant.revoke" => {
                self.operations
                    .store
                    .revoke_grant(required(&params, "id")?)?;
                Ok(json!({"revoked":true}))
            }
            "notification.list" => {
                Ok(json!(self.operations.store.notifications(
                    params["unreadOnly"].as_bool().unwrap_or(true)
                )?))
            }
            "notification.read" => {
                self.operations
                    .store
                    .mark_notification_read(required(&params, "id")?)?;
                Ok(json!({"read":true}))
            }
            "adapter.discover" => {
                let plugin_id = required(&params, "pluginId")?;
                let adapter_id = required(&params, "adapterId")?;
                let extensions = self.extensions.read().unwrap();
                let adapter = extensions
                    .plugins
                    .adapters()
                    .iter()
                    .find(|adapter| {
                        adapter.plugin_id == plugin_id && adapter.adapter_id == adapter_id
                    })
                    .ok_or_else(|| Error::Config("Adapter 不存在或未启用".into()))?;
                let resources = adapter.discover(params["input"].clone())?;
                for resource in &resources {
                    if resource.provider_id != adapter.provider_id() {
                        return Err(Error::Tool("Adapter 返回了越权 Provider 资源".into()));
                    }
                    self.operations.store.upsert_resource(resource)?;
                }
                Ok(json!({"resources":resources}))
            }
            "adapter.planMutation" => {
                let plugin_id = required(&params, "pluginId")?;
                let adapter_id = required(&params, "adapterId")?;
                let extensions = self.extensions.read().unwrap();
                let adapter = extensions
                    .plugins
                    .adapters()
                    .iter()
                    .find(|adapter| {
                        adapter.plugin_id == plugin_id && adapter.adapter_id == adapter_id
                    })
                    .ok_or_else(|| Error::Config("Adapter 不存在或未启用".into()))?;
                let snapshot_ids = params["snapshotIds"]
                    .as_array()
                    .ok_or_else(|| Error::Config("snapshotIds must be an array".into()))?;
                let snapshots = snapshot_ids
                    .iter()
                    .map(|id| {
                        let snapshot = self.operations.store.snapshot(id.as_str().unwrap_or(""))?;
                        let content = self.operations.artifacts.get(&snapshot.content_artifact)?;
                        Ok((snapshot, content))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let plan = adapter.plan(
                    &snapshots,
                    params["request"].clone(),
                    &self.operations.artifacts,
                )?;
                let operation = self.operations.store.create(
                    params["runId"].as_str(),
                    required(&params, "title")?,
                    plan,
                )?;
                let operation = self.operations.request_authorization(&operation.id)?;
                Ok(json!({"operation":operation,"requiresAuthorization":true}))
            }
            "adapter.diagnose" => {
                let plugin_id = required(&params, "pluginId")?;
                let adapter_id = required(&params, "adapterId")?;
                let extensions = self.extensions.read().unwrap();
                let adapter = extensions
                    .plugins
                    .adapters()
                    .iter()
                    .find(|adapter| {
                        adapter.plugin_id == plugin_id && adapter.adapter_id == adapter_id
                    })
                    .ok_or_else(|| Error::Config("Adapter 不存在或未启用".into()))?;
                let findings = adapter.diagnose(params["input"].clone())?;
                for finding in &findings {
                    if finding.provider_id != adapter.provider_id() {
                        return Err(Error::Tool("Adapter 返回了越权诊断结果".into()));
                    }
                    self.operations.store.save_diagnostic(finding)?;
                }
                Ok(json!({"findings":findings}))
            }
            "permission.get" => {
                use crate::runtime::operation::permissions::PermissionMode;
                let mode = self.config.lock().unwrap().runtime.permission_mode;
                Ok(json!({
                    "mode":mode,
                    "label":mode.label(),
                    "description":mode.description(),
                    "levels":PermissionMode::levels()
                        .iter()
                        .map(|(mode, label, description)| json!({
                            "value":mode,
                            "label":label,
                            "description":description,
                        }))
                        .collect::<Vec<_>>(),
                }))
            }
            "permission.set" => {
                use crate::runtime::operation::permissions::PermissionMode;
                let mode: PermissionMode = serde_json::from_value(params["mode"].clone())
                    .map_err(|_| Error::Config("未知的审批级别".into()))?;
                AppConfig::set_permission_mode(&self.config_path, mode)?;
                self.reload_runtime()?;
                Ok(json!({"mode":mode,"label":mode.label()}))
            }
            "model.use" => self.use_model(required(&params, "id")?),
            "model.list" => self.list_models(&params),
            "model.save" => {
                AppConfig::save_model(&self.config_path, &params["model"])?;
                self.reload_runtime()?;
                Ok(json!({"saved":true}))
            }
            "model.delete" => self.delete_model(required(&params, "id")?),
            "skill.setEnabled" => {
                let name = required(&params, "name")?;
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or_else(|| Error::Config("enabled is required".into()))?;
                AppConfig::set_skill_enabled(&self.config_path, name, enabled)?;
                self.reload_runtime()?;
                Ok(json!({"enabled":enabled}))
            }
            "plugin.setEnabled" => {
                let id = required(&params, "id")?;
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or_else(|| Error::Config("enabled is required".into()))?;
                AppConfig::set_plugin_enabled(&self.config_path, id, enabled)?;
                self.reload_runtime()?;
                self.reload_extensions()?;
                Ok(json!({"restartRequired":false}))
            }
            "bridge.state" => BgiClient::new(self.config.lock().unwrap().bridge.clone()).state(),
            "bridge.recoveryList" => crate::bridge::control::recovery("list", &[]),
            "bridge.restore" => crate::bridge::control::recovery(
                "restore",
                &[
                    required(&params, "changeId")?,
                    required(&params, "recordVersion")?,
                    required(&params, "currentVersion")?,
                ],
            ),
            "bridge.catalog" | "bridge.describe" => {
                let mut config = self.config.lock().unwrap().bridge.clone();
                config.enabled = true;
                let client = BgiClient::new(config);
                if method == "bridge.describe" {
                    client.describe(required(&params, "methodId")?)
                } else {
                    client.catalog_page(
                        params["query"].as_str().unwrap_or(""),
                        params["group"].as_str(),
                        params["offset"].as_u64().unwrap_or(0),
                    )
                }
            }
            "bridge.setEnabled" => {
                let enabled = params["enabled"]
                    .as_bool()
                    .ok_or_else(|| Error::Config("enabled 必须是布尔值".into()))?;
                self.set_bridge_enabled(enabled)
            }
            "bridge.setLaunchSilently" => {
                let silently = params["silently"]
                    .as_bool()
                    .ok_or_else(|| Error::Config("silently 必须是布尔值".into()))?;
                self.set_bridge_launch_silently(silently)
            }
            _ => Err(Error::Config(format!("unknown IPC method: {method}"))),
        }
    }

    /// 保存或更新草稿，不执行、不发布、不产生外部写入。
    fn save_draft(&self, params: &Value) -> Result<Value> {
        use crate::runtime::operation::task::{TaskLimits, TaskNode, compile};

        let nodes: Vec<TaskNode> = serde_json::from_value(params["nodes"].clone())
            .map_err(|error| Error::Config(format!("任务定义无法解析：{error}")))?;
        let limits: Option<TaskLimits> = match params.get("limits") {
            Some(value) if !value.is_null() => Some(serde_json::from_value(value.clone())?),
            _ => None,
        };
        let catalog = self.supervisor.tool_catalog();
        let existing = params["id"]
            .as_str()
            .map(str::to_owned)
            .filter(|id| !id.is_empty());
        let task_id = existing
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let definition = match &existing {
            Some(id) => self.supervisor.tasks.definition(id)?,
            None => crate::runtime::operation::task::WorkflowDefinition {
                id: task_id.clone(),
                name: params["name"].as_str().unwrap_or("新快捷任务").into(),
                description: params["description"].as_str().unwrap_or("").into(),
                source_conversation_id: params["sourceConversationId"].as_str().map(str::to_owned),
                source_message_id: params["sourceMessageId"].as_str().map(str::to_owned),
                source_title_snapshot: params["sourceTitleSnapshot"].as_str().unwrap_or("").into(),
                published_revision: None,
                draft_revision: None,
                archived_at: None,
                deleted_at: None,
                pinned: false,
                last_run_id: None,
                created_at: crate::runtime::types::now(),
                updated_at: crate::runtime::types::now(),
            },
        };
        if definition.deleted_at.is_some() {
            return Err(Error::Conflict("已删除的任务不能继续编辑".into()));
        }
        let revision_number = self.supervisor.tasks.next_revision_number(&task_id)?;
        let revision = compile(
            &task_id,
            revision_number,
            params["name"].as_str().unwrap_or(&definition.name),
            params["description"]
                .as_str()
                .unwrap_or(&definition.description),
            nodes,
            limits,
            &catalog,
        )?;
        let mut definition = definition;
        definition.draft_revision = Some(revision_number);
        definition.updated_at = crate::runtime::types::now();
        if existing.is_some() {
            self.supervisor.tasks.save_definition(&definition)?;
        } else {
            self.supervisor.tasks.create_definition(&definition)?;
        }
        self.supervisor.tasks.save_revision(&revision)?;
        Ok(json!({
            "id":task_id,
            "revision":revision_number,
            "validation":revision.validation,
            "state":"draft",
        }))
    }

    /// 发布一份不可变修订，本身不产生真实工具写入。
    fn publish_task(&self, params: &Value) -> Result<Value> {
        let id = required(params, "id")?;
        let revision_number = params["draftRevision"]
            .as_u64()
            .ok_or_else(|| Error::Config("需要指出要发布的草稿版本".into()))?;
        let revision = self.supervisor.tasks.revision(id, revision_number)?;
        if !revision.validation.publishable() {
            return Err(Error::Conflict(format!(
                "任务还不能发布：{}",
                revision
                    .validation
                    .issues
                    .first()
                    .map(|issue| issue.message.clone())
                    .unwrap_or_else(|| "缺少必要信息".into())
            )));
        }
        let mut definition = self.supervisor.tasks.definition(id)?;
        if let Some(expected) = params["expectedPublishedRevision"].as_u64()
            && definition
                .published_revision
                .is_some_and(|current| current != expected)
        {
            return Err(Error::Conflict(
                "任务已经发布过更新的版本，请再试一次".into(),
            ));
        }
        definition.published_revision = Some(revision_number);
        definition.draft_revision = None;
        definition.name = revision.name.clone();
        definition.description = revision.description.clone();
        definition.updated_at = crate::runtime::types::now();
        self.supervisor.tasks.save_definition(&definition)?;
        Ok(json!({
            "taskId":id,
            "publishedRevision":revision_number,
            "zeroToken":revision.model_usage.is_deterministic(),
        }))
    }

    /// 改名、置顶、归档、恢复，都不触碰执行语义与稳定 ID。
    fn patch_task(&self, params: &Value, method: &str) -> Result<Value> {
        use crate::runtime::operation::task_store::DefinitionPatch;
        let id = required(params, "id")?;
        let mut definition = self.supervisor.tasks.definition(id)?;
        let patch = match method {
            "workflow.rename" => DefinitionPatch {
                name: Some(required(params, "name")?.to_owned()),
                description: params["description"].as_str().map(str::to_owned),
                pinned: None,
            },
            "workflow.pin" => DefinitionPatch {
                pinned: Some(
                    params["pinned"]
                        .as_bool()
                        .ok_or_else(|| Error::Config("pinned 必须是布尔值".into()))?,
                ),
                ..DefinitionPatch::default()
            },
            "workflow.archive" => {
                definition.archived_at = Some(crate::runtime::types::now());
                definition.updated_at = crate::runtime::types::now();
                self.supervisor.tasks.save_definition(&definition)?;
                return Ok(json!({"archived":true}));
            }
            _ => {
                definition.archived_at = None;
                definition.updated_at = crate::runtime::types::now();
                self.supervisor.tasks.save_definition(&definition)?;
                return Ok(json!({"archived":false}));
            }
        };
        definition.apply(patch)?;
        self.supervisor.tasks.save_definition(&definition)?;
        Ok(json!({"saved":true}))
    }

    /// 从一次已验证运行提取快捷任务，固定执行契约与真实资源。
    fn extract_task(&self, params: &Value) -> Result<Value> {
        use crate::runtime::operation::task::{
            FailurePolicy, SequenceNode, TaskNode, ToolNode, compile,
        };
        let run_id = required(params, "runId")?;
        let run = self.supervisor.journal.get(run_id)?;
        if !matches!(
            run.state,
            crate::runtime::types::RunState::Succeeded | crate::runtime::types::RunState::Answered
        ) {
            return Err(Error::Conflict(
                "只有已验证成功的运行可以提取为快捷任务".into(),
            ));
        }
        let plan = self
            .supervisor
            .journal
            .plan(run_id)?
            .ok_or_else(|| Error::Conflict("该运行没有结构化计划".into()))?;
        let outcomes = self.supervisor.journal.step_outcomes(run_id)?;
        let mut nodes = Vec::new();
        for step in plan.steps {
            if !matches!(
                outcomes.get(&step.id).map(String::as_str),
                Some("completed" | "verifiedSucceeded")
            ) {
                return Err(Error::Conflict("计划包含没有完成验证的步骤".into()));
            }
            let tool = step.tool.ok_or_else(|| {
                Error::Conflict("包含旧版领域能力的运行请使用兼容策略提取".into())
            })?;
            nodes.push(TaskNode::Tool(ToolNode {
                id: step.id,
                title: step.title,
                tool: Some(tool),
                capability_id: step.capability_id,
                // 旧 Job ID 是历史证据，不是新运行的输入参数。
                arguments: step.arguments,
                execution: step.execution,
                provider_version: step.provider_version,
                resource_versions: step.resource_versions,
                on_failure: FailurePolicy::Stop,
                on_unverified: FailurePolicy::Stop,
            }));
        }
        let nodes = vec![TaskNode::Sequence(SequenceNode {
            id: "steps".into(),
            title: plan.goal.clone(),
            nodes,
        })];
        let catalog = self.supervisor.tool_catalog();
        let task_id = uuid::Uuid::new_v4().to_string();
        let revision = compile(
            &task_id,
            1,
            params["name"].as_str().unwrap_or(&plan.goal),
            "由已验证运行提取",
            nodes,
            None,
            &catalog,
        )?;
        let definition = crate::runtime::operation::task::WorkflowDefinition {
            id: task_id.clone(),
            name: revision.name.clone(),
            description: revision.description.clone(),
            source_conversation_id: Some(run.conversation_id.clone()),
            source_message_id: None,
            source_title_snapshot: plan.goal.clone(),
            // 这一份修订还没有执行过，状态仍是「尚未实机验证」。
            published_revision: Some(1),
            draft_revision: None,
            archived_at: None,
            deleted_at: None,
            pinned: false,
            last_run_id: None,
            created_at: crate::runtime::types::now(),
            updated_at: crate::runtime::types::now(),
        };
        self.supervisor.tasks.create_definition(&definition)?;
        self.supervisor.tasks.save_revision(&revision)?;
        Ok(json!({
            "id":task_id,
            "taskId":task_id,
            "revision":1,
            "validation":revision.validation,
        }))
    }

    fn set_bridge_enabled(&self, enabled: bool) -> Result<Value> {
        if enabled {
            self.connect_bridge()?;
            Ok(json!({"enabled":true}))
        } else {
            let mut config = self.config.lock().unwrap().bridge.clone();
            config.enabled = false;
            AppConfig::set_bridge(&self.config_path, &config)?;
            self.reload_runtime()?;
            self.reload_extensions()?;
            let warning = crate::bridge::control::disable(&config);
            log::info!(
                "BetterGI 连接已关闭：{}",
                warning.as_deref().unwrap_or("已断开")
            );
            Ok(json!({"enabled":false,"warning":warning}))
        }
    }

    /// 连接 BetterGI 的完整流程：宿主没运行就自动启动，注入后记住安装位置。
    pub fn connect_bridge(&self) -> Result<()> {
        let mut config = self.config.lock().unwrap().bridge.clone();
        log::info!("正在连接 BetterGI（{}）", config.base_url);
        crate::bridge::control::prepare(&mut config)
            .inspect_err(|error| log::warn!("准备桥失败：{error}"))?;
        AppConfig::set_bridge(&self.config_path, &config)?;
        self.reload_runtime()?;
        crate::bridge::control::enable(&config)
            .inspect_err(|error| log::warn!("注入或启用桥失败：{error}"))?;
        config.enabled = true;
        config.instance_id = None;
        let remembered = BgiClient::new(config.clone())
            .host()
            .ok()
            .and_then(|host| host["installPath"].as_str().map(str::to_owned));
        if let Some(install) = remembered {
            let install = std::path::PathBuf::from(install);
            if config.host_install_path.as_deref() != Some(install.as_path()) {
                log::info!("记住 BetterGI 安装位置：{}", install.display());
                config.host_install_path = Some(install);
            }
        }
        AppConfig::set_bridge(&self.config_path, &config)?;
        self.reload_runtime()?;
        self.reload_extensions()?;
        log::info!("BetterGI 已连接（{}）", config.base_url);
        Ok(())
    }

    fn set_bridge_launch_silently(&self, silently: bool) -> Result<Value> {
        let mut current = self.config.lock().expect("config mutex poisoned");
        let mut bridge = current.bridge.clone();
        bridge.launch_silently = silently;
        AppConfig::set_bridge(&self.config_path, &bridge)?;
        current.bridge = bridge;
        Ok(json!({"launchSilently":silently}))
    }

    /// 桥开关当前是否打开。给桌面壳的监视循环用。
    pub fn bridge_enabled(&self) -> bool {
        self.config.lock().unwrap().bridge.enabled
    }

    /// 桥是否已连上（短超时探测）。给桌面壳的监视循环用。
    pub fn bridge_connected(&self) -> bool {
        let mut config = self.config.lock().unwrap().bridge.clone();
        config.timeout_ms = config.timeout_ms.min(2000);
        BgiClient::new(config).info().is_ok()
    }

    fn bootstrap(&self) -> Result<Value> {
        self.ensure_conversation_models()?;
        let extensions = self.extensions.read().unwrap();
        let config = self.config.lock().expect("config mutex poisoned");
        let models = config
            .models
            .iter()
            .map(|model| ModelView {
                id: &model.id,
                name: &model.name,
                protocol: model.protocol,
                model: &model.model,
                base_url: &model.base_url,
                active: model.id == config.active_model,
                timeout_ms: model.options.timeout_ms,
                context_window: model.options.context_window,
                max_output_tokens: model.options.max_output_tokens,
                auth: model.auth,
                prompt_cache: model.options.prompt_cache,
            })
            .collect::<Vec<_>>();
        let permission = {
            use crate::runtime::operation::permissions::PermissionMode;
            let mode = config.runtime.permission_mode;
            json!({
                "mode":mode,
                "label":mode.label(),
                "description":mode.description(),
                "levels":PermissionMode::levels()
                    .iter()
                    .map(|(mode, label, description)| json!({
                        "value":mode,
                        "label":label,
                        "description":description,
                    }))
                    .collect::<Vec<_>>(),
            })
        };
        let skill_environment = self.supervisor.skill_environment(&config);
        let skill_context = skill_environment.context();
        let online: Vec<&str> = skill_context.providers.iter().map(String::as_str).collect();
        let skills = extensions
            .skills
            .standalone()
            .into_iter()
            .map(|skill| {
                json!({
                    "name":skill.name,
                    "description":skill.description,
                    "source":skill.source,
                    "tags":skill.tags,
                    "requiresPlugins":skill.requires_plugins,
                    "requiresCapabilities":skill.requires_capabilities,
                    "resourceKinds":skill.resource_kinds,
                    "platforms":skill.platforms,
                    "allowedTools":skill.allowed_tools,
                    "alwaysLoad":skill.always_load,
                    "requiresProviders":skill.requires_providers,
                    "instructions":skill.body,
                    // 「启用」是用户的开关，「可用」还取决于依赖是否在线。
                    "enabled":!config.agent.disabled_skills.contains(&skill.name),
                    "available":!config.agent.disabled_skills.contains(&skill.name)
                        && extensions.skills.eligible(&skill, &skill_context),
                    "unavailableReason": crate::extension::providers::skill_unavailable_reason(
                        &skill.requires_providers,
                        &online,
                    ),
                })
            })
            .collect::<Vec<_>>();
        let bridge_status = if config.bridge.enabled {
            match crate::bridge::control::info(&config.bridge) {
                Ok(info) => {
                    json!({"enabled":true,"connected":info["enabled"] != false,"baseUrl":config.bridge.base_url,"launchSilently":config.bridge.launch_silently})
                }
                Err(error) => {
                    json!({"enabled":true,"connected":false,"baseUrl":config.bridge.base_url,"launchSilently":config.bridge.launch_silently,"error":error.to_string()})
                }
            }
        } else {
            json!({"enabled":false,"connected":false,"baseUrl":config.bridge.base_url,"launchSilently":config.bridge.launch_silently})
        };
        let plugins = extensions
            .plugins
            .list()
            .iter()
            .map(|plugin| {
                json!({
                    "manifest":{"id":plugin.manifest.id,"name":plugin.manifest.name,"version":plugin.manifest.version,"description":plugin.manifest.description},
                    "status":plugin.state,
                    "error":plugin.error.as_ref().map(|_|"插件加载失败，请检查配置"),
                    "configuredEnabled":config.plugin_enabled(&plugin.manifest.id),
                    // 随产品提供的插件不能移除。
                    "host":crate::extension::providers::is_host_provider(&plugin.manifest.id)
                })
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"permission":permission,"configPath":self.config_path.display().to_string(),"models":models,"skills":skills,"plugins":plugins,"tools":extensions.tools.definitions(),"conversations":self.supervisor.journal.conversations()?,"tasks":self.supervisor.journal.list()?.iter().map(crate::runtime::types::public_run).collect::<Vec<_>>(),"strategies":self.supervisor.journal.strategies()?,"workflows":self.supervisor.task_summaries(None)?,"operations":self.operations.store.list()?,"resources":self.operations.store.resources()?,"diagnostics":self.operations.store.diagnostics()?,"notifications":self.operations.store.notifications(true)?,"conversationGroups":self.supervisor.journal.conversation_groups()?,"bridge":bridge_status}),
        )
    }

    fn reload_extensions(&self) -> Result<()> {
        let config = AppConfig::load(&self.config_path)?;
        let config_dir = config_directory(&self.config_path);
        let mut skills = SkillRegistry::default();
        skills.load(&skill_directories(&config, &config_dir))?;
        let mut tools = ToolRegistry::default();
        let mut plugins = PluginManager::default();
        plugins.load(
            &config.plugins.directories,
            &config.plugins.enabled,
            &config.plugins.disabled,
            &mut tools,
            &mut skills,
        )?;
        let skills = Arc::new(skills);
        let plugins = Arc::new(plugins);
        let bridge = Arc::new(BgiClient::new(config.bridge.clone()));
        if config.host_plugin_enabled() && config.bridge.enabled {
            register_tools(&mut tools, bridge.clone())?;
        }
        register_builtin_tools(
            &mut tools,
            skills.clone(),
            plugins.clone(),
            Arc::new(crate::runtime::workspace::Workspace::from_config(
                &self.config_path,
                (config.host_plugin_enabled() && config.bridge.enabled).then(|| bridge.clone()),
            )),
        )?;
        let tools = Arc::new(tools);
        self.operations
            .configure_file_brokers(plugins.broker_specs())?;
        self.operations.configure_verifiers(plugins.adapters());
        self.supervisor.update_extensions(
            skills.clone(),
            tools.clone(),
            plugins.adapters().to_vec(),
        );
        *self.extensions.write().unwrap() = Extensions {
            skills,
            plugins,
            tools,
        };
        Ok(())
    }
    fn list_models(&self, params: &Value) -> Result<Value> {
        let protocol: ModelProtocol = serde_json::from_value(params["protocol"].clone())
            .map_err(|_| Error::Config("未知的模型协议".into()))?;
        let auth = params
            .get("auth")
            .cloned()
            .filter(|value| !value.is_null())
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(ModelAuth::Auto);
        let mut api_key = params["apiKey"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if api_key.is_none()
            && let Some(id) = params["id"].as_str().filter(|id| !id.is_empty())
        {
            api_key = self
                .config
                .lock()
                .expect("config mutex poisoned")
                .models
                .iter()
                .find(|model| model.id == id)
                .and_then(|model| model.api_key.clone());
        }
        let config = ModelConfig {
            id: "probe".into(),
            name: "probe".into(),
            protocol,
            model: String::new(),
            base_url: params["baseUrl"].as_str().unwrap_or("").to_string(),
            api_key,
            auth,
            headers: HashMap::new(),
            options: ModelOptions::default(),
        };
        let models = list_remote_models(&config, params["modelsUrl"].as_str())?;
        Ok(json!({ "models": models }))
    }

    fn use_model(&self, model_id: &str) -> Result<Value> {
        AppConfig::set_active(&self.config_path, model_id)?;
        self.reload_runtime()?;
        Ok(json!({"activeModel":model_id}))
    }

    fn delete_model(&self, id: &str) -> Result<Value> {
        let active = AppConfig::delete_model(&self.config_path, id)?;
        self.reload_runtime()?;
        self.ensure_conversation_models()?;
        Ok(json!({"deleted":true,"activeModel":active}))
    }

    fn ensure_conversation_models(&self) -> Result<()> {
        let (ids, default) = {
            let config = self.config.lock().expect("config mutex poisoned");
            (
                config
                    .models
                    .iter()
                    .map(|model| model.id.clone())
                    .collect::<Vec<_>>(),
                config.active_model.clone(),
            )
        };
        self.supervisor.journal.rebind_models(&ids, &default)
    }

    fn reload_runtime(&self) -> Result<()> {
        let config = AppConfig::load(&self.config_path)?;
        self.supervisor.update_config(config.clone())?;
        *self.config.lock().expect("config mutex poisoned") = config;
        Ok(())
    }
}

/// 配置里所有相对路径的基准，也是用户自己的技能与插件所在的目录。
fn config_directory(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// 配置里列出的技能目录及其归属。
fn skill_directories(
    config: &AppConfig,
    config_dir: &Path,
) -> Vec<(PathBuf, crate::extension::skills::SkillSource)> {
    config
        .agent
        .skill_directories
        .iter()
        .map(|path| {
            (
                path.clone(),
                crate::config::skill_directory_source(config_dir, path),
            )
        })
        .collect()
}

fn required<'a>(params: &'a Value, field: &str) -> Result<&'a str> {
    params[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Config(format!("{field} is required")))
}

fn register_builtin_tools(
    registry: &mut ToolRegistry,
    skills: Arc<SkillRegistry>,
    plugins: Arc<PluginManager>,
    workspace: Arc<crate::runtime::workspace::Workspace>,
) -> Result<()> {
    let core_read = || ToolExecution {
        deferred: false,
        always_load: true,
        ..ToolExecution::read_only()
    };
    let skills_search = skills.clone();
    registry.register(
        FunctionTool::new("skills.search", "仅在任务需要额外的已安装操作手册时搜索 Skill。已经自动加载的手册不需要先搜索或读取。", json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"],"additionalProperties":false}), "core:skills", move |arguments| serde_json::to_value(skills_search.search(arguments["query"].as_str().unwrap_or_default(), arguments["limit"].as_u64().unwrap_or(8) as usize)).map_err(Error::from))
            .with_label("检索技能")
            .with_execution(core_read()),
    )?;
    registry.register(
        FunctionTool::new("skills.read", "读取已发现但未自动加载的 Skill。当前上下文已有完整说明时不要重复读取。", json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}), "core:skills", move |arguments| skills.get(arguments["name"].as_str().unwrap_or_default()).map(|skill| json!({"name":skill.name,"description":skill.description,"instructions":skill.body})).ok_or_else(|| Error::Tool("skill not found".into())))
            .with_label("读取技能")
            .with_execution(core_read()),
    )?;
    registry.register(
        FunctionTool::new(
            "plugins.list",
            "仅在任务明确涉及扩展时列出已安装插件及启用状态。",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            "core:plugins",
            move |_| Ok(plugins.public_list()),
        )
        .with_label("读取插件列表")
        .with_execution(core_read()),
    )?;
    crate::runtime::workspace::register_tools(registry, workspace)?;
    Ok(())
}
