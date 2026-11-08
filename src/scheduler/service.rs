use super::{
    digest,
    model::*,
    store::Store,
    system::{TaskBackend, WindowsTasks},
};
use crate::{
    Error, Result,
    runtime::{
        Supervisor,
        operation::task::{ModelUsage, TaskNode, bound_fingerprint},
        types::RunState,
    },
};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

pub struct Scheduler {
    pub(crate) store: Store,
    backend: Arc<dyn TaskBackend>,
    config_path: PathBuf,
    supervisor: OnceLock<Weak<Supervisor>>,
    enabled: AtomicBool,
    stop: CancellationToken,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    mutation: Mutex<()>,
    allowed_calls: Mutex<HashSet<(String, String)>>,
    pub(super) default_actions: Mutex<
        std::collections::HashMap<
            (String, DefaultAction),
            crate::runtime::operation::shortcuts::ShortcutCall,
        >,
    >,
}

impl Scheduler {
    pub fn open(config_path: &Path, config: &crate::AppConfig) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            store: Store::open(&config.storage.database)?,
            backend: Arc::new(WindowsTasks::new(config_path)?),
            config_path: config_path.to_path_buf(),
            supervisor: OnceLock::new(),
            enabled: AtomicBool::new(config.plugin_enabled(PLUGIN_ID)),
            stop: CancellationToken::new(),
            worker: Mutex::new(None),
            mutation: Mutex::new(()),
            allowed_calls: Mutex::new(HashSet::new()),
            default_actions: Mutex::new(Default::default()),
        }))
    }

    pub fn server(self: &Arc<Self>) -> Result<crate::extension::mcp_server::RegistryServer> {
        super::tools::server(self.clone())
    }

    /// 应用提供方登记准入；该接口不通过 MCP、IPC 或插件清单暴露给模型。
    pub fn register_action(&self, name: &str, source: &str) -> Result<()> {
        if name.is_empty() || source.is_empty() || name.starts_with("scheduler.") {
            return Err(Error::Config("定时动作登记无效".into()));
        }
        self.allowed_calls
            .lock()
            .unwrap()
            .insert((name.into(), source.into()));
        Ok(())
    }

    fn prepare_workflow(
        &self,
        workflow: &mut crate::runtime::operation::task::WorkflowRevision,
        definitions: &[crate::extension::ToolDefinition],
    ) -> Result<()> {
        for node in &mut workflow.nodes {
            if let TaskNode::Wait(wait) = node {
                if wait.seconds.is_none() || wait.until.is_some() || wait.probe.is_some() {
                    return Err(Error::Config("定时任务的等待步骤只接受固定时长".into()));
                }
                continue;
            }
            let TaskNode::Tool(node) = node else {
                return Err(Error::Config(
                    "定时任务只接纳固定动作，不接纳循环或动态分支".into(),
                ));
            };
            if serde_json::to_string(&node.arguments)?.contains("{{") {
                return Err(Error::Config(
                    "定时执行只接纳固定参数，不能绑定运行时参数模板".into(),
                ));
            }
            let name = node
                .tool
                .as_deref()
                .ok_or_else(|| Error::Config("任务没有绑定工具".into()))?;
            let tool = definitions
                .iter()
                .find(|tool| tool.name == name)
                .ok_or_else(|| Error::Config("目标工具尚未启用".into()))?;
            let bgi = name.starts_with("bgi.")
                && matches!(tool.source.as_str(), "core:bgi" | "plugin:bgi:mcp:bridge");
            let registered = self
                .allowed_calls
                .lock()
                .unwrap()
                .contains(&(name.into(), tool.source.clone()));
            if name.starts_with("scheduler.") || (!bgi && !registered) {
                return Err(Error::Config("此动作尚未登记为可定时执行的动作".into()));
            }
            let native = if crate::bridge::scheduled::command_plan_mode(node).is_some() {
                crate::bridge::scheduled::bind_command_plan(
                    node,
                    self.supervisor()?.bridge_config(),
                )?;
                true
            } else {
                crate::bridge::scheduled::prepare(node)?
            };
            if !native
                && (tool.execution.model_usage != ModelUsage::None
                    || tool.execution.effect_is_dynamic
                    || tool.execution.effect == crate::extension::ToolEffect::Unknown
                    || tool.execution.unattended != crate::extension::UnattendedPolicy::Allowed
                    || tool.execution.risk == crate::extension::RiskLevel::Irreversible)
            {
                return Err(Error::Config(format!(
                    "动作「{}」不支持无人值守执行",
                    tool.label
                )));
            }
            if native && let Some(policy) = node.execution.as_mut() {
                policy.model_usage = ModelUsage::None;
            }
        }
        workflow.model_usage = ModelUsage::None;
        workflow.validation.model_usage = ModelUsage::None;
        Ok(())
    }

    pub fn attach(self: &Arc<Self>, supervisor: &Arc<Supervisor>) -> Result<()> {
        self.supervisor
            .set(Arc::downgrade(supervisor))
            .map_err(|_| Error::Conflict("定时执行器已经绑定".into()))?;
        if let Err(error) = self.configure(self.enabled.load(Ordering::SeqCst)) {
            log::warn!("定时任务停用同步未完成：{error}");
        }
        self.store.recover()?;
        let weak = Arc::downgrade(self);
        *self.worker.lock().unwrap() = Some(
            std::thread::Builder::new()
                .name("scheduled-tasks".into())
                .spawn(move || {
                    let runtime = match tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(value) => value,
                        Err(error) => {
                            log::error!("定时执行器启动失败：{error}");
                            return;
                        }
                    };
                    loop {
                        let Some(service) = weak.upgrade() else {
                            break;
                        };
                        if service.stop.is_cancelled() {
                            break;
                        }
                        if let Err(error) = service.store.heartbeat() {
                            log::error!("定时执行器心跳写入失败：{error}");
                            break;
                        }
                        if let Err(error) = service.reconcile_stopping() {
                            log::warn!("定时任务停止状态核对失败：{error}");
                        }
                        match service.store.claim() {
                            Ok(Some(mut occurrence)) => {
                                if let Err(error) =
                                    runtime.block_on(service.execute(&mut occurrence))
                                {
                                    occurrence.state = "unknown".into();
                                    occurrence.error = Some(error.user_message());
                                    log::error!("定时执行记录失败：{error}");
                                    let _ = service.store.update_occurrence(&occurrence);
                                }
                            }
                            Ok(None) => {}
                            Err(error) => log::error!("读取定时执行队列失败：{error}"),
                        }
                        drop(service);
                        std::thread::sleep(Duration::from_millis(200));
                    }
                })?,
        );
        Ok(())
    }

    pub fn shutdown(&self) {
        self.stop.cancel();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }

    pub fn configure(&self, enabled: bool) -> Result<()> {
        self.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            let _mutation = self.mutation.lock().unwrap();
            let mut errors = Vec::new();
            for mut schedule in self.store.list()?.into_iter().filter(|item| item.enabled) {
                let expected = schedule.revision;
                schedule.revision += 1;
                schedule.enabled = false;
                self.apply(&mut schedule);
                if let Some(error) = &schedule.error {
                    errors.push(format!("{}：{error}", schedule.name));
                }
                self.store.save(&schedule, Some(expected))?;
            }
            if !errors.is_empty() {
                return Err(Error::Conflict(format!(
                    "插件已停用，部分系统任务未同步：{}",
                    errors.join("；")
                )));
            }
        }
        Ok(())
    }

    pub(super) fn supervisor(&self) -> Result<Arc<Supervisor>> {
        self.supervisor
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::Conflict("定时执行器尚未就绪".into()))
    }

    fn ensure_enabled(&self) -> Result<()> {
        if !self.enabled.load(Ordering::SeqCst)
            || !crate::AppConfig::load(&self.config_path)?.plugin_enabled(PLUGIN_ID)
        {
            return Err(Error::Conflict("请先启用定时任务插件".into()));
        }
        Ok(())
    }

    fn bind(&self, input: &ScheduleInput, previous: Option<&Schedule>) -> Result<Vec<BoundEntry>> {
        input.validate(OffsetDateTime::now_utc())?;
        let supervisor = self.supervisor()?;
        let definitions = supervisor.execution_tool_definitions();
        let mut ids = HashSet::new();
        let mut entries: Vec<BoundEntry> = Vec::new();
        for entry in &input.entries {
            let entry_id = if let Some(id) = &entry.entry_id {
                uuid(id)?;
                if !previous
                    .is_some_and(|old| old.entries.iter().any(|entry| entry.entry_id == *id))
                {
                    return Err(Error::Conflict("执行项不属于此定时任务".into()));
                }
                id.clone()
            } else {
                uuid::Uuid::new_v4().to_string()
            };
            if !ids.insert(entry_id.clone()) {
                return Err(Error::Config("执行项标识不能重复".into()));
            }
            let default_action = match entry.binding {
                Binding::CloseGame => Some(DefaultAction::CloseGame),
                Binding::CloseToolSoftware => Some(DefaultAction::CloseToolSoftware),
                Binding::Shortcut { .. } => None,
            };
            if let Some(action) = default_action {
                let anchor = entries
                    .iter()
                    .rev()
                    .find(|entry| entry.default_action.is_none())
                    .ok_or_else(|| {
                        Error::Config(format!("{}前请先添加所属软件的任务", action.title()))
                    })?;
                let mut bound = anchor.clone();
                bound.entry_id = entry_id;
                bound.default_action = Some(action);
                bound.title = action.title().into();
                bound.workflow = self.default_workflow(&anchor.workflow, action, &definitions)?;
                bound.contract_hash = bound_fingerprint(&bound.workflow, &definitions)?;
                entries.push(bound);
                continue;
            }
            let Binding::Shortcut {
                shortcut_id,
                expected_published_revision,
            } = &entry.binding
            else {
                unreachable!()
            };
            let definition = supervisor.tasks.definition(shortcut_id)?;
            if definition.published_revision != Some(*expected_published_revision) {
                return Err(Error::Conflict("绑定任务已变化，请重新选择".into()));
            }
            let mut workflow = supervisor
                .tasks
                .revision(shortcut_id, *expected_published_revision)?;
            let source_hash = digest(&workflow)?;
            if workflow.shortcut.is_none() || !workflow.validation.publishable() {
                return Err(Error::Config("只支持已发布且无需模型的快捷任务".into()));
            }
            self.prepare_workflow(&mut workflow, &definitions)?;
            let contract_hash = bound_fingerprint(&workflow, &definitions)?;
            entries.push(BoundEntry {
                default_action: None,
                entry_id,
                workflow_id: shortcut_id.clone(),
                published_revision: *expected_published_revision,
                title: workflow.name.clone(),
                source_hash,
                contract_hash,
                workflow,
            });
        }
        Ok(entries)
    }

    pub fn actions(&self, query: &str, offset: usize, limit: usize) -> Result<Value> {
        let supervisor = self.supervisor()?;
        let query = query.trim().to_lowercase();
        let mut items = Vec::new();
        for definition in supervisor.tasks.definitions()? {
            let Some(revision) = supervisor.tasks.published_revision(&definition.id)? else {
                continue;
            };
            let content = format!(
                "{} {} {}",
                definition.name,
                definition.description,
                serde_json::to_string(&revision.shortcut)?
            );
            if !query
                .split_whitespace()
                .all(|word| content.to_lowercase().contains(word))
            {
                continue;
            }
            let input = ScheduleInput {
                name: definition.name.clone(),
                trigger: Trigger::Daily {
                    local_times: vec!["00:00".into()],
                    interval_days: 1,
                    start_date: None,
                },
                entries: vec![EntryInput {
                    entry_id: None,
                    binding: Binding::Shortcut {
                        shortcut_id: definition.id.clone(),
                        expected_published_revision: revision.revision,
                    },
                }],
            };
            let issue = self
                .bind(&input, None)
                .err()
                .map(|error| error.user_message());
            let repair=(issue.is_some() && revision.nodes.iter().any(|node|matches!(node,TaskNode::Tool(call) if call.tool.as_deref()==Some("bgi.api.invoke") && (call.arguments["methodId"].as_str().is_some_and(|id|id.starts_with("cmd.")) || crate::bridge::scheduled::command_plan_mode(call).is_some())))).then(||json!({"kind":"nativeCommandPlan","capabilityState":"readCurrentContract","readContracts":["bgi.validate_command_plan","bgi.run_host_command_plan","bgi.run_game_command_plan"],"creationDoesNotExecute":true,"next":"读取当前计划接口契约，把固定命令序列和运行期对象选择条件封装成一个快捷入口，再预览定时任务。原始入口不可准入不是目标不可实现；不能把动态数据在创建时写死，也不能擅自改成交付手动入口。"}));
            items.push(json!({"id":definition.id,"title":definition.name,"content":definition.description,"publishedRevision":revision.revision,"binding":{"kind":"shortcut","shortcutId":definition.id,"expectedPublishedRevision":revision.revision},"available":issue.is_none(),"issue":issue,"repair":repair}));
        }
        let total = items.len();
        let limit = limit.clamp(1, 50);
        let values = items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(
            json!({"items":values,"total":total,"nextOffset":(offset+limit < total).then_some(offset+limit)}),
        )
    }

    pub fn preview(&self, input: &ScheduleInput) -> Result<Value> {
        let mut input = input.clone();
        for entry in &mut input.entries {
            entry.entry_id = None;
        }
        let entries = self.bind(&input, None)?;
        Ok(
            json!({"name":input.name,"trigger":input.trigger,"entries":entries.iter().map(|entry| json!({"title":entry.title,"workflowId":entry.workflow_id})).collect::<Vec<_>>(),"nextRuns":input.trigger.next(OffsetDateTime::now_utc(),5)? .iter().map(|at| at.format(&time::format_description::well_known::Rfc3339).expect("日期")).collect::<Vec<_>>() }),
        )
    }

    pub fn create(&self, input: ScheduleInput) -> Result<Schedule> {
        let _mutation = self.mutation.lock().unwrap();
        self.ensure_enabled()?;
        self.backend.ensure_available()?;
        input.validate(OffsetDateTime::now_utc())?;
        let mut normalized = input.clone();
        normalized.name = normalized.name.trim().into();
        let schedules = self.store.list()?;
        for schedule in &schedules {
            let saved = ScheduleInput {
                name: schedule.name.clone(),
                trigger: schedule.trigger.clone(),
                entries: schedule
                    .entries
                    .iter()
                    .map(|entry| EntryInput {
                        entry_id: None,
                        binding: entry.binding(),
                    })
                    .collect(),
            };
            if digest(&normalized)? == digest(&saved)? {
                if schedule.registration != "verified" {
                    return Err(Error::Conflict(format!(
                        "这份定时安排已有待核对的注册记录：{}",
                        schedule.id
                    )));
                }
                self.backend.verify(schedule)?;
                return Ok(schedule.clone());
            }
        }
        if schedules.len() >= 100 {
            return Err(Error::Config("最多保存 100 条定时任务".into()));
        }
        let entries = self.bind(&input, None)?;
        let mut schedule = Schedule {
            id: uuid::Uuid::new_v4().to_string(),
            revision: 1,
            name: input.name.trim().into(),
            trigger: input.trigger,
            entries,
            enabled: true,
            registration: "pending".into(),
            system_hash: None,
            error: None,
            created_at: crate::runtime::types::now(),
        };
        self.store.save(&schedule, None)?;
        self.apply(&mut schedule);
        self.store.save(&schedule, Some(1))?;
        if let Some(error) = &schedule.error {
            return Err(Error::Conflict(format!(
                "注册失败（定时任务 {}）：{error}",
                schedule.id
            )));
        }
        Ok(schedule)
    }

    fn apply(&self, schedule: &mut Schedule) {
        match self.backend.apply(schedule) {
            Ok(hash) => {
                schedule.system_hash = Some(hash);
                schedule.registration = "verified".into();
                schedule.error = None;
            }
            Err(error) => {
                schedule.registration = "error".into();
                schedule.error = Some(error.user_message());
            }
        }
    }

    pub fn update(&self, id: &str, expected: u64, input: ScheduleInput) -> Result<Schedule> {
        let _mutation = self.mutation.lock().unwrap();
        self.ensure_enabled()?;
        let mut schedule = self.read(id, expected)?;
        let entries = self.bind(&input, Some(&schedule))?;
        schedule.entries = entries;
        schedule.trigger = input.trigger;
        schedule.name = input.name.trim().into();
        schedule.revision += 1;
        let hash = self.backend.apply(&schedule)?;
        schedule.system_hash = Some(hash);
        schedule.error = None;
        schedule.registration = "verified".into();
        self.store.save(&schedule, Some(expected))?;
        Ok(schedule)
    }

    fn read(&self, id: &str, expected: u64) -> Result<Schedule> {
        uuid(id)?;
        let schedule = self.store.get(id)?;
        if schedule.revision != expected {
            return Err(Error::Conflict("定时任务已被修改，请重新读取".into()));
        }
        Ok(schedule)
    }

    pub fn set_enabled(&self, id: &str, expected: u64, enabled: bool) -> Result<Schedule> {
        let _mutation = self.mutation.lock().unwrap();
        self.ensure_enabled()?;
        let mut schedule = self.read(id, expected)?;
        if enabled {
            schedule.trigger.validate(OffsetDateTime::now_utc())?;
        }
        schedule.enabled = enabled;
        schedule.revision += 1;
        schedule.system_hash = Some(self.backend.apply(&schedule)?);
        schedule.registration = "verified".into();
        schedule.error = None;
        self.store.save(&schedule, Some(expected))?;
        Ok(schedule)
    }

    pub fn delete(&self, id: &str, expected: u64) -> Result<Value> {
        let _mutation = self.mutation.lock().unwrap();
        self.ensure_enabled()?;
        let schedule = self.read(id, expected)?;
        self.backend.delete(&schedule)?;
        self.store.delete(id, expected)?;
        Ok(json!({"deleted":true,"historyKept":true}))
    }

    pub fn run_now(&self, id: &str, expected: u64, key: &str) -> Result<Occurrence> {
        self.ensure_enabled()?;
        uuid(key)?;
        let schedule = self.read(id, expected)?;
        self.backend.verify(&schedule)?;
        self.store.enqueue(schedule, format!("manual:{key}"))
    }

    async fn execute(&self, occurrence: &mut Occurrence) -> Result<()> {
        self.backend.ensure_available()?;
        let supervisor = self.supervisor()?;
        for entry in occurrence.schedule.entries.clone() {
            if occurrence.results.iter().any(|result| {
                result.entry_id == entry.entry_id
                    && !matches!(result.state.as_str(), "starting" | "running")
            }) {
                continue;
            }
            if self.stop.is_cancelled()
                || self.ensure_enabled().is_err()
                || self.store.cancelled(&occurrence.id)?
            {
                break;
            }
            let result = self.execute_entry(&supervisor, occurrence, &entry).await;
            let value = match result {
                Ok(value) => value,
                Err(error) => EntryResult {
                    entry_id: entry.entry_id.clone(),
                    title: entry.title.clone(),
                    state: "failed".into(),
                    run_id: None,
                    error: Some(error.user_message()),
                    stop_state: None,
                },
            };
            occurrence
                .results
                .retain(|result| result.entry_id != entry.entry_id);
            occurrence.results.push(value);
            self.store.update_occurrence(occurrence)?;
        }
        let stopping = self.store.cancelled(&occurrence.id)?
            || self.stop.is_cancelled()
            || self.ensure_enabled().is_err();
        occurrence.state = super::history::occurrence_state(occurrence, stopping).into();
        self.store.update_occurrence(occurrence)
    }

    async fn execute_entry(
        &self,
        supervisor: &Arc<Supervisor>,
        occurrence: &mut Occurrence,
        entry: &BoundEntry,
    ) -> Result<EntryResult> {
        let requires_desktop=entry.workflow.nodes.iter().any(|node|matches!(node,TaskNode::Tool(tool) if tool.tool.as_deref()==Some("bgi.api.invoke") || tool.execution.as_ref().is_some_and(|policy|policy.effect==crate::extension::ToolEffect::GameWrite || policy.concurrency==crate::extension::ConcurrencyMode::GameExclusive)));
        if requires_desktop && !self.backend.desktop_available() {
            return Err(Error::Conflict("当前桌面不可交互".into()));
        }
        let definition = supervisor.tasks.definition(&entry.workflow_id)?;
        if definition.published_revision != Some(entry.published_revision)
            || digest(
                &supervisor
                    .tasks
                    .revision(&entry.workflow_id, entry.published_revision)?,
            )? != entry.source_hash
        {
            return Err(Error::Conflict(
                "绑定任务已经变化，请重新保存此定时任务".into(),
            ));
        }
        let origin = format!("{}:{}", occurrence.id, entry.entry_id);
        let mut current = supervisor
            .tasks
            .revision(&entry.workflow_id, entry.published_revision)?;
        self.prepare_workflow(&mut current, &supervisor.execution_tool_definitions())?;
        if let Some(action) = entry.default_action {
            current =
                self.default_workflow(&current, action, &supervisor.execution_tool_definitions())?;
        }
        if bound_fingerprint(&current, &supervisor.execution_tool_definitions())?
            != entry.contract_hash
        {
            return Err(Error::Conflict("定时动作的准入或契约已经变化".into()));
        }
        let submit = if entry.default_action.is_some() {
            Supervisor::submit_bound_followup
        } else {
            Supervisor::submit_bound_workflow
        };
        let run = submit(
            supervisor,
            entry.workflow.clone(),
            &origin,
            &entry.contract_hash,
        )?;
        occurrence
            .results
            .retain(|result| result.entry_id != entry.entry_id);
        occurrence.results.push(EntryResult {
            entry_id: entry.entry_id.clone(),
            title: entry.title.clone(),
            state: "running".into(),
            run_id: Some(run.id.clone()),
            error: None,
            stop_state: None,
        });
        self.store.update_occurrence(occurrence)?;
        let deadline = std::time::Instant::now()
            + Duration::from_secs(entry.workflow.limits.max_seconds.clamp(1, 86400) as u64 + 10);
        loop {
            self.store.heartbeat()?;
            let current = supervisor.journal.get(&run.id)?;
            if current.state.terminal() || current.state == RunState::Blocked {
                return super::history::entry_result(supervisor, entry, &current, false, false);
            }
            if self.stop.is_cancelled()
                || self.ensure_enabled().is_err()
                || self.store.cancelled(&occurrence.id)?
                || std::time::Instant::now() >= deadline
            {
                let cancelled = self.stop.is_cancelled()
                    || self.ensure_enabled().is_err()
                    || self.store.cancelled(&occurrence.id)?;
                return match supervisor.cancel(&run.id) {
                    Ok(current) => {
                        super::history::entry_result(supervisor, entry, &current, !cancelled, true)
                    }
                    Err(error) => Ok(EntryResult {
                        entry_id: entry.entry_id.clone(),
                        title: entry.title.clone(),
                        state: "unknown".into(),
                        run_id: Some(run.id),
                        error: Some(error.user_message()),
                        stop_state: Some("unknown".into()),
                    }),
                };
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn stop_occurrence(&self, id: &str) -> Result<Value> {
        uuid(id)?;
        let requested = self.store.cancel(id)?;
        Ok(json!({"cancelRequested":requested}))
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub fn run_cli_if_requested() -> Result<bool> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if !args.iter().any(|value| value == "--run-schedule") {
        return Ok(false);
    }
    if args.len() != 3 || args[1] != "--run-schedule" {
        return Err(Error::Config("定时任务入口参数无效".into()));
    }
    let id = args[2]
        .to_str()
        .ok_or_else(|| Error::Config("任务标识无效".into()))?;
    uuid(id)?;
    let path = PathBuf::from(&args[0]).canonicalize()?;
    if let Some(directory) = path.parent() {
        let _temporary = unsafe { crate::data_directory::configure_process_temp(directory)? };
    }
    let config = crate::AppConfig::load(&path)?;
    if !config.plugin_enabled(PLUGIN_ID) {
        return Err(Error::Conflict("定时任务插件已停用".into()));
    }
    let store = Store::open(&config.storage.database)?;
    let schedule = store.get(id)?;
    if !schedule.enabled || schedule.registration != "verified" {
        return Err(Error::Conflict("定时任务未开启或系统注册尚未核验".into()));
    }
    WindowsTasks::new(&path)?.verify(&schedule)?;
    let now = OffsetDateTime::now_utc();
    let due = schedule
        .trigger
        .next(now - time::Duration::seconds(90), 1)?
        .into_iter()
        .find(|at| *at <= now)
        .ok_or_else(|| Error::Conflict("未到执行时刻或本次执行已经错过".into()))?;
    let occurrence = store.enqueue(
        schedule,
        format!(
            "timer:{id}:{}:{}",
            store.get(id)?.revision,
            due.unix_timestamp()
        ),
    )?;
    let controller = match crate::app::AppController::load_bound_only(&path) {
        Ok(value) => Some(value),
        Err(Error::Conflict(message)) if message == "该数据目录已有运行中的 Sleepy Doll" => {
            None
        }
        Err(error) => return Err(error),
    };
    let deadline = std::time::Instant::now()
        + Duration::from_secs(
            occurrence
                .schedule
                .entries
                .iter()
                .map(|entry| entry.workflow.limits.max_seconds.clamp(1, 86400) as u64 + 30)
                .sum::<u64>()
                + 60,
        );
    let final_state = loop {
        let state = store.occurrence(&occurrence.id)?;
        if !matches!(state.state.as_str(), "queued" | "running" | "cancelling") {
            break state;
        }
        if std::time::Instant::now() > deadline {
            if let Some(controller) = &controller {
                controller.shutdown();
            }
            return Err(Error::Timeout("等待定时执行器响应超时".into()));
        }
        if state.state == "queued"
            && crate::runtime::types::unix_now() - store.last_heartbeat()? > 10
        {
            store.cancel(&occurrence.id)?;
            if let Some(controller) = &controller {
                controller.shutdown();
            }
            return Err(Error::Conflict(
                "当前数据目录没有可用的管理员定时执行实例".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    if let Some(controller) = controller {
        controller.shutdown();
    }
    if matches!(
        final_state.state.as_str(),
        "failed" | "unknown" | "timeout" | "partial"
    ) {
        return Err(Error::Tool(final_state.error.unwrap_or_else(|| {
            "本轮定时任务存在执行失败或未确认项，请查看运行记录".into()
        })));
    }
    Ok(true)
}
