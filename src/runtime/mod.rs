pub mod context;
pub mod gateway;
pub mod host;
pub mod operation;
pub mod policy;
pub mod store;
pub mod types;

use crate::{
    config::AppConfig,
    error::{Error, Result},
    extension::skills::SkillRegistry,
    extension::{ToolDefinition, ToolEffect, ToolExecution, ToolRegistry},
    model::{Message, Role, ToolCall},
};
use context::message;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::Duration,
};
use store::journal::Journal;
use tokio_util::sync::CancellationToken;
use types::*;

/// Streaming frames are coalesced before they reach SQLite; a long answer would
/// otherwise become one locked insert per token.
const DELTA_BATCH_CHARS: usize = 240;
const DELTA_BATCH_INTERVAL: Duration = Duration::from_millis(150);
/// 输出被截断后最多再续写几次。再多就以已有正文收成 Partial。
const MAX_TRUNCATION_RECOVERIES: usize = 3;

fn remember_discovered(run: &mut Run, exposed: &HashSet<String>) {
    let mut names: Vec<String> = exposed.iter().cloned().collect();
    names.sort();
    run.discovered = names;
}

fn context_overflow(error: &Error) -> bool {
    let text = match error {
        Error::Http(message) | Error::ModelProtocol(message) | Error::Conflict(message) => {
            message.to_lowercase()
        }
        _ => return false,
    };
    text.contains("prompt is too long")
        || text.contains("context length")
        || text.contains("context window")
        || text.contains("http 413")
        || (text.contains("token") && text.contains("exceed"))
}

/// 与领域无关的底座：语气、证据纪律、内部实现的边界。
///
/// 领域知识不写在这里。BetterGI 的行为规范与操作手册随 BGI 能力包分发，
/// 只有该提供方在线时才注入 —— 将来接入别的软件时，它们不该被 BGI 的规则污染。
const CORE_AGENT_POLICY: &str = r#"你是 Sleepy Doll，一个本地桌面助手。使用简体中文。

"Sleepy Doll" 是产品身份，不是角色扮演。不要自称别的角色，不编造身份设定，也不需要反复介绍自己；用直接、可靠、不过度热情的语气体现"少操心、直接办事"。

工作方式：
1. 用实际执行和读取到的结果回答，先给结果再给依据。只陈述有可靠依据的事实；无法观测、无法核实的事情直接说明不知道，不编造，也不用无关的工具去猜。
2. 先查完本机能够取得的信息，再判断是否真的缺少用户输入。只问无法自行取得、且不同答案会改变结果的信息，一次问完。
3. 需要启动程序、更新内容、修改配置或执行任务时，直接去做。要不要先征求同意由运行时的审批级别决定，不要在对话里替它先问一遍；被拦下时再说明它在等什么。
4. 一个数据源已经明确报出连接或鉴权错误时，不再调用依赖它的其他工具，直接报告这一个阻塞项。
5. 回复先给结果；只附必要证据、生效条件，或一个无法自行解决的阻塞项。不要给用户罗列选择题来代替继续工作，也不要把自己能做到的准备步骤交回给用户。

对用户说话：
- 进程、注入、反射、程序集、服务、方法、路由、端点、RPC、schema、序列化、HTTP 状态码都是内部实现，不是用户要看的内容；把它们翻译成用户的功能和结果。
- 只有用户明确要求开发排障时，才展开内部标识和原始错误摘要。"#;

fn configured_agent_instructions(prompt: &str) -> &str {
    // Two generated defaults shipped before the domain manual became an
    // always-loaded skill. Keeping either duplicates and contradicts the
    // current policy, while a genuinely user-authored prompt is preserved.
    if prompt.starts_with("你是 Sleepy Doll，一个操作 BetterGI 的桌面 Agent。")
        && (prompt.contains("# 接口分两层") || prompt.contains("# 用户配置在文件里"))
    {
        ""
    } else {
        prompt.trim()
    }
}

pub(crate) fn executor() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .enable_all()
            .build()
            .expect("runtime initialization")
    })
}

pub struct Supervisor {
    shutting_down: std::sync::atomic::AtomicBool,
    pub journal: Arc<Journal>,
    pub tasks: Arc<operation::task_store::TaskStore>,
    config: RwLock<AppConfig>,
    extensions: RwLock<RuntimeExtensions>,
    active: Mutex<HashMap<String, CancellationToken>>,
    model_gate: tokio::sync::RwLock<()>,
    catalog: host::catalog::Catalog,
    hooks: RwLock<Arc<host::hooks::HookBus>>,
    operations: Arc<operation::operations::OperationEngine>,
}
/// 技能可用性判定的输入集合。持有所有权，方便在运行循环外复用。
pub struct SkillEnvironment {
    plugins: HashSet<String>,
    capabilities: HashSet<String>,
    resource_kinds: HashSet<String>,
    providers: HashSet<String>,
}

impl SkillEnvironment {
    pub fn context(&self) -> crate::extension::skills::SkillContext<'_> {
        crate::extension::skills::SkillContext {
            plugins: &self.plugins,
            capabilities: &self.capabilities,
            resource_kinds: &self.resource_kinds,
            providers: &self.providers,
            platform: std::env::consts::OS,
        }
    }
}

struct RuntimeExtensions {
    skills: Arc<SkillRegistry>,
    tools: Arc<ToolRegistry>,
    adapters: Vec<Arc<host::adapter::AdapterClient>>,
}
impl Supervisor {
    async fn emit_lifecycle(
        &self,
        kind: host::hooks::HookEventKind,
        run_id: Option<&str>,
        operation_id: Option<&str>,
        data: Value,
        cancel: &CancellationToken,
    ) -> Result<host::hooks::HookOutcome> {
        let event = host::hooks::HookBus::event(kind, run_id, operation_id, data);
        let hook_bus = self.hooks.read().unwrap().clone();
        let mut outcome = hook_bus.emit(&event).await?;
        let event_name = serde_json::to_value(kind)?
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for adapter in self.adapters() {
            if !adapter.accepts_hook(&event_name) {
                continue;
            }
            let value = adapter.handle_hook(&event, cancel.clone()).await?;
            if value["blocked"] == true
                && matches!(
                    kind,
                    host::hooks::HookEventKind::BeforePlanCommit
                        | host::hooks::HookEventKind::BeforeToolUse
                        | host::hooks::HookEventKind::OperationPrepared
                )
            {
                return Err(Error::Conflict(
                    value["reason"]
                        .as_str()
                        .unwrap_or("操作被 Plugin Hook 阻止")
                        .into(),
                ));
            }
            outcome.context.extend(
                value["context"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|text| text.len() <= 4096)
                    .map(str::to_owned),
            );
        }
        Ok(outcome)
    }

    fn metric(&self, name: &str, value: f64, unit: &str, labels: Value) {
        let _ = self
            .operations
            .store
            .record_metric(&operation::kernel::MetricEvent {
                name: name.into(),
                value,
                unit: unit.into(),
                labels,
                recorded_at: types::now(),
            });
    }
    /// 技能可用性判定的输入。运行循环与界面读的是同一份，条件不会各算一套。
    pub fn skill_environment(&self, config: &AppConfig) -> SkillEnvironment {
        let mut capabilities = self
            .tools()
            .definitions()
            .into_iter()
            .map(|tool| tool.name)
            .collect::<HashSet<_>>();
        capabilities.extend(self.catalog.capabilities.keys().cloned());
        let resource_kinds = self
            .operations
            .store
            .resources()
            .unwrap_or_default()
            .into_iter()
            .map(|resource| resource.kind)
            .collect::<HashSet<_>>();
        let plugins = config
            .plugins
            .enabled
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        // 领域说明跟着提供方走：桥关掉，BGI 的行为规范与操作手册也不再出现在
        // 提示词里，通用对话不会被它带偏。
        let mut providers = plugins.clone();
        if config.bridge.enabled {
            providers.insert("bgi".into());
        }
        SkillEnvironment {
            plugins,
            capabilities,
            resource_kinds,
            providers,
        }
    }

    fn skills(&self) -> Arc<SkillRegistry> {
        self.extensions.read().unwrap().skills.clone()
    }
    fn tools(&self) -> Arc<ToolRegistry> {
        self.extensions.read().unwrap().tools.clone()
    }
    fn adapters(&self) -> Vec<Arc<host::adapter::AdapterClient>> {
        self.extensions.read().unwrap().adapters.clone()
    }
    pub fn update_extensions(
        &self,
        skills: Arc<SkillRegistry>,
        tools: Arc<ToolRegistry>,
        adapters: Vec<Arc<host::adapter::AdapterClient>>,
    ) {
        *self.extensions.write().unwrap() = RuntimeExtensions {
            skills,
            tools,
            adapters,
        };
    }

    pub fn new(
        config: AppConfig,
        skills: Arc<SkillRegistry>,
        tools: Arc<ToolRegistry>,
        operations: Arc<operation::operations::OperationEngine>,
        adapters: Vec<Arc<host::adapter::AdapterClient>>,
    ) -> Result<Arc<Self>> {
        let journal = Arc::new(Journal::open(&config.storage.database)?);
        let tasks = Arc::new(operation::task_store::TaskStore::open(
            &config.storage.database,
        )?);
        let catalog = host::catalog::Catalog::load(&config.runtime.catalog_directory)?;
        let hook_bus = Arc::new(host::hooks::HookBus::new(config.hooks.clone())?);
        let supervisor = Arc::new(Self {
            shutting_down: std::sync::atomic::AtomicBool::new(false),
            journal,
            tasks,
            config: RwLock::new(config),
            extensions: RwLock::new(RuntimeExtensions {
                skills,
                tools,
                adapters,
            }),
            active: Mutex::new(HashMap::new()),
            model_gate: tokio::sync::RwLock::new(()),
            catalog,
            hooks: RwLock::new(hook_bus),
            operations,
        });
        // Reconcile before accepting new game writes. Persistent leases survive crashes.
        let weak = Arc::downgrade(&supervisor);
        let notifier = supervisor.journal.notifier();
        executor().spawn(async move {
            if let Some(s) = weak.upgrade() {
                let _ = s.operations.recover();
                let _ = s.recover().await;
            }
            loop {
                let Some(s) = weak.upgrade() else { break };
                let _ = s.schedule();
                drop(s);
                // Wake on new work instead of polling SQLite ten times a second.
                // The timeout only covers a notification that cannot be delivered.
                tokio::select! {
                    _ = notifier.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
            }
        });
        Ok(supervisor)
    }
    pub fn submit(
        &self,
        prompt: &str,
        conversation: Option<&str>,
        key: &str,
        duration: Option<i64>,
        model: Option<&str>,
    ) -> Result<Run> {
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Config("应用正在退出，请重新打开后发送".into()));
        }
        if prompt.trim().is_empty() {
            return Err(Error::Config("消息为空".into()));
        }
        if prompt.len() > 128 * 1024 {
            return Err(Error::Config("消息过长（上限 128 KiB）".into()));
        }
        // Reject content that cannot fit the context budget here, where it reads as
        // an input problem, instead of failing mid-run with a budget error after the
        // user has already waited for a model turn.
        let config = self.config.read().unwrap();
        let active = config.active()?;
        let (context_chars, _) = policy::budget(&config.runtime, active);
        let prompt_chars = prompt.chars().count();
        let reserved = config.agent.system_prompt.chars().count() + 8192;
        drop(config);
        if prompt_chars + reserved > context_chars {
            return Err(Error::Config(format!(
                "消息过长：本次 {prompt_chars} 字符，加上系统提示超过上下文预算 {context_chars} 字符"
            )));
        }
        let conversation = conversation
            .map(str::to_owned)
            .unwrap_or_else(|| format!("conversation-{key}"));
        let duration = duration.unwrap_or(self.config.read().unwrap().runtime.duration_sec);
        if !(1..=86400).contains(&duration) {
            return Err(Error::Config("任务时限必须在 1 秒到 24 小时之间".into()));
        }
        // 模型选择按优先级取：用户在界面上刚挑的那一个 → 会话自己存的 → 默认模型。
        // 新对话还没有会话记录，第一条消息就是把它定下来的时机。
        let requested = self.validated_model(model)?;
        let stored = self
            .journal
            .conversation(&conversation)
            .ok()
            .and_then(|summary| summary.model_id)
            .and_then(|id| self.validated_model(Some(&id)).ok().flatten());
        let resolved = requested
            .clone()
            .or(stored)
            .or_else(|| {
                self.config
                    .read()
                    .unwrap()
                    .active()
                    .ok()
                    .map(|model| model.id.clone())
            })
            .ok_or_else(|| Error::Config("还没有配置模型。请先在设置里添加。".into()))?;
        let run = self
            .journal
            .create(prompt, &conversation, key, duration, Some(&resolved))?;
        // 每个对话都要有绑定的模型。删掉的配置在上面已经回落到默认。
        self.journal
            .set_conversation_model(&conversation, Some(&resolved))?;
        Ok(run)
    }

    /// 请求里的模型必须真实存在，否则宁可报错也不静默换成别的模型。
    fn validated_model(&self, model: Option<&str>) -> Result<Option<String>> {
        let Some(id) = model.filter(|id| !id.is_empty()) else {
            return Ok(None);
        };
        if !self
            .config
            .read()
            .unwrap()
            .models
            .iter()
            .any(|entry| entry.id == id)
        {
            return Err(Error::Config("选择的模型配置不存在".into()));
        }
        Ok(Some(id.to_owned()))
    }
    pub fn submit_strategy(&self, id: &str, key: &str, duration: Option<i64>) -> Result<Run> {
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Config("应用正在退出，请重新打开后运行".into()));
        }
        let duration = duration.unwrap_or(self.config.read().unwrap().runtime.duration_sec);
        if !(1..=86400).contains(&duration) {
            return Err(Error::Config("任务时限必须在 1 秒到 24 小时之间".into()));
        }
        let mut strategy = self.journal.strategy(id)?;
        let run = self.journal.create_manual_run(&strategy, key, duration)?;
        self.journal.mark_strategy_run(&mut strategy, &run.id)?;
        Ok(run)
    }
    /// 运行快捷任务。只接纳已发布修订；期望版本对不上时先让卡片刷新，
    /// 不用旧文案暗跑新目标。
    pub fn submit_workflow(
        &self,
        id: &str,
        expected: Option<u64>,
        key: &str,
        duration: Option<i64>,
    ) -> Result<Run> {
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Config("应用正在退出，请重新打开后运行".into()));
        }
        let duration = duration.unwrap_or(self.config.read().unwrap().runtime.duration_sec);
        let definition = self.tasks.definition(id)?;
        if definition.deleted_at.is_some() {
            return Err(Error::Conflict("这个快捷任务已被删除".into()));
        }
        if definition.archived_at.is_some() {
            return Err(Error::Conflict("这个快捷任务已归档；恢复后才能运行".into()));
        }
        let Some(revision) = self.tasks.published_revision(id)? else {
            return Err(Error::Conflict("这个快捷任务还不能运行".into()));
        };
        // 同一任务默认不允许重入：两处同时点击只接纳一个活动运行。
        if let Some(active) = self.active_run_for(id)? {
            return Ok(active);
        }
        if let Some(expected) = expected
            && expected != revision.revision
        {
            return Err(Error::Conflict(format!(
                "任务已更新到第 {} 版，请刷新后重试",
                revision.revision
            )));
        }
        let duration = duration.min(revision.limits.max_seconds);
        let run = self.journal.create_workflow_run(
            id,
            &definition.name,
            revision.revision,
            key,
            duration,
        )?;
        self.tasks.record_run(id, revision.revision, &run.id)?;
        Ok(run)
    }

    fn active_run_for(&self, task_id: &str) -> Result<Option<Run>> {
        for run in self.journal.pending()? {
            if let RunSource::SavedWorkflow { workflow_id, .. } = &run.source
                && workflow_id == task_id
                && run.state.holds_lease()
            {
                return Ok(Some(run));
            }
        }
        Ok(None)
    }

    /// 会话列表。默认不含已归档，也不把全部历史一次交给前端。
    pub fn conversations(
        &self,
        query: &store::journal::ConversationQuery,
    ) -> Result<Vec<store::journal::ConversationSummary>> {
        self.journal.conversations_matching(query)
    }

    /// 任务定义摘要。依赖可用性由运行时判定，Core 不认识具体领域工具。
    pub fn task_summaries(
        &self,
        conversation: Option<&str>,
    ) -> Result<Vec<operation::task_store::TaskSummary>> {
        let conversations = self
            .journal
            .conversations()?
            .into_iter()
            .map(|conversation| conversation.id)
            .collect::<HashSet<_>>();
        let definitions = self.tasks.definitions()?;
        let available = self.tool_names();
        let is_available = |name: &str| available.contains(name);
        let mut summaries = definitions
            .iter()
            .filter(|definition| {
                conversation
                    .is_none_or(|id| definition.source_conversation_id.as_deref() == Some(id))
            })
            .map(|definition| {
                self.tasks
                    .summary(definition, &is_available, &conversations)
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| right.updated_at.cmp(&left.updated_at))
        });
        Ok(summaries)
    }

    /// 某任务仍占用互斥锁的运行，用于删除定义时如实提示活动运行。
    pub fn task_run_ids(&self, task_id: &str) -> Result<Vec<String>> {
        Ok(self
            .journal
            .pending()?
            .into_iter()
            .filter(|run| {
                matches!(&run.source, RunSource::SavedWorkflow { workflow_id, .. } if workflow_id == task_id)
                    && run.state.holds_lease()
            })
            .map(|run| run.id)
            .collect())
    }

    /// 运行接纳时固定的模型配置。配置被删掉时显式失败，不静默换成别的协议。
    fn model_for(&self, run: &Run) -> Result<crate::config::ModelConfig> {
        let config = self.config.read().unwrap();
        match run.model_id.as_deref() {
            None => config.active().cloned(),
            Some(id) => config
                .models
                .iter()
                .find(|model| model.id == id)
                .cloned()
                .ok_or_else(|| {
                    Error::Config("本轮绑定的模型配置已删除，请重新选择后再发送".into())
                }),
        }
    }

    fn tool_names(&self) -> HashSet<String> {
        self.definitions(&HashSet::new())
            .into_iter()
            .map(|definition| definition.name)
            .collect()
    }

    /// 编译快捷任务所需的工具契约快照。
    pub fn tool_catalog(&self) -> operation::task::ToolCatalog {
        let mut catalog = operation::task::ToolCatalog::new();
        for definition in self.definitions(&HashSet::new()) {
            catalog.insert(
                &definition.name,
                operation::task::ToolContract {
                    execution: definition.execution.clone(),
                    provider_version: definition.provider_version.clone(),
                    input_schema: definition.input_schema.clone(),
                },
            );
        }
        catalog
    }

    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.definitions(&HashSet::new())
    }
    fn schedule(self: &Arc<Self>) -> Result<()> {
        if self.shutting_down.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        let runs = self.journal.pending()?;
        let mut occupied = HashSet::new();
        let mut active = self.active.lock().unwrap();
        for run in &runs {
            if active.contains_key(&run.id) {
                occupied.insert(run.conversation_id.clone());
            }
        }
        for run in runs {
            if !matches!(run.state, RunState::Queued | RunState::Recovering)
                || active.contains_key(&run.id)
                || occupied.contains(&run.conversation_id)
                || active.len() >= 4
            {
                continue;
            }
            let cancel = CancellationToken::new();
            active.insert(run.id.clone(), cancel.clone());
            occupied.insert(run.conversation_id.clone());
            let s = self.clone();
            executor().spawn(async move {
                let mut run = run;
                let result = s.session(&mut run, &cancel).await;
                if let Err(e) = result
                    && !run.state.terminal()
                {
                    if let Error::Storage(storage) = &e {
                        eprintln!("Run {} storage failure: {storage}", run.id);
                    }
                    let unknown = s
                        .journal
                        .attempts(&run.id)
                        .map(|a| {
                            a.iter().any(|a| {
                                matches!(a.outcome.as_str(), "unknown" | "running" | "submitting")
                            })
                        })
                        .unwrap_or(true);
                    let next = if unknown {
                        RunState::NeedsReview
                    } else if matches!(e, Error::Cancelled) {
                        RunState::Cancelled
                    } else {
                        RunState::Failed
                    };
                    run.error = Some(e.user_message());
                    if let Err(storage) = s.journal.save(&mut run, next) {
                        eprintln!("Unable to persist failed run: {storage}");
                    }
                }
                let kind = if run.state == RunState::NeedsReview {
                    host::hooks::HookEventKind::RunNeedsReview
                } else {
                    host::hooks::HookEventKind::RunCompleted
                };
                let _ = s
                    .emit_lifecycle(
                        kind,
                        Some(&run.id),
                        None,
                        json!({"state":run.state,"error":run.error}),
                        &cancel,
                    )
                    .await;
                if matches!(
                    run.state,
                    RunState::Succeeded | RunState::Failed | RunState::NeedsReview
                ) {
                    let (notification_kind, title) = match run.state {
                        RunState::Succeeded => ("runCompleted", "运行已完成"),
                        RunState::NeedsReview => ("runNeedsReview", "运行需要核对"),
                        _ => ("runFailed", "运行未完成"),
                    };
                    let _ = s.operations.store.notify(
                        notification_kind,
                        title,
                        run.result
                            .as_deref()
                            .or(run.error.as_deref())
                            .unwrap_or("请查看运行记录"),
                        Some(json!({"runId":run.id,"conversationId":run.conversation_id})),
                    );
                }
                s.active.lock().unwrap().remove(&run.id);
            });
        }
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> Result<Run> {
        let mut run = self.journal.get(id)?;
        if let Some(token) = self.active.lock().unwrap().get(id) {
            token.cancel();
            self.journal.emit(&run, "cancel.requested", json!({}))?;
        } else if run.state == RunState::Queued {
            self.journal.save(&mut run, RunState::Cancelled)?;
        }
        Ok(run)
    }
    pub fn shutdown(&self) {
        self.shutting_down
            .store(true, std::sync::atomic::Ordering::SeqCst);
        for token in self.active.lock().unwrap().values() {
            token.cancel();
        }
        if let Ok(runs) = self.journal.pending() {
            for run in runs {
                if run.state == RunState::Queued {
                    let _ = self.cancel(&run.id);
                }
            }
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !self.active.lock().unwrap().is_empty() {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    pub fn update_config(&self, config: AppConfig) -> Result<()> {
        config.validate()?;
        let hooks = Arc::new(host::hooks::HookBus::new(config.hooks.clone())?);
        // All model calls hold a read guard, so no old model can start after this returns.
        executor().block_on(async {
            let _guard = self.model_gate.write().await;
            *self.config.write().unwrap() = config;
            *self.hooks.write().unwrap() = hooks;
        });
        Ok(())
    }
    async fn recover(&self) -> Result<()> {
        for mut run in self.journal.pending()? {
            if run.state.terminal() || run.state == RunState::Queued {
                continue;
            }
            if matches!(
                run.state,
                RunState::AwaitingApproval | RunState::AwaitingUser | RunState::Cancelling
            ) {
                run.error = Some("程序中断，等待信息或停止状态需要重新核对".into());
                self.journal.save(&mut run, RunState::NeedsReview)?;
            } else {
                self.journal.save(&mut run, RunState::Recovering)?;
            }
        }
        Ok(())
    }
    async fn reconcile(
        &self,
        run: &mut Run,
        bridge: &host::bridge::Bridge,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let attempts = self.journal.attempts(&run.id)?;
        for mut a in attempts {
            if matches!(a.outcome.as_str(), "running" | "submitting" | "unknown") {
                let info = bridge.get("/bridge/v1/info", cancel).await?;
                if info["instanceId"] != a.instance_id || a.job_id.is_none() {
                    return Err(Error::Conflict(
                        "无法确定旧调用结果；保留执行锁并等待核对".into(),
                    ));
                }
                self.journal.save(run, RunState::WaitingJob)?;
                bridge.wait(&self.journal, run, &mut a, cancel).await?;
            } else if a.outcome == "prepared" {
                a.outcome = "notSubmitted".into();
                self.journal.attempt(&a)?;
                self.journal.release(&a)?;
            }
        }
        let history = self.journal.history(run)?;
        let attempts = self.journal.attempts(&run.id)?;
        for call in history.iter().flat_map(|m| m.tool_calls.iter()) {
            if history
                .iter()
                .any(|m| m.tool_call_id.as_ref() == Some(&call.id))
            {
                continue;
            }
            let result=attempts.iter().find(|a|a.call_id==call.id).map(|a|json!({"outcome":a.outcome,"evidence":a.evidence})).unwrap_or_else(||json!({"ok":false,"error":"运行中断，此调用没有持久化的执行结果；不要假定已执行"}));
            self.journal.append_message(
                run,
                &Message {
                    role: Role::Tool,
                    content: result.to_string(),
                    tool_call_id: Some(call.id.clone()),
                    tool_calls: vec![],
                    reasoning: None,
                },
            )?;
        }
        if attempts.iter().any(|a| a.outcome == "unknown") {
            return Err(Error::Conflict("执行结果仍然未知".into()));
        }
        Ok(())
    }
    async fn session(&self, run: &mut Run, cancel: &CancellationToken) -> Result<()> {
        let initial = self.config.read().unwrap().clone();
        let policy = initial.runtime.clone();
        let bridge = host::bridge::Bridge::new(initial.bridge.clone())?;
        self.emit_lifecycle(
            host::hooks::HookEventKind::RunStarted,
            Some(&run.id),
            None,
            json!({"source":run.source}),
            cancel,
        )
        .await?;
        if run.state == RunState::Recovering {
            self.reconcile(run, &bridge, cancel).await?;
        }
        if matches!(run.source, RunSource::SavedStrategy { .. }) {
            return self.run_saved_strategy(run, &bridge, &policy, cancel).await;
        }
        if let RunSource::SavedWorkflow {
            ref workflow_id,
            workflow_revision,
        } = run.source
        {
            let workflow_id = workflow_id.clone();
            return self
                .run_saved_workflow(
                    run,
                    &workflow_id,
                    workflow_revision,
                    &bridge,
                    &policy,
                    cancel,
                )
                .await;
        }
        self.journal.save(run, RunState::Deciding)?;
        let mut exposed: HashSet<String> = run.discovered.iter().cloned().collect();
        let mut history = self.journal.history(run)?;
        let skill_snapshot = self.skills();
        let mut truncation_recoveries = 0usize;
        let mut budget_scale = 100usize;
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if unix_now() > run.deadline
                || run.decisions >= policy.max_decisions
                || run.tool_calls >= policy.max_tools
            {
                // 轮次、工具次数与时限约束一次运行的总量；token 只按当轮上下文
                // 判断（见下面的 estimated），不累加历轮 —— 累加会让多轮任务在
                // 上下文还很空的时候就被判定超支。
                run.error = Some("已达到本次运行预算".into());
                self.journal.save(run, RunState::NeedsReview)?;
                return Ok(());
            }
            for input in self.journal.drain_inputs(&run.id)? {
                let m = message(Role::User, input);
                history.push(m);
            }
            let current = self.config.read().unwrap().clone();
            let skill_environment = self.skill_environment(&current);
            let skill_context = skill_environment.context();
            let available = skill_snapshot
                .list()
                .into_iter()
                .filter(|s| {
                    !current.agent.disabled_skills.contains(&s.name)
                        && skill_snapshot.eligible(s, &skill_context)
                })
                .collect::<Vec<_>>();
            let skill_catalog = available
                .iter()
                .map(|s| format!("{}: {}", s.name, s.description))
                .collect::<Vec<_>>()
                .join("\n");
            let matched = if current.agent.auto_load_skills {
                skill_snapshot
                    .search(
                        &context::skill_query(&run.prompt, &history),
                        current.agent.max_loaded_skills,
                    )
                    .into_iter()
                    .map(|s| s.name)
                    .collect::<HashSet<_>>()
            } else {
                HashSet::new()
            };
            let explicit_skills = available
                .iter()
                .filter(|s| {
                    s.always_load
                        || run.prompt.contains(&format!("${}", s.name))
                        || matched.contains(&s.name)
                })
                .collect::<Vec<_>>();
            let mut attachments = host::attachments::AttachmentSet::default();
            if let Ok(checkpoint) = self.journal.checkpoint(&run.id) {
                attachments.insert(host::attachments::Attachment {
                    id: format!("checkpoint:{}", checkpoint.run_revision),
                    kind: host::attachments::AttachmentKind::Checkpoint,
                    content: serde_json::to_string(&checkpoint)?,
                    priority: 100,
                });
            }
            attachments.insert(host::attachments::Attachment {
                id: "skill-catalog".into(),
                kind: host::attachments::AttachmentKind::SkillCatalog,
                content: skill_catalog,
                priority: 40,
            });
            for skill in explicit_skills {
                attachments.insert(host::attachments::Attachment {
                    id: format!("skill:{}", skill.name),
                    kind: host::attachments::AttachmentKind::SkillInstructions,
                    content: skill.body.clone(),
                    priority: 60,
                });
            }
            if let Ok(preferences) = self.operations.store.preferences("global") {
                for preference in preferences.into_iter().take(16) {
                    attachments.insert(host::attachments::Attachment {
                        id: format!("preference:{}", preference.key),
                        kind: host::attachments::AttachmentKind::Preference,
                        content: serde_json::to_string(&preference)?,
                        priority: 30,
                    });
                }
            }
            // 预算按当前模型自己的窗口推导，所以先取模型。运行接纳时已经固定
            // 配置；运行途中换选择只影响之后开始的运行。
            let model = self.model_for(run)?;
            let output_reserve = model.options.max_output_tokens.unwrap_or(8192);
            let (base_chars, token_budget) = policy::budget(&policy, &model);
            let char_budget = (base_chars.saturating_mul(budget_scale) / 100).max(4096);
            let attachment_text = attachments.render(char_budget / 3);
            let configured = configured_agent_instructions(&current.agent.system_prompt);
            let system = format!(
                "{CORE_AGENT_POLICY}\n\n用户自定义指令：\n{}\n\n当前目标：\n{}\n\n{}",
                if configured.is_empty() {
                    "无"
                } else {
                    configured
                },
                run.prompt,
                attachment_text,
            );
            let plan = self.journal.plan(&run.id)?;
            let definitions = self.definitions(&exposed);
            let definitions_json = serde_json::to_string(&definitions)?;
            // `context::build` 的预算以字符计，工具契约也按字符扣减。
            let reserve = definitions_json.chars().count();
            let packed =
                context::build(system, history.clone(), char_budget.saturating_sub(reserve))?;
            let estimated = packed.tokens + context::estimate_tokens(&definitions_json) + 1024;
            run.context_tokens = estimated;
            run.context_window = token_budget;
            let compacted = packed.compacted();
            if compacted {
                self.journal.emit(
                    run,
                    "context.compacted",
                    json!({
                        "clearedResults": packed.cleared_results,
                        "droppedGroups": packed.dropped_groups,
                        "tokens": packed.tokens,
                        "window": token_budget,
                    }),
                )?;
            }
            run.context_compacted = run.context_compacted || compacted;
            let messages = packed.messages;
            let guard = self.model_gate.read().await;
            let model_config = self.config.read().unwrap().clone();
            // 只看当前这一轮的上下文占用。把历轮输入累加起来比，算的是累计
            // 花销而不是上下文大小 —— 多轮任务跑到一半就会以「预算不足」收场，
            // 尽管当轮上下文离上限还很远。历轮用量只作记录，不设闸门；轮次与
            // 工具次数分别由 max_decisions 和 max_tools 约束。
            if estimated + output_reserve > token_budget {
                return Err(Error::Conflict(format!(
                    "当前上下文约 {estimated} token，加上输出预留 {output_reserve} 超过上限 {token_budget}"
                )));
            }
            run.decisions += 1;
            let previously_estimated = run.usage_estimated;
            run.input_tokens += estimated;
            run.output_tokens += output_reserve;
            run.usage_estimated = true;
            self.journal.save(run, RunState::Deciding)?;
            let protocol = model.protocol;
            let mut candidates = vec![model];
            candidates.extend(model_config.agent.fallback_models.iter().filter_map(|id| {
                model_config
                    .models
                    .iter()
                    .find(|model| &model.id == id)
                    .cloned()
            }));
            let mut response = None;
            let mut last_error = None;
            for (index, candidate) in candidates.into_iter().enumerate() {
                let mut emitted = false;
                // One row per stream frame would make every token a locked SQLite
                // insert, so frames are coalesced into short batches. A plain
                // Mutex keeps the surrounding future Send for executor spawn.
                let batch = std::sync::Mutex::new((String::new(), std::time::Instant::now()));
                let model_started = std::time::Instant::now();
                let result = tokio::time::timeout(
                    Duration::from_secs((run.deadline - unix_now()).max(1) as u64),
                    gateway::complete(candidate.clone(), &messages, &definitions, cancel, |text| {
                        if text.is_empty() {
                            return Ok(());
                        }
                        emitted = true;
                        let text = {
                            let mut guard = batch.lock().unwrap();
                            guard.0.push_str(text);
                            if guard.0.chars().count() < DELTA_BATCH_CHARS
                                && guard.1.elapsed() < DELTA_BATCH_INTERVAL
                            {
                                return Ok(());
                            }
                            guard.1 = std::time::Instant::now();
                            std::mem::take(&mut guard.0)
                        };
                        self.journal.emit(
                            run,
                            "assistant.delta",
                            json!({"text":text,"turn":run.decisions,"model":candidate.id}),
                        )
                    }),
                )
                .await
                .map_err(|_| Error::Conflict("模型等待超过任务时限".into()))
                .and_then(|result| result);
                // Surface what the model already produced even when the turn
                // failed or was cancelled mid-stream.
                let tail = std::mem::take(&mut batch.lock().unwrap().0);
                if !tail.is_empty() {
                    self.journal.emit(
                        run,
                        "assistant.delta",
                        json!({"text":tail,"turn":run.decisions,"model":candidate.id}),
                    )?;
                }
                self.metric(
                    "model.duration",
                    model_started.elapsed().as_secs_f64() * 1000.0,
                    "ms",
                    json!({"modelId":candidate.id,"success":result.is_ok()}),
                );
                match result {
                    Ok(value) => {
                        response = Some(value);
                        break;
                    }
                    Err(error) if !emitted && context_overflow(&error) && budget_scale > 66 => {
                        last_error = Some(error);
                        break;
                    }
                    Err(error)
                        if !emitted
                            && index + 1 < model_config.agent.fallback_models.len() + 1
                            && matches!(error, Error::Http(_) | Error::Timeout(_)) =>
                    {
                        self.journal.emit(
                            run,
                            "model.fallback",
                            json!({"from":candidate.id,"reason":"providerUnavailable"}),
                        )?;
                        last_error = Some(error);
                    }
                    Err(error) => return Err(error),
                }
            }
            if response.is_none()
                && last_error.as_ref().is_some_and(context_overflow)
                && budget_scale > 66
            {
                budget_scale = 66;
                self.journal
                    .emit(run, "context.overflow", json!({"scale": budget_scale}))?;
                drop(guard);
                continue;
            }
            let response = response.ok_or_else(|| {
                last_error.unwrap_or_else(|| Error::Http("all configured models failed".into()))
            })?;
            drop(guard);
            run.input_tokens =
                run.input_tokens - estimated + response.usage.input_tokens.unwrap_or(estimated);
            run.output_tokens = run.output_tokens - output_reserve
                + response.usage.output_tokens.unwrap_or(
                    response.text.len() as u64
                        + response
                            .tool_calls
                            .iter()
                            .map(|c| c.arguments.to_string().len() as u64 + 64)
                            .sum::<u64>(),
                );
            run.cache_read_tokens = response.usage.cache_read_tokens.unwrap_or(0);
            if let Some(actual) = response.usage.context_tokens(protocol) {
                run.context_tokens = actual;
            }
            run.usage_estimated = previously_estimated
                || response.usage.input_tokens.is_none()
                || response.usage.output_tokens.is_none();
            // 一轮里要的调用数超上限时，执行允许的部分，其余作为错误结果回给
            // 模型让它分批重试。直接中止整轮等于让它白等一次模型调用 —— 一次
            // 想读十个文件是很自然的要求，不该是致命错误。整轮预算用尽才是。
            let per_turn = current.agent.max_tool_calls_per_turn;
            let remaining = policy.max_tools.saturating_sub(run.tool_calls);
            if remaining == 0 {
                return Err(Error::Conflict("工具调用超过预算".into()));
            }
            let allowed = per_turn.min(remaining);
            let mut calls = response.tool_calls;
            let mut deferred = if calls.len() > allowed {
                calls.split_off(allowed)
            } else {
                Vec::new()
            };
            // Provider IDs may repeat between turns. The internal identifier never does.
            for call in calls.iter_mut().chain(deferred.iter_mut()) {
                call.id = format!("call_{}", uuid::Uuid::new_v4());
            }
            // 未执行的调用同样要进 assistant 轮，并各自拿到一条错误结果：提供方
            // 要求每个 tool_use 都有配对的 tool_result，模型也要知道自己少做了
            // 哪几件。
            let mut tool_calls = calls.clone();
            tool_calls.extend(deferred.iter().cloned());
            let assistant = Message {
                role: Role::Assistant,
                content: response.text.clone(),
                tool_call_id: None,
                tool_calls,
                // 逐字随消息持久化：下一轮必须原样回传，否则提供方拒绝请求。
                reasoning: response.reasoning,
            };
            self.journal.append_message(run, &assistant)?;
            history.push(assistant);
            self.journal.emit(
                run,
                "assistant.completed",
                json!({"text":response.text,"turn":run.decisions}),
            )?;
            if gateway::output_truncated(response.finish_reason.as_deref())
                && calls.is_empty()
                && deferred.is_empty()
            {
                if truncation_recoveries >= MAX_TRUNCATION_RECOVERIES {
                    run.result = Some(response.text.clone());
                    run.error = Some("回复在输出长度上限处被截断".into());
                    self.journal.finish(run, RunState::Partial)?;
                    return Ok(());
                }
                truncation_recoveries += 1;
                let nudge = message(
                    Role::User,
                    "上一轮回复在输出长度上限处被截断。从截断处继续写完，不要重复已经给出的内容。",
                );
                self.journal.append_message(run, &nudge)?;
                history.push(nudge);
                self.journal.emit(
                    run,
                    "output.truncated",
                    json!({"turn":run.decisions,"recoveries":truncation_recoveries}),
                )?;
                continue;
            }
            if calls.is_empty() && deferred.is_empty() {
                let attempts = self.journal.attempts(&run.id)?;
                let next = if attempts.iter().any(|a| {
                    matches!(
                        a.outcome.as_str(),
                        "unknown" | "running" | "submitting" | "prepared" | "completed"
                    )
                }) {
                    RunState::NeedsReview
                } else if attempts.iter().any(|a| {
                    matches!(
                        a.outcome.as_str(),
                        "failed" | "verifiedFailed" | "notSubmitted"
                    )
                }) {
                    if attempts.iter().any(|a| a.outcome == "verifiedSucceeded") {
                        RunState::Partial
                    } else {
                        RunState::Failed
                    }
                } else if plan.as_ref().is_some_and(|p| {
                    p.steps
                        .iter()
                        .filter(|step| {
                            step.capability_id.is_some()
                                || step.execution.as_ref().is_some_and(|execution| {
                                    execution.effect != ToolEffect::ReadOnly
                                })
                        })
                        .any(|s| {
                            !attempts.iter().any(|a| {
                                a.request["stepId"] == s.id && a.outcome == "verifiedSucceeded"
                            })
                        })
                }) {
                    RunState::NeedsReview
                } else if attempts.is_empty() {
                    RunState::Answered
                } else {
                    RunState::Succeeded
                };
                run.result = Some(response.text);
                if !self.journal.finish(run, next)? {
                    run.result = None;
                    continue;
                }
                return Ok(());
            }
            if calls.len() > 1
                && calls.iter().all(|call| {
                    self.definition(&call.name, &exposed)
                        .is_some_and(|definition| definition.execution.can_run_concurrently())
                })
            {
                for batch in calls.chunks(4) {
                    run.tool_calls += batch.len();
                    self.journal.save(run, RunState::Executing)?;
                    for call in batch {
                        self.journal.emit(run, "tool.started", json!(call))?;
                    }
                    let results = futures_util::future::join_all(batch.iter().map(|call| {
                        let mut local_run = run.clone();
                        let mut local_exposed = exposed.clone();
                        let bridge = &bridge;
                        let policy = &policy;
                        let result_limit = self
                            .definition(&call.name, &exposed)
                            .map(|definition| definition.execution.max_result_chars)
                            .unwrap_or(12_000);
                        async move {
                            let result = self
                                .tool(
                                    &mut local_run,
                                    call,
                                    bridge,
                                    policy,
                                    cancel,
                                    &mut local_exposed,
                                )
                                .await;
                            (call, result, local_exposed, result_limit)
                        }
                    }))
                    .await;
                    for (call, result, discovered, result_limit) in results {
                        exposed.extend(discovered);
                        self.record_result(run, call, result, result_limit, &mut history)?;
                    }
                }
                remember_discovered(run, &exposed);
                self.journal.save(run, RunState::Deciding)?;
                continue;
            }
            for call in calls {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                run.tool_calls += 1;
                self.journal.save(run, RunState::Executing)?;
                self.journal.emit(run, "tool.started", json!(call))?;
                let result_limit = self
                    .definition(&call.name, &exposed)
                    .map(|definition| definition.execution.max_result_chars)
                    .unwrap_or(12_000);
                let result = self
                    .tool(run, &call, &bridge, &policy, cancel, &mut exposed)
                    .await;
                self.record_result(run, &call, result, result_limit, &mut history)?;
                remember_discovered(run, &exposed);
                if matches!(run.state, RunState::AwaitingUser | RunState::Verifying) {
                    self.journal.save(run, RunState::Deciding)?;
                }
            }
            for call in &deferred {
                let result = Err(Error::Tool(format!(
                    "本轮工具调用已达上限（{allowed} 次），本次未执行；请分批重试"
                )));
                let result_limit = self
                    .definition(&call.name, &exposed)
                    .map(|definition| definition.execution.max_result_chars)
                    .unwrap_or(12_000);
                self.record_result(run, call, result, result_limit, &mut history)?;
            }
            remember_discovered(run, &exposed);
            self.journal.save(run, RunState::Deciding)?;
        }
    }
    async fn run_saved_strategy(
        &self,
        run: &mut Run,
        bridge: &host::bridge::Bridge,
        policy: &policy::RuntimeConfig,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let plan = self
            .journal
            .plan(&run.id)?
            .ok_or_else(|| Error::Conflict("保存的策略缺少执行计划".into()))?;
        self.journal.save(run, RunState::Deciding)?;
        let mut seen = HashSet::new();
        if plan.steps.is_empty() || plan.steps.len() > policy.max_tools {
            return Err(Error::Conflict("保存的策略步骤数量无效".into()));
        }
        for step in &plan.steps {
            if !step.depends_on.iter().all(|id| seen.contains(id)) || !seen.insert(step.id.clone())
            {
                return Err(Error::Conflict("保存的策略依赖已经失效".into()));
            }
            let capability_id = step
                .capability_id
                .as_deref()
                .ok_or_else(|| Error::Conflict("兼容策略缺少领域能力绑定".into()))?;
            let (binding, _) = self.catalog.resolve(capability_id, &step.arguments)?;
            let contract = bridge.describe(&binding.method_id, cancel).await?;
            if contract["catalogVersion"] != binding.catalog_version
                || contract["callable"] != true
                || !crate::extension::validate(&step.arguments, &contract["inputSchema"], "$")
                    .is_empty()
            {
                return Err(Error::Conflict("保存的策略与当前 BGI 能力不兼容".into()));
            }
        }
        for step in &plan.steps {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if unix_now() > run.deadline {
                return Err(Error::Conflict("策略运行超过时限".into()));
            }
            let attempts = self.journal.attempts(&run.id)?;
            if attempts
                .iter()
                .any(|a| a.request["stepId"] == step.id && a.outcome == "verifiedSucceeded")
            {
                continue;
            }
            let capability_id = step
                .capability_id
                .as_deref()
                .ok_or_else(|| Error::Conflict("兼容策略缺少领域能力绑定".into()))?;
            let (binding, resources) = self.catalog.resolve(capability_id, &step.arguments)?;
            run.tool_calls += 1;
            self.journal.save(run, RunState::Executing)?;
            self.journal.emit(
                run,
                "step.started",
                json!({"id":step.id,"title":step.title}),
            )?;
            let result = bridge
                .invoke(
                    &self.journal,
                    run,
                    host::bridge::Invocation {
                        authorization: &self.config,
                        call_id: &format!("strategy-{}-{}", run.id, step.id),
                        binding: &binding,
                        arguments: &step.arguments,
                        resources,
                        catalog: &self.catalog,
                    },
                    policy,
                    cancel,
                )
                .await?;
            self.journal.emit(
                run,
                "step.finished",
                json!({"id":step.id,"outcome":result["outcome"]}),
            )?;
            if result["outcome"] != "verifiedSucceeded" {
                break;
            }
        }
        let attempts = self.journal.attempts(&run.id)?;
        let succeeded = attempts
            .iter()
            .filter(|a| a.outcome == "verifiedSucceeded")
            .count();
        let next = if succeeded == plan.steps.len() {
            RunState::Succeeded
        } else if succeeded > 0 {
            RunState::Partial
        } else if attempts.iter().any(|a| {
            matches!(
                a.outcome.as_str(),
                "unknown" | "running" | "submitting" | "completed"
            )
        }) {
            RunState::NeedsReview
        } else {
            RunState::Failed
        };
        run.result = Some(format!(
            "策略“{}”已执行：{}/{} 个步骤验证成功。",
            plan.goal,
            succeeded,
            plan.steps.len()
        ));
        self.journal.finish(run, next)?;
        Ok(())
    }
    /// 快捷任务的确定性执行：预检 → 执行 → 核验，全程不调用模型。
    async fn run_saved_workflow(
        &self,
        run: &mut Run,
        task_id: &str,
        revision: u64,
        bridge: &host::bridge::Bridge,
        policy: &policy::RuntimeConfig,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let revision = self.tasks.revision(task_id, revision)?;
        // 已发布的修订是运行期唯一依据；再跑一遍静态校验，防止手工改库。
        if !revision.validation.issues.is_empty() {
            self.journal.save(run, RunState::Blocked)?;
            run.error = Some(
                revision
                    .validation
                    .issues
                    .first()
                    .map(|issue| issue.message.clone())
                    .unwrap_or_else(|| "任务定义未通过校验".into()),
            );
            return Ok(());
        }
        self.journal.save(run, RunState::Preflighting)?;
        let mut executor =
            operation::executor::TaskExecutor::new(self, bridge, policy, cancel, &revision, run);
        if let Err(error) = executor.preflight().await {
            // 预检失败且未提交任何外部写入：直接给 blocked，修好后重新点击运行。
            let run = executor.run_mut();
            run.error = Some(error.user_message());
            self.journal.save(run, RunState::Blocked)?;
            return Ok(());
        }
        self.journal.save(executor.run_mut(), RunState::Executing)?;
        let next = match executor.run().await {
            Ok(next) => next,
            Err(Error::Cancelled) => {
                self.journal
                    .finish(executor.run_mut(), RunState::Cancelled)?;
                return Ok(());
            }
            Err(error) => {
                // 执行器只把「结果未确认」的失败留给待核对，其余按已知失败收场。
                let unknown = executor.reports_have_unknown();
                let run = executor.run_mut();
                run.error = Some(error.user_message());
                self.journal.finish(
                    run,
                    if unknown {
                        RunState::NeedsReview
                    } else {
                        RunState::Failed
                    },
                )?;
                return Ok(());
            }
        };
        self.journal.finish(executor.run_mut(), next)?;
        Ok(())
    }
    fn record_result(
        &self,
        run: &Run,
        call: &ToolCall,
        result: Result<Value>,
        result_limit: usize,
        history: &mut Vec<Message>,
    ) -> Result<()> {
        let value = match result {
            Ok(v) => json!({"ok":true,"value":v}),
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(Error::Storage(e)) => return Err(Error::Storage(e)),
            Err(Error::Conflict(e)) => return Err(Error::Conflict(e)),
            Err(e) => json!({"ok":false,"error":e.to_string()}),
        };
        self.journal
            .record_tool(&run.conversation_id, call, &value)?;
        let full = value.to_string();
        let chars = full.chars().count();
        // 预览用工具自己的上限而不是本轮的剩余额度：额度耗尽时预览会变成空串，
        // 模型拿不到任何信息，只会反复重读同一个文件。聚合体积由 context 侧的
        // 清理负责。
        let content = if chars > result_limit {
            let artifact = self.operations.artifacts.put(full.as_bytes())?;
            self.operations
                .store
                .link_artifact(&artifact, "run", &run.id, "toolResult")?;
            json!({"artifactId":artifact,"preview":full.chars().take(result_limit.min(2000)).collect::<String>(),"truncated":true,"originalChars":chars}).to_string()
        } else {
            full
        };
        let m = Message {
            role: Role::Tool,
            content,
            tool_call_id: Some(call.id.clone()),
            tool_calls: vec![],
            reasoning: None,
        };
        self.journal.append_message(run, &m)?;
        history.push(m);
        self.journal.emit(
            run,
            "tool.completed",
            json!({"callId":call.id,"result":value}),
        )?;
        Ok(())
    }
    fn definition(&self, name: &str, exposed: &HashSet<String>) -> Option<ToolDefinition> {
        self.definitions(exposed)
            .into_iter()
            .find(|definition| definition.name == name)
    }
    fn definitions(&self, exposed: &HashSet<String>) -> Vec<ToolDefinition> {
        let mut definitions = self
            .tools()
            .definitions()
            .into_iter()
            .filter(|t| {
                t.execution.always_load || !t.execution.deferred || exposed.contains(&t.name)
            })
            .collect::<Vec<_>>();
        for (name, description, properties, required, execution) in [
            (
                "tools.search",
                "仅在任务明确涉及已安装插件时搜索插件工具。它不包含 BetterGI 原生接口或 User 文件。",
                json!({"query":{"type":"string"}}),
                json!(["query"]),
                ToolExecution::read_only(),
            ),
            (
                "user.ask",
                "仅询问无法从本机文件、接口契约或状态取得，且不同答案会改变目标或不可逆结果的信息。一次问完；不要重复运行时审批。",
                json!({"question":{"type":"string"}}),
                json!(["question"]),
                ToolExecution {
                    effect: ToolEffect::InternalState,
                    deferred: false,
                    always_load: true,
                    ..ToolExecution::default()
                },
            ),
            (
                "plan.update",
                "为两个以上相互依赖的写入或执行动作建立计划。纯查询、一次读取或单项修改不需要计划。",
                json!({"goal":{"type":"string"},"steps":{"type":"array","items":{"type":"object"}}}),
                json!(["goal", "steps"]),
                ToolExecution {
                    effect: ToolEffect::InternalState,
                    deferred: false,
                    always_load: true,
                    ..ToolExecution::default()
                },
            ),
            (
                "artifact.read",
                "仅当工具结果明确返回 artifactId 和 truncated=true 时读取完整结果。",
                json!({"id":{"type":"string"}}),
                json!(["id"]),
                ToolExecution::read_only(),
            ),
            (
                "resource.search",
                "仅搜索已安装插件登记的资源；不搜索 BetterGI 的配置组、路线、脚本或原生接口。",
                json!({"query":{"type":"string"}}),
                json!(["query"]),
                ToolExecution::read_only(),
            ),
            (
                "operation.propose",
                "提交已安装领域插件生成的 MutationPlan。普通 BetterGI 文件和接口操作不使用此入口。",
                json!({"title":{"type":"string"},"plan":{"type":"object"}}),
                json!(["title", "plan"]),
                ToolExecution {
                    effect: ToolEffect::InternalState,
                    risk: crate::extension::RiskLevel::Low,
                    verification: crate::extension::VerificationMode::None,
                    compensation: crate::extension::CompensationMode::None,
                    deferred: false,
                    always_load: true,
                    ..ToolExecution::default()
                },
            ),
            (
                "operation.get",
                "读取事务操作的持久状态、检查点和验证结果",
                json!({"id":{"type":"string"}}),
                json!(["id"]),
                ToolExecution::read_only(),
            ),
            (
                "skills.reference",
                "当已加载 Skill 明确引用同目录资料时读取该资料；不用于发现 BetterGI 内容。",
                json!({"name":{"type":"string"},"path":{"type":"string"}}),
                json!(["name", "path"]),
                ToolExecution::read_only(),
            ),
            (
                "task.save",
                "把用户想反复做的一件事保存成快捷任务。只做静态校验和保存，绝不执行、也绝不产生任何外部写入；用户说「不要现在运行」时照此办理。\
                 步骤类型：tool（固定工具与参数）、sequence（顺序子步骤，nodes）、condition（condition 三值判断，另有 then/otherwise/unknown 三个分支，unknown 必填）、\
                 foreach（items 取值引用、itemKey、nodes、maxItems）、repeat（count 或 until、nodes、maxIterations）、wait（seconds 或 until+timeoutSeconds，可选只读 probe）、\
                 result（受限模板 template，只允许 {{ nodes.<步骤ID>.字段 }}、{{ item }}、{{ iteration }}）。\
                 取值引用写成 {\"kind\":\"nodeOutput\",\"node\":\"<步骤ID>\",\"path\":[\"字段\"]} 或用 {\"kind\":\"literal\",\"value\":…} 写常量。\
                 每个 tool 步骤必须带 id、title、tool、arguments；参数里引用别的步骤输出时写成 {\"$ref\":<取值引用>}。\
                 需要用户先确定的信息先问清楚再保存；工具契约或目标资源还没有证据时保存草稿并说明缺什么。",
                json!({
                    "id":{"type":"string","description":"要修改已有任务时填它的 ID；新建省略"},
                    "name":{"type":"string","description":"任务名称，一到两句话能说明用途"},
                    "description":{"type":"string","description":"一到两句具体说明：做什么、作用在哪个对象上"},
                    "nodes":{"type":"array","items":{"type":"object"},"description":"顶层步骤，通常是 sequence 或 tool"},
                    "limits":{"type":"object","description":"可选；留空用默认上限"},
                    "publish":{"type":"boolean","description":"默认 true。静态校验通过就发布为可运行；false 只留草稿"}
                }),
                json!(["name", "description", "nodes"]),
                ToolExecution {
                    effect: ToolEffect::InternalState,
                    deferred: false,
                    always_load: true,
                    ..ToolExecution::default()
                },
            ),
        ] {
            definitions.push(ToolDefinition{name:name.into(),description:description.into(),input_schema:json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),output_schema:None,source:"core:runtime".into(),provider_version:Some(env!("CARGO_PKG_VERSION").into()),execution});
        }
        definitions
    }
    /// 保存（必要时发布）一个快捷任务定义。
    ///
    /// 这里只做编译与落库：静态校验通过即可发布为「可运行，尚未实机验证」，
    /// 不要求先真的改一个文件或跑一次游戏才配保存。发布本身也不产生外部写入。
    fn save_task(&self, run: &Run, call: &ToolCall, arguments: &Value) -> Result<Value> {
        use operation::task::{TaskLimits, TaskNode, WorkflowDefinition, compile};

        let nodes: Vec<TaskNode> = serde_json::from_value(arguments["nodes"].clone())
            .map_err(|error| Error::Tool(format!("任务定义无法解析：{error}")))?;
        let limits: Option<TaskLimits> = match arguments.get("limits") {
            Some(value) if !value.is_null() => Some(serde_json::from_value(value.clone())?),
            _ => None,
        };
        let task_id = arguments["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        let mut definition = match &task_id {
            Some(id) => {
                let mut existing = self.tasks.definition(id)?;
                if existing.deleted_at.is_some() {
                    return Err(Error::Tool("这个快捷任务已经被删除".into()));
                }
                existing.name = arguments["name"].as_str().unwrap_or(&existing.name).into();
                existing.description = arguments["description"]
                    .as_str()
                    .unwrap_or(&existing.description)
                    .into();
                existing.updated_at = types::now();
                existing
            }
            None => WorkflowDefinition {
                id: uuid::Uuid::new_v4().to_string(),
                name: arguments["name"].as_str().unwrap_or("新快捷任务").into(),
                description: arguments["description"].as_str().unwrap_or("").into(),
                source_conversation_id: Some(run.conversation_id.clone()),
                source_message_id: Some(call.id.clone()),
                source_title_snapshot: run.prompt.chars().take(120).collect(),
                published_revision: None,
                draft_revision: None,
                archived_at: None,
                deleted_at: None,
                pinned: false,
                last_run_id: None,
                created_at: types::now(),
                updated_at: types::now(),
            },
        };
        let revision_number = self.tasks.next_revision_number(&definition.id)?;
        let revision = compile(
            &definition.id,
            revision_number,
            &definition.name,
            &definition.description,
            nodes,
            limits,
            &self.tool_catalog(),
        )?;
        let publish =
            arguments["publish"].as_bool().unwrap_or(true) && revision.validation.publishable();
        definition.draft_revision = Some(revision_number);
        if publish {
            definition.published_revision = Some(revision_number);
            definition.draft_revision = None;
        }
        if task_id.is_some() {
            self.tasks.save_definition(&definition)?;
        } else {
            self.tasks.create_definition(&definition)?;
        }
        self.tasks.save_revision(&revision)?;
        let state = if publish {
            operation::task::DefinitionState::ReadyUnverified
        } else {
            operation::task::DefinitionState::Draft
        };
        Ok(json!({
            "taskId":definition.id,
            "name":definition.name,
            "revision":revision_number,
            "state":state,
            "stateLabel":state.label(),
            "runnable":state.runnable(),
            "zeroToken":revision.model_usage.is_deterministic(),
            "modelUsage":revision.model_usage,
            "issues":revision.validation.issues,
            "missingBindings":revision.validation.missing_bindings,
            "note":"已保存到当前对话的任务清单和快捷任务页；没有被执行。",
        }))
    }

    /// 按契约声明算出本次写入的实际影响。
    ///
    /// 读的是目标资源的当前内容，不是工具名 —— 「改一个字段」和「整份替换」在
    /// 参数上长得一样，差别只在真实差异。取不到差异就返回 `None`：引擎不猜，
    /// 由上层按未界定处理，而不是笼统地把所有写入都拦下来。
    async fn change_scope(
        &self,
        call: &ToolCall,
        definition: &ToolDefinition,
        cancel: &CancellationToken,
    ) -> Option<operation::permissions::ChangeScope> {
        use crate::extension::ScopeKind;
        use operation::permissions::ChangeScope;
        match definition.execution.scope {
            ScopeKind::Unknown => None,
            ScopeKind::Delete => Some(ChangeScope::delete()),
            ScopeKind::Whole => Some(ChangeScope::whole()),
            ScopeKind::Objects => {
                let target = definition.execution.scope_target.as_deref()?;
                let count = call.arguments[target].as_array()?.len();
                Some(ChangeScope {
                    objects: count,
                    fields: 0,
                    ..ChangeScope::default()
                })
            }
            ScopeKind::Fields => {
                let target = definition.execution.scope_target.as_deref()?;
                let next = call.arguments["content"].as_str()?;
                let reader = definition.execution.scope_reader.as_deref()?;
                let path = call.arguments[target].as_str()?;
                // 截断的旧内容不是差异基线，宁可不界定也不能少算改动。
                let previous = self
                    .tools()
                    .call_async(reader, &json!({"path": path}), cancel.clone())
                    .await
                    .ok()
                    .filter(|value| value["truncated"] != true)
                    .and_then(|value| value["text"].as_str().map(str::to_owned));
                match previous {
                    Some(previous) => ChangeScope::from_contents(Some(&previous), next),
                    // 没有版本前置条件就是新建；有前置条件却读不到旧内容，
                    // 说明差异无法界定。
                    None if call.arguments.get("expectedSha256").is_none() => {
                        ChangeScope::from_contents(None, next)
                    }
                    None => None,
                }
            }
        }
    }

    async fn tool(
        &self,
        run: &mut Run,
        call: &ToolCall,
        bridge: &host::bridge::Bridge,
        policy: &policy::RuntimeConfig,
        cancel: &CancellationToken,
        exposed: &mut HashSet<String>,
    ) -> Result<Value> {
        let a = &call.arguments;
        let current = self.config.read().unwrap().clone();
        let definitions = self.definitions(exposed);
        let definition = definitions
            .iter()
            .find(|t| t.name == call.name)
            .ok_or_else(|| Error::Tool("工具未被发现或未授权".into()))?;
        self.metric(
            "tool.call",
            1.0,
            "count",
            json!({"tool":call.name,"effect":definition.execution.effect}),
        );
        self.emit_lifecycle(
            host::hooks::HookEventKind::BeforeToolUse,
            Some(&run.id),
            None,
            json!({"tool":call.name,"argumentsHash":hash(a)}),
            cancel,
        )
        .await?;
        if !crate::extension::validate(a, &definition.input_schema, "$").is_empty() {
            return Err(Error::Tool("工具参数不符合契约".into()));
        }
        match call.name.as_str() {
            "tools.search" => {
                let q = a["query"].as_str().unwrap_or("").to_lowercase();
                let found = self
                    .tools()
                    .definitions()
                    .into_iter()
                    .filter(|t| {
                        t.source.starts_with("plugin:")
                            && (t.name.to_lowercase().contains(&q)
                                || t.description.to_lowercase().contains(&q)
                                || t.execution
                                    .search_hint
                                    .as_ref()
                                    .is_some_and(|hint| hint.to_lowercase().contains(&q)))
                    })
                    .take(8)
                    .collect::<Vec<_>>();
                for t in &found {
                    exposed.insert(t.name.clone());
                }
                Ok(json!(found))
            }
            "resource.search" => {
                let query = a["query"].as_str().unwrap_or("").to_lowercase();
                let resources = self
                    .operations
                    .store
                    .resources()?
                    .into_iter()
                    .filter(|resource| {
                        format!(
                            "{} {} {} {}",
                            resource.id, resource.provider_id, resource.kind, resource.display_name
                        )
                        .to_lowercase()
                        .contains(&query)
                    })
                    .take(20)
                    .collect::<Vec<_>>();
                Ok(json!({"resources":resources,"legacyCatalog":self.catalog.search(&query)}))
            }
            "operation.propose" => {
                let plan: operation::kernel::MutationPlan =
                    serde_json::from_value(a["plan"].clone())?;
                let operation = self.operations.store.create(
                    Some(&run.id),
                    a["title"].as_str().unwrap_or("领域操作"),
                    plan,
                )?;
                let operation = self.operations.request_authorization(&operation.id)?;
                let mut grants = current.runtime.trust_grants.clone();
                grants.extend(self.operations.store.grants().unwrap_or_default());
                let operation = if self.operations.permission_decision(
                    &operation,
                    current.runtime.permission_mode,
                    &grants,
                )? == operation::permissions::PermissionDecision::Allow
                {
                    self.operations.execute_pre_authorized(&operation.id)?
                } else {
                    operation
                };
                self.journal.emit(
                    run,
                    "operation.proposed",
                    json!({"operationId":operation.id,"state":operation.state}),
                )?;
                Ok(
                    json!({"requiresAuthorization":operation.state == operation::kernel::OperationState::AwaitingAuthorization,"operation":operation}),
                )
            }
            "operation.get" => Ok(json!(
                self.operations.store.get(a["id"].as_str().unwrap_or(""))?
            )),
            "task.save" => self.save_task(run, call, a),
            "skills.reference" => {
                let name = a["name"].as_str().unwrap_or("");
                if current.agent.disabled_skills.iter().any(|n| n == name) {
                    return Err(Error::Tool("Skill 已停用".into()));
                }
                Ok(
                    json!({"content":self.skills().read_reference(name,a["path"].as_str().unwrap_or(""))?}),
                )
            }
            "bgi.state.get" => bridge.get("/bridge/v1/state", cancel).await,
            "bgi.api.search" => {
                let query = {
                    let mut query = url::form_urlencoded::Serializer::new(String::new());
                    query
                        .append_pair("q", a["query"].as_str().unwrap_or(""))
                        .append_pair("limit", &a["limit"].as_u64().unwrap_or(8).to_string())
                        .append_pair("offset", &a["offset"].as_u64().unwrap_or(0).to_string());
                    if let Some(group) = a["group"].as_str() {
                        query.append_pair("group", group);
                    }
                    query.finish()
                };
                bridge
                    .get(&format!("/bridge/v1/catalog?{query}"), cancel)
                    .await
            }
            "bgi.api.describe" => {
                let id = a["methodId"].as_str().unwrap_or("");
                let contract = bridge.describe(id, cancel).await?;
                let version = contract["catalogVersion"]
                    .as_str()
                    .ok_or_else(|| Error::Tool("当前 BetterGI 连接缺少接口说明".into()))?;
                if !contract["guide"].is_object() {
                    return Err(Error::Tool("当前 BetterGI 连接缺少接口说明".into()));
                }
                exposed.insert(format!("bridge-api:{id}:{version}"));
                Ok(contract)
            }
            "bgi.api.read" | "bgi.api.invoke" => {
                let id = a["methodId"].as_str().unwrap_or("");
                let contract = bridge.describe(id, cancel).await?;
                let version = contract["catalogVersion"]
                    .as_str()
                    .ok_or_else(|| Error::Tool("当前 BetterGI 连接缺少接口说明".into()))?;
                if !exposed.contains(&format!("bridge-api:{id}:{version}")) {
                    return Err(Error::Tool(
                        "请先用 bgi.api.describe 阅读当前版本的完整调用说明".into(),
                    ));
                }
                if contract["callable"] != true {
                    return Err(Error::Tool(
                        contract["unavailableReason"]
                            .as_str()
                            .unwrap_or("接口当前不可调用")
                            .into(),
                    ));
                }
                let arguments = &a["arguments"];
                let issues = crate::extension::validate(arguments, &contract["inputSchema"], "$");
                if !issues.is_empty() {
                    return Err(Error::Tool(format!(
                        "参数不符合接口契约：{}",
                        json!(issues)
                    )));
                }
                if call.name == "bgi.api.read" {
                    if contract["effect"] != "readOnly" {
                        return Err(Error::Tool(
                            "只读入口拒绝写接口；请使用需要授权的 bgi.api.invoke".into(),
                        ));
                    }
                    let info = bridge.get("/bridge/v1/info", cancel).await?;
                    if info["catalogVersion"] != contract["catalogVersion"]
                        || info["instanceId"] != contract["instanceId"]
                    {
                        return Err(Error::Tool("桥实例或接口契约已变化，请重新读取".into()));
                    }
                    let result = bridge.request("POST", "/bridge/v1/invoke", Some(&json!({
                        "methodId":id,"arguments":arguments,"instanceId":info["instanceId"],"catalogVersion":version
                    })), cancel).await?;
                    Ok(result["result"].clone())
                } else {
                    if contract["effect"] == "readOnly" {
                        return Err(Error::Tool("此接口只读，请使用 bgi.api.read".into()));
                    }
                    if current.runtime.permission_mode
                        == operation::permissions::PermissionMode::PlanOnly
                    {
                        return Err(Error::Tool(
                            "当前为只读规划模式，不能修改配置或执行命令".into(),
                        ));
                    }
                    if id == "bgi.run_script_group" {
                        let host = bridge.get("/bridge/v1/host", cancel).await?;
                        if let Some(root) = host["userPath"].as_str() {
                            let group = arguments["name"]
                                .as_str()
                                .or_else(|| arguments["groupName"].as_str())
                                .or_else(|| arguments["scriptGroupName"].as_str())
                                .unwrap_or("");
                            if let Some(missing) = crate::bridge::resolve::missing_paths_for_group(
                                Path::new(root),
                                group,
                            )
                            .filter(|missing| !missing.is_empty())
                            {
                                return Err(Error::Tool(format!(
                                    "配置组「{group}」引用的路径已经不在本机：{}。先更新或订阅这些路径，不要空跑。",
                                    missing.join("、")
                                )));
                            }
                        }
                    }
                    let mut catalog = self.catalog.clone();
                    catalog.capabilities.insert(
                        id.into(),
                        host::catalog::Capability {
                            id: id.into(),
                            method_id: id.into(),
                            description: contract["summary"].as_str().unwrap_or(id).into(),
                            catalog_version: version.into(),
                            aliases: vec![],
                            resource_fields: vec![],
                            postconditions: vec![],
                        },
                    );
                    let (binding, resources) = catalog.resolve(id, arguments)?;
                    bridge
                        .invoke(
                            &self.journal,
                            run,
                            host::bridge::Invocation {
                                authorization: &self.config,
                                call_id: &call.id,
                                binding: &binding,
                                arguments,
                                resources,
                                catalog: &catalog,
                            },
                            policy,
                            cancel,
                        )
                        .await
                }
            }
            "bgi.capability.search" => Ok(self.catalog.search(a["query"].as_str().unwrap_or(""))),
            "bgi.capability.describe" => {
                let id = a["methodId"].as_str().unwrap_or("");
                let c = self
                    .catalog
                    .capabilities
                    .get(id)
                    .ok_or_else(|| Error::Tool("能力尚未建立语义绑定".into()))?;
                let v = bridge.describe(&c.method_id, cancel).await?;
                if v["catalogVersion"] != c.catalog_version {
                    return Err(Error::Tool("能力绑定版本失效".into()));
                }
                exposed.insert(id.into());
                Ok(json!({"semantic":c,"contract":v}))
            }
            "bgi.capability.invoke" => {
                let id = a["methodId"].as_str().unwrap_or("");
                if !exposed.contains(id) {
                    return Err(Error::Tool("请先读取该能力的完整契约".into()));
                }
                let (binding, resources) = self.catalog.resolve(id, &a["arguments"])?;
                self.journal
                    .emit(run, "resource.bound", resources.clone())?;
                bridge
                    .invoke(
                        &self.journal,
                        run,
                        host::bridge::Invocation {
                            authorization: &self.config,
                            call_id: &call.id,
                            binding: &binding,
                            arguments: &a["arguments"],
                            resources,
                            catalog: &self.catalog,
                        },
                        policy,
                        cancel,
                    )
                    .await
            }
            "skills.search" => Ok(json!(
                self.skills()
                    .search(a["query"].as_str().unwrap_or(""), 8)
                    .into_iter()
                    .filter(|s| !current.agent.disabled_skills.contains(&s.name))
                    .collect::<Vec<_>>()
            )),
            "skills.read" => {
                let name = a["name"].as_str().unwrap_or("");
                if current.agent.disabled_skills.iter().any(|n| n == name) {
                    return Err(Error::Tool("Skill 已停用".into()));
                }
                let skills = self.skills();
                let skill = skills
                    .get(name)
                    .ok_or_else(|| Error::Tool("Skill 不存在".into()))?;
                Ok(
                    json!({"name":name,"instructions":skill.body,"version":hash(&json!(skill.body))}),
                )
            }
            "plugins.list" => Ok(self.tools().call(&call.name, a)),
            "artifact.read" => Ok(
                json!({"content":if self.operations.store.can_read_artifact(a["id"].as_str().unwrap_or(""),"run",&run.id)? {
                    String::from_utf8_lossy(&self.operations.artifacts.get(a["id"].as_str().unwrap_or(""))?).chars().take(12000).collect::<String>()
                } else {
                    self.journal.read_artifact(&run.id,a["id"].as_str().unwrap_or(""))?.chars().take(12000).collect::<String>()
                }}),
            ),
            "user.ask" => {
                self.journal.save(run, RunState::AwaitingUser)?;
                self.journal
                    .emit(run, "question", json!({"question":a["question"]}))?;
                let notifier = self.journal.notifier();
                loop {
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    if unix_now() > run.deadline {
                        return Err(Error::Conflict("等待回复超时".into()));
                    }
                    let input = self.journal.drain_inputs(&run.id)?;
                    if !input.is_empty() {
                        return Ok(json!({"answer":input.join("\n")}));
                    }
                    // Wake on the next input instead of polling every 100 ms.
                    tokio::select! {
                        _ = notifier.notified() => {}
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    }
                }
            }
            "plan.update" => {
                let previous = self.journal.plan(&run.id)?;
                let revision = previous.as_ref().map(|p| p.revision + 1).unwrap_or(1);
                if revision > policy.max_replans + 1 {
                    return Err(Error::Conflict("重规划次数已达上限".into()));
                }
                let mut steps: Vec<PlanStep> = serde_json::from_value(a["steps"].clone())?;
                if let Some(previous) = &previous {
                    let attempts = self.journal.attempts(&run.id)?;
                    for old in &previous.steps {
                        if attempts.iter().any(|a| a.request["stepId"] == old.id)
                            && !steps
                                .iter()
                                .any(|s| s.id == old.id && json!(s) == json!(old))
                        {
                            return Err(Error::Tool("已经执行的步骤不可改写或删除".into()));
                        }
                    }
                }
                let mut seen = HashSet::new();
                if steps.len() > policy.max_tools {
                    return Err(Error::Tool("计划步骤过多".into()));
                }
                for step in &mut steps {
                    if !step.depends_on.iter().all(|id| seen.contains(id))
                        || !seen.insert(step.id.clone())
                    {
                        return Err(Error::Tool(
                            "计划依赖必须指向先前步骤，且 ID 不可重复".into(),
                        ));
                    }
                    match (step.capability_id.as_deref(), step.tool.as_deref()) {
                        (Some(capability_id), None) => {
                            let (binding, _) =
                                self.catalog.resolve(capability_id, &step.arguments)?;
                            let d = bridge.describe(&binding.method_id, cancel).await?;
                            if d["catalogVersion"] != binding.catalog_version
                                || d["callable"] != true
                                || !crate::extension::validate(
                                    &step.arguments,
                                    &d["inputSchema"],
                                    "$",
                                )
                                .is_empty()
                            {
                                return Err(Error::Tool("计划包含无效领域能力或参数".into()));
                            }
                        }
                        (None, Some(tool)) => {
                            let definition = self
                                .definition(tool, exposed)
                                .ok_or_else(|| Error::Tool("计划引用了未发现的工具".into()))?;
                            if !crate::extension::validate(
                                &step.arguments,
                                &definition.input_schema,
                                "$",
                            )
                            .is_empty()
                            {
                                return Err(Error::Tool("计划包含无效工具参数".into()));
                            }
                            step.execution = Some(definition.execution);
                            step.provider_version = definition.provider_version;
                        }
                        _ => {
                            return Err(Error::Tool(
                                "计划步骤必须且只能绑定 capabilityId 或 tool".into(),
                            ));
                        }
                    }
                }
                let plan = PlanRevision {
                    revision,
                    goal: a["goal"].as_str().unwrap_or("").into(),
                    steps,
                };
                self.emit_lifecycle(
                    host::hooks::HookEventKind::BeforePlanCommit,
                    Some(&run.id),
                    None,
                    json!({"plan":plan}),
                    cancel,
                )
                .await?;
                self.journal.save_plan(run, &plan)?;
                for step in &plan.steps {
                    let attempts = self.journal.attempts(&run.id)?;
                    if attempts
                        .iter()
                        .any(|a| a.request["stepId"] == step.id && a.outcome == "verifiedSucceeded")
                    {
                        continue;
                    }
                    if self.journal.has_inputs(&run.id)? {
                        break;
                    }
                    if run.tool_calls >= policy.max_tools || unix_now() > run.deadline {
                        return Err(Error::Conflict("计划执行达到预算上限".into()));
                    }
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    self.journal.save(run, RunState::Executing)?;
                    let call_id = format!("{}-{}", call.id, step.id);
                    run.tool_calls += 1;
                    self.journal.save(run, RunState::Executing)?;
                    self.journal.emit(
                        run,
                        "step.started",
                        json!({"id":step.id,"title":step.title}),
                    )?;
                    let result = if let Some(capability_id) = step.capability_id.as_deref() {
                        let (binding, resources) =
                            self.catalog.resolve(capability_id, &step.arguments)?;
                        bridge
                            .invoke(
                                &self.journal,
                                run,
                                host::bridge::Invocation {
                                    authorization: &self.config,
                                    call_id: &call_id,
                                    binding: &binding,
                                    arguments: &step.arguments,
                                    resources,
                                    catalog: &self.catalog,
                                },
                                policy,
                                cancel,
                            )
                            .await?
                    } else {
                        let tool_name = step
                            .tool
                            .as_deref()
                            .ok_or_else(|| Error::Tool("计划工具缺失".into()))?;
                        Box::pin(self.tool(
                            run,
                            &ToolCall {
                                id: call_id,
                                name: tool_name.into(),
                                arguments: step.arguments.clone(),
                            },
                            bridge,
                            policy,
                            cancel,
                            exposed,
                        ))
                        .await?
                    };
                    let outcome = result
                        .pointer("/verification/status")
                        .and_then(Value::as_str)
                        .map(|status| match status {
                            "succeeded" => "verifiedSucceeded",
                            "failed" => "verifiedFailed",
                            _ => "unknown",
                        })
                        .unwrap_or_else(|| {
                            if step.capability_id.is_some() {
                                result["outcome"].as_str().unwrap_or("unknown")
                            } else {
                                "completed"
                            }
                        });
                    self.journal.emit(
                        run,
                        "step.finished",
                        json!({"id":step.id,"outcome":outcome,"planRevision":plan.revision}),
                    )?;
                    if matches!(outcome, "verifiedFailed" | "unknown") {
                        break;
                    }
                }
                Ok(json!({"plan":plan,"attempts":self.journal.attempts(&run.id)?}))
            }
            // 用户文件是本地文件，不是游戏对象，也不属于任何插件。
            "bgi.user.list" | "bgi.user.read" | "bgi.user.inspect_script" | "bgi.user.resolve" => {
                let registry = self.tools().clone();
                let call = call.clone();
                registry
                    .call_async(&call.name, &call.arguments, cancel.clone())
                    .await
            }
            "bgi.job.get" => {
                let registry = self.tools().clone();
                let call = call.clone();
                registry
                    .call_async(&call.name, &call.arguments, cancel.clone())
                    .await
            }
            "bgi.user.write" | "bgi.user.restore" | "bgi.job.cancel" => {
                let request = json!({"methodId":call.name,"arguments":a});
                let scope = self.change_scope(call, definition, cancel).await;
                let permission = operation::permissions::PermissionEngine::decide(
                    current.runtime.permission_mode,
                    &operation::permissions::PermissionRequest {
                        provider_id: "core:bgi",
                        resource_ids: &[],
                        resource_kinds: &[],
                        effect: definition.execution.effect,
                        risk: definition.execution.risk,
                        unattended: definition.execution.unattended,
                        scope,
                    },
                    &current.runtime.trust_grants,
                );
                if permission == operation::permissions::PermissionDecision::Deny {
                    return Err(Error::Tool("当前处于只读规划模式，禁止执行此写操作".into()));
                }
                if !current.runtime.allows(&request)
                    && permission != operation::permissions::PermissionDecision::Allow
                {
                    let approval = Approval {
                        id: uuid::Uuid::new_v4().to_string(),
                        run_id: run.id.clone(),
                        request_hash: hash(&request),
                        request: request.clone(),
                        expires_at: unix_now() + 300,
                        decision: None,
                    };
                    self.journal.approval(&approval)?;
                    self.journal.save(run, RunState::AwaitingApproval)?;
                    self.journal
                        .emit(run, "approval.requested", json!(approval))?;
                    loop {
                        if cancel.is_cancelled() {
                            return Err(Error::Cancelled);
                        }
                        if unix_now() > approval.expires_at || unix_now() > run.deadline {
                            self.journal.save(run, RunState::Executing)?;
                            return Err(Error::Tool("等待操作授权超时".into()));
                        }
                        if let Some(decision) = self.journal.approval_result(&approval.id)?.decision
                        {
                            if !decision {
                                self.journal.save(run, RunState::Executing)?;
                                return Err(Error::Tool("用户拒绝了本次操作".into()));
                            }
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    self.journal.save(run, RunState::Executing)?;
                }
                let registry = self.tools().clone();
                let call = call.clone();
                registry
                    .call_async(&call.name, &call.arguments, cancel.clone())
                    .await
            }
            _ => {
                let plugin = definition
                    .source
                    .split(':')
                    .nth(1)
                    .ok_or_else(|| Error::Tool("无效插件来源".into()))?;
                if !definition.source.starts_with("plugin:")
                    || !current.plugins.enabled.iter().any(|p| p == plugin)
                {
                    return Err(Error::Tool("插件已停用或不可用".into()));
                }
                if definition.execution.effect == ToolEffect::ReadOnly {
                    // The read-only effect is declared by the plugin manifest, so an
                    // auto-approved call is recorded rather than run silently.
                    self.journal.emit(
                        run,
                        "plugin.readOnly",
                        json!({"tool":call.name,"plugin":plugin,"declaredEffect":"readOnly"}),
                    )?;
                    let registry = self.tools().clone();
                    let call = call.clone();
                    return match tokio::time::timeout(
                        Duration::from_millis(definition.execution.timeout_ms),
                        registry.call_async(&call.name, &call.arguments, cancel.clone()),
                    )
                    .await
                    {
                        Ok(Ok(value)) => Ok(value),
                        Ok(Err(Error::Cancelled)) => Err(Error::Cancelled),
                        Ok(Err(error)) => Err(Error::Tool(error.user_message())),
                        Err(_) => Err(Error::Tool("只读插件等待超时".into())),
                    };
                }
                let instance = if current.bridge.enabled {
                    bridge.get("/bridge/v1/info", cancel).await?["instanceId"]
                        .as_str()
                        .ok_or_else(|| Error::Tool("缺少游戏实例标识".into()))?
                        .to_owned()
                } else {
                    "plugins-global".into()
                };
                let attempts = self.journal.attempts(&run.id)?;
                let plan = self.journal.plan(&run.id)?;
                let completed = |step: &str| {
                    attempts.iter().any(|attempt| {
                        attempt.request["stepId"] == step && attempt.outcome == "verifiedSucceeded"
                    })
                };
                let step_id = plan
                    .as_ref()
                    .and_then(|plan| {
                        plan.steps.iter().find(|step| {
                            step.tool.as_deref() == Some(call.name.as_str())
                                && step.arguments == *a
                                && step
                                    .depends_on
                                    .iter()
                                    .all(|dependency| completed(dependency))
                                && !attempts
                                    .iter()
                                    .any(|attempt| attempt.request["stepId"] == step.id)
                        })
                    })
                    .map(|step| step.id.clone());
                let request = json!({"methodId":call.name,"arguments":a,"catalogVersion":hash(&json!(definition)),"instanceId":instance,"stepId":step_id});
                let scope = self.change_scope(call, definition, cancel).await;
                let permission = operation::permissions::PermissionEngine::decide(
                    current.runtime.permission_mode,
                    &operation::permissions::PermissionRequest {
                        provider_id: plugin,
                        resource_ids: &[],
                        resource_kinds: &[],
                        effect: definition.execution.effect,
                        risk: definition.execution.risk,
                        unattended: definition.execution.unattended,
                        scope,
                    },
                    &current.runtime.trust_grants,
                );
                if permission == operation::permissions::PermissionDecision::Deny {
                    return Err(Error::Tool("当前处于只读规划模式，禁止执行写操作".into()));
                }
                if !current.runtime.allows(&request)
                    && permission != operation::permissions::PermissionDecision::Allow
                {
                    let approval = Approval {
                        id: uuid::Uuid::new_v4().to_string(),
                        run_id: run.id.clone(),
                        request_hash: hash(&request),
                        request: request.clone(),
                        expires_at: unix_now() + 300,
                        decision: None,
                    };
                    self.journal.approval(&approval)?;
                    self.journal.save(run, RunState::AwaitingApproval)?;
                    self.journal
                        .emit(run, "approval.requested", json!(approval))?;
                    loop {
                        if cancel.is_cancelled() {
                            return Err(Error::Cancelled);
                        }
                        if unix_now() > approval.expires_at || unix_now() > run.deadline {
                            self.journal.save(run, RunState::Executing)?;
                            return Err(Error::Tool("等待插件授权超时".into()));
                        }
                        if let Some(decision) = self.journal.approval_result(&approval.id)?.decision
                        {
                            if !decision {
                                self.journal.save(run, RunState::Executing)?;
                                return Err(Error::Tool("用户拒绝插件调用".into()));
                            }
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    self.journal.save(run, RunState::Executing)?;
                }
                if !self
                    .config
                    .read()
                    .unwrap()
                    .plugins
                    .enabled
                    .iter()
                    .any(|p| p == plugin)
                {
                    return Err(Error::Tool("插件已停用".into()));
                }
                let instance = request["instanceId"].as_str().unwrap();
                let mut attempt = self
                    .journal
                    .prepare(run, &call.id, request.clone(), instance)?;
                while !self.journal.acquire(&attempt)? {
                    if cancel.is_cancelled() || unix_now() > run.deadline {
                        attempt.outcome = "notSubmitted".into();
                        self.journal.attempt(&attempt)?;
                        return Err(Error::Cancelled);
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                attempt.outcome = "running".into();
                self.journal.attempt(&attempt)?;
                let registry = self.tools().clone();
                let call = call.clone();
                let outcome = tokio::time::timeout(
                    Duration::from_millis(definition.execution.timeout_ms),
                    registry.call_async(&call.name, &call.arguments, cancel.clone()),
                )
                .await;
                match outcome {
                    Ok(Ok(value)) => {
                        attempt.outcome = match value
                            .pointer("/verification/status")
                            .and_then(Value::as_str)
                        {
                            Some("succeeded") => "verifiedSucceeded",
                            Some("failed") => "verifiedFailed",
                            _ => "completed",
                        }
                        .into();
                        attempt.evidence = value.clone();
                        self.journal.attempt(&attempt)?;
                        self.journal.release(&attempt)?;
                        Ok(value)
                    }
                    Ok(Err(error))
                        if definition.execution.cancellation
                            == crate::extension::CancellationMode::Reliable =>
                    {
                        attempt.outcome = "failed".into();
                        attempt.evidence =
                            json!({"error":error.user_message(),"termination":"confirmed"});
                        self.journal.attempt(&attempt)?;
                        self.journal.release(&attempt)?;
                        Err(Error::Tool(error.user_message()))
                    }
                    other => {
                        attempt.outcome = "unknown".into();
                        attempt.evidence =
                            json!({"termination":"unconfirmed","result":format!("{other:?}")});
                        self.journal.attempt(&attempt)?;
                        Err(Error::Conflict(
                            "插件执行结果或停止状态未确认，已保留执行锁".into(),
                        ))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    /// 底座提示词只放与领域无关的规则。BGI 的工具名和领域流程一旦出现
    /// 在这里，就等于所有提供方都要背 BGI 的规则。
    #[test]
    fn core_policy_stays_free_of_domain_knowledge() {
        for leaked in ["bgi.", "BetterGI", "配置组", "调度器", "User 目录"] {
            assert!(
                !CORE_AGENT_POLICY.contains(leaked),
                "领域内容混进了全局提示词：{leaked}"
            );
        }
        for required in ["先给结果", "无法观测", "不是角色扮演", "内部实现"] {
            assert!(
                CORE_AGENT_POLICY.contains(required),
                "missing policy: {required}"
            );
        }
    }

    /// 领域说明绑定在提供方上：桥没开，BGI 的行为规范与操作手册都不注入。
    #[test]
    fn bgi_skills_are_gated_on_the_bridge_provider() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
        let mut registry = crate::extension::skills::SkillRegistry::default();
        registry
            .load(&[(root, "shipped".into())])
            .expect("shipped skills load");

        let empty = std::collections::HashSet::new();
        let kinds = std::collections::HashSet::new();
        let with_bgi = std::collections::HashSet::from(["bgi".to_owned()]);
        for name in ["bgi-assistant", "bgi-operator"] {
            let skill = registry.get(name).expect(name);
            assert!(skill.always_load, "{name} 应当随提供方在线时自动加载");
            assert_eq!(skill.requires_providers, vec!["bgi".to_owned()]);
            let context = crate::extension::skills::SkillContext {
                plugins: &empty,
                capabilities: &empty,
                resource_kinds: &kinds,
                providers: &empty,
                platform: std::env::consts::OS,
            };
            assert!(
                !registry.eligible(skill, &context),
                "{name} 在桥关闭时不该注入"
            );
            let context = crate::extension::skills::SkillContext {
                providers: &with_bgi,
                ..context
            };
            assert!(
                registry.eligible(skill, &context),
                "{name} 在桥开启时应当注入"
            );
        }
    }

    #[test]
    fn obsolete_generated_prompt_is_suppressed_but_user_prompt_is_preserved() {
        assert_eq!(
            configured_agent_instructions(
                "你是 Sleepy Doll，一个操作 BetterGI 的桌面 Agent。\n# 用户配置在文件里"
            ),
            ""
        );
        assert_eq!(
            configured_agent_instructions("回答时使用简体中文"),
            "回答时使用简体中文"
        );
    }
}
