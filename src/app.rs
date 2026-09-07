use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    agent::AgentEvent,
    bgi::{BgiClient, register_tools},
    config::AppConfig,
    error::{Error, Result},
    plugins::PluginManager,
    skills::SkillRegistry,
    store::Store,
    tools::{FunctionTool, ToolExecution, ToolRegistry},
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
    store: Arc<Store>,
    bridge: Arc<BgiClient>,
    supervisor: Arc<crate::runtime::Supervisor>,
    operations: Arc<crate::runtime::operations::OperationEngine>,
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
        skills.load(
            &config
                .agent
                .skill_directories
                .iter()
                .cloned()
                .map(|path| (path, "workspace".into()))
                .collect::<Vec<_>>(),
        )?;
        let mut tools = ToolRegistry::default();
        let mut plugins = PluginManager::default();
        plugins.load(
            &config.plugins.directories,
            &config.plugins.enabled,
            &mut tools,
            &mut skills,
        )?;
        let skills = Arc::new(skills);
        let plugins = Arc::new(plugins);
        let bridge = Arc::new(BgiClient::new(config.bridge.clone()));
        if config.bridge.enabled {
            register_tools(&mut tools, bridge.clone())?;
        }
        register_builtin_tools(&mut tools, skills.clone(), plugins.clone())?;
        let tools = Arc::new(tools);
        let store = Arc::new(Store::open(&config.storage.database)?);
        let operation_store = Arc::new(crate::runtime::operations::OperationStore::open(
            &config.storage.database,
        )?);
        let artifact_root = config.storage.database.with_extension("artifacts");
        let artifacts = Arc::new(crate::runtime::artifacts::ArtifactStore::new(
            artifact_root,
            64 * 1024 * 1024,
        )?);
        let operations = Arc::new(crate::runtime::operations::OperationEngine::new(
            operation_store,
            artifacts,
        ));
        operations.configure_file_brokers(plugins.broker_specs())?;
        operations.configure_verifiers(plugins.adapters());
        let supervisor = crate::runtime::Supervisor::new(
            config.clone(),
            store.clone(),
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
            store,
            bridge,
            supervisor,
            operations,
        })
    }

    pub fn handle(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        _emit: Arc<dyn Fn(String, AgentEvent) + Send + Sync>,
    ) -> Result<Value> {
        let _edit = if matches!(
            method,
            "model.use"
                | "model.save"
                | "skill.setEnabled"
                | "plugin.setEnabled"
                | "plugin.install"
                | "plugin.remove"
                | "extensions.reload"
                | "strategy.extract"
                | "strategy.run"
                | "workflow.run"
                | "workflow.extract"
        ) {
            Some(self.config_edit.lock().unwrap())
        } else {
            None
        };
        match method {
            "plugin.install" => {
                let config = self.config.lock().unwrap().clone();
                let id = crate::runtime::installation::install(
                    &config,
                    Path::new(required(&params, "path")?),
                )?;
                self.reload_extensions()?;
                Ok(json!({"id":id}))
            }
            "plugin.remove" => {
                let config = self.config.lock().unwrap().clone();
                crate::runtime::installation::remove(&config, required(&params, "id")?)?;
                self.reload_extensions()?;
                Ok(json!({"removed":true,"recoverable":true}))
            }
            "extensions.reload" => {
                self.reload_extensions()?;
                Ok(json!({"reloaded":true}))
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
                )?))
            }
            "task.get" | "run.get" => Ok(crate::runtime::types::public_run(
                &self.supervisor.journal.get(required(&params, "id")?)?,
            )),
            "task.cancel" | "run.cancel" => Ok(crate::runtime::types::public_run(
                &self.supervisor.cancel(required(&params, "id")?)?,
            )),
            "run.input" => {
                self.supervisor.journal.input(
                    required(&params, "id")?,
                    "supplement",
                    required(&params, "content")?,
                )?;
                Ok(json!({"accepted":true}))
            }
            "run.resume" => Ok(crate::runtime::types::public_run(
                &self.supervisor.journal.reopen(required(&params, "id")?)?,
            )),
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
                // Wait on the journal instead of polling SQLite every 100 ms. The
                // permit stored by `notify_one` closes the gap between reading the
                // events and starting to wait, so no wake-up can be lost.
                let notifier = self.supervisor.journal.notifier();
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
                loop {
                    let events = self.supervisor.journal.events(conversation, after)?;
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if !events.is_empty() || remaining.is_zero() {
                        return Ok(json!({"events":events}));
                    }
                    let _ = crate::runtime::executor()
                        .block_on(tokio::time::timeout(remaining, notifier.notified()));
                }
            }
            "conversation.get" => {
                let id = required(&params, "id")?;
                Ok(json!({"id":id,"messages":self.supervisor.journal.conversation_messages(id)?}))
            }
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
            "workflow.extract" => {
                let run_id = required(&params, "runId")?;
                let run = self.supervisor.journal.get(run_id)?;
                if !matches!(
                    run.state,
                    crate::runtime::types::RunState::Succeeded
                        | crate::runtime::types::RunState::Answered
                ) {
                    return Err(Error::Conflict("只有已验证成功的运行可以提取流程".into()));
                }
                let plan = self
                    .supervisor
                    .journal
                    .plan(run_id)?
                    .ok_or_else(|| Error::Conflict("该运行没有结构化计划".into()))?;
                let mut steps = Vec::new();
                let outcomes = self.supervisor.journal.step_outcomes(run_id)?;
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
                    let execution = step
                        .execution
                        .ok_or_else(|| Error::Conflict("计划没有固定工具执行契约".into()))?;
                    steps.push(crate::runtime::workflow::WorkflowStep {
                        id: step.id,
                        title: step.title,
                        tool,
                        arguments: step.arguments,
                        depends_on: step.depends_on,
                        execution,
                        provider_version: step.provider_version,
                        resource_versions: step.resource_versions,
                    });
                }
                let workflow = crate::runtime::workflow::Workflow {
                    id: uuid::Uuid::new_v4().to_string(),
                    revision: 1,
                    name: params["name"].as_str().unwrap_or(&plan.goal).into(),
                    description: "由已验证运行提取".into(),
                    input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
                    steps,
                    verified_from_run: run_id.into(),
                    verified_at: crate::runtime::types::now(),
                    unattended: crate::tools::UnattendedPolicy::Forbidden,
                    created_at: crate::runtime::types::now(),
                };
                self.operations.store.save_workflow(&workflow)?;
                Ok(json!(workflow))
            }
            "workflow.run" => {
                let key = params["clientKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                Ok(crate::runtime::types::public_run(
                    &self.supervisor.submit_workflow(
                        required(&params, "id")?,
                        &key,
                        params["durationSec"].as_i64(),
                    )?,
                ))
            }
            "workflow.list" => Ok(json!(self.operations.store.workflows()?)),
            "operation.list" => Ok(json!(self.operations.store.list()?)),
            "operation.get" => Ok(json!(self.operations.store.get(required(&params, "id")?)?)),
            "operation.create" => {
                let plan: crate::runtime::kernel::MutationPlan =
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
                let preference: crate::runtime::kernel::PreferenceRecord =
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
                let grant: crate::runtime::permissions::TrustGrant =
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
            "model.use" => self.use_model(required(&params, "id")?),
            "model.save" => {
                AppConfig::save_model(&self.config_path, &params["model"])?;
                self.reload_runtime()?;
                Ok(json!({"saved":true}))
            }
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
            "bridge.state" => self.bridge.state(),
            _ => Err(Error::Config(format!("unknown IPC method: {method}"))),
        }
    }

    fn bootstrap(&self) -> Result<Value> {
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
            })
            .collect::<Vec<_>>();
        let skills = extensions
            .skills
            .list()
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
                    "instructions":skill.body,
                    "enabled":!config.agent.disabled_skills.contains(&skill.name)
                })
            })
            .collect::<Vec<_>>();
        let plugins = extensions
            .plugins
            .list()
            .iter()
            .map(|plugin| {
                json!({
                    "manifest":{"id":plugin.manifest.id,"name":plugin.manifest.name,"version":plugin.manifest.version,"description":plugin.manifest.description},
                    "status":&plugin.status,
                    "error":plugin.error.as_ref().map(|_|"插件加载失败，请检查配置"),
                    "configuredEnabled":config.plugins.enabled.contains(&plugin.manifest.id)
                })
            })
            .collect::<Vec<_>>();
        let bridge_status = if self.bridge.enabled() {
            match self.bridge.info() {
                Ok(_) => json!({"enabled":true,"connected":true,"baseUrl":self.bridge.base_url()}),
                Err(error) => {
                    json!({"enabled":true,"connected":false,"baseUrl":self.bridge.base_url(),"error":error.to_string()})
                }
            }
        } else {
            json!({"enabled":false,"connected":false,"baseUrl":self.bridge.base_url()})
        };
        Ok(
            json!({"models":models,"skills":skills,"plugins":plugins,"tools":extensions.tools.definitions(),"conversations":self.store.conversations()?,"tasks":self.supervisor.journal.list()?.iter().map(crate::runtime::types::public_run).collect::<Vec<_>>(),"strategies":self.supervisor.journal.strategies()?,"workflows":self.operations.store.workflows()?,"operations":self.operations.store.list()?,"resources":self.operations.store.resources()?,"diagnostics":self.operations.store.diagnostics()?,"notifications":self.operations.store.notifications(true)?,"bridge":bridge_status}),
        )
    }

    fn reload_extensions(&self) -> Result<()> {
        let config = AppConfig::load(&self.config_path)?;
        let mut skills = SkillRegistry::default();
        skills.load(
            &config
                .agent
                .skill_directories
                .iter()
                .map(|p| (p.clone(), "workspace".into()))
                .collect::<Vec<_>>(),
        )?;
        let mut tools = ToolRegistry::default();
        let mut plugins = PluginManager::default();
        plugins.load(
            &config.plugins.directories,
            &config.plugins.enabled,
            &mut tools,
            &mut skills,
        )?;
        let skills = Arc::new(skills);
        let plugins = Arc::new(plugins);
        if config.bridge.enabled {
            register_tools(&mut tools, self.bridge.clone())?;
        }
        register_builtin_tools(&mut tools, skills.clone(), plugins.clone())?;
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
    fn use_model(&self, model_id: &str) -> Result<Value> {
        AppConfig::set_active(&self.config_path, model_id)?;
        self.reload_runtime()?;
        Ok(json!({"activeModel":model_id}))
    }

    fn reload_runtime(&self) -> Result<()> {
        let config = AppConfig::load(&self.config_path)?;
        self.supervisor.update_config(config.clone())?;
        *self.config.lock().expect("config mutex poisoned") = config;
        Ok(())
    }
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
) -> Result<()> {
    let core_read = || ToolExecution {
        deferred: false,
        always_load: true,
        ..ToolExecution::read_only()
    };
    let skills_search = skills.clone();
    registry.register(
        FunctionTool::new("skills.search", "搜索已安装的 Skill。", json!({"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"}},"required":["query"],"additionalProperties":false}), "core:skills", move |arguments| serde_json::to_value(skills_search.search(arguments["query"].as_str().unwrap_or_default(), arguments["limit"].as_u64().unwrap_or(8) as usize)).map_err(Error::from))
            .with_execution(core_read()),
    )?;
    registry.register(
        FunctionTool::new("skills.read", "读取一个 Skill 的完整操作说明。", json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}), "core:skills", move |arguments| skills.get(arguments["name"].as_str().unwrap_or_default()).map(|skill| json!({"name":skill.name,"description":skill.description,"instructions":skill.body})).ok_or_else(|| Error::Tool("skill not found".into())))
            .with_execution(core_read()),
    )?;
    registry.register(
        FunctionTool::new(
            "plugins.list",
            "列出本地 Plugins 及启用状态。",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            "core:plugins",
            move |_| Ok(plugins.public_list()),
        )
        .with_execution(core_read()),
    )?;
    Ok(())
}
