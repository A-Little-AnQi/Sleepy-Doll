use super::*;
use crate::extension::{
    CompensationMode, FunctionTool, ModelUsage, ToolEffect, ToolExecution, ToolRegistry,
    UnattendedPolicy, VerificationMode,
};
use crate::runtime::operation::{
    shortcuts::{ShortcutBinding, ShortcutCall},
    task::{self, WorkflowDefinition},
};

#[derive(Default)]
struct FakeTasks {
    values: Mutex<std::collections::HashMap<String, String>>,
}
impl TaskBackend for FakeTasks {
    fn ensure_available(&self) -> Result<()> {
        Ok(())
    }
    fn desktop_available(&self) -> bool {
        true
    }
    fn apply(&self, value: &Schedule) -> Result<String> {
        let mut values = self.values.lock().unwrap();
        if let Some(previous) = values.get(&value.id)
            && value.system_hash.as_ref() != Some(previous)
        {
            return Err(Error::Conflict("外部修改".into()));
        }
        let hash = digest(&(
            value.id.clone(),
            value.revision,
            value.enabled,
            &value.trigger,
        ))?;
        values.insert(value.id.clone(), hash.clone());
        Ok(hash)
    }
    fn delete(&self, value: &Schedule) -> Result<()> {
        self.verify(value)?;
        self.values.lock().unwrap().remove(&value.id);
        Ok(())
    }
    fn verify(&self, value: &Schedule) -> Result<()> {
        if self.values.lock().unwrap().get(&value.id) == value.system_hash.as_ref() {
            Ok(())
        } else {
            Err(Error::Conflict("外部修改".into()))
        }
    }
}

struct Fixture {
    service: Arc<Scheduler>,
    supervisor: Arc<Supervisor>,
    backend: Arc<FakeTasks>,
    calls: Arc<Mutex<Vec<String>>>,
    a: String,
    b: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.service.shutdown();
        self.supervisor.shutdown_bound_only();
    }
}

fn fixture() -> Fixture {
    let root = crate::test_root("scheduler");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(root.join("catalog")).unwrap();
    let path = root.join("config.json");
    let config_value = json!({"version":5,"activeModel":"","models":[],"agent":{"systemPrompt":""},"plugins":{"directories":[],"enabled":[PLUGIN_ID,"fixture"]},"bridge":{"enabled":false,"baseUrl":"http://127.0.0.1:1","timeoutMs":1000},"storage":{"database":root.join("agent.db")},"runtime":{"permissionMode":"fullAccess","catalogDirectory":root.join("catalog")}});
    std::fs::write(&path, serde_json::to_vec(&config_value).unwrap()).unwrap();
    let config = crate::AppConfig::load(&path).unwrap();
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = calls.clone();
    let mut tools = ToolRegistry::default();
    tools.register(FunctionTool::new("fixture.run","执行测试动作",json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}),"plugin:fixture:local",move|args:&Value|{
        let name=args["name"].as_str().unwrap().to_owned();
        let mut calls=seen.lock().unwrap();calls.push(name.clone());
        if name=="A" && calls.iter().filter(|name|name.as_str()=="A").count()==1 {return Err(Error::Tool("第一项的预期错误".into()));}
        Ok(json!({"verification":{"status":"succeeded"}}))
    }).with_execution(ToolExecution {effect:ToolEffect::LocalWrite,model_usage:ModelUsage::None,unattended:UnattendedPolicy::Allowed,cancellation:crate::extension::CancellationMode::Reliable,verification:VerificationMode::None,compensation:CompensationMode::None,..ToolExecution::default()})).unwrap();
    for (name, label) in [
        ("fixture.close_game", "close-game"),
        ("fixture.close_tool", "close-tool"),
    ] {
        let calls = calls.clone();
        tools
            .register(
                FunctionTool::new(
                    name,
                    label,
                    json!({"type":"object","additionalProperties":false}),
                    "plugin:fixture:local",
                    move |_: &Value| {
                        calls.lock().unwrap().push(label.into());
                        Ok(json!({"verification":{"status":"succeeded"}}))
                    },
                )
                .with_execution(ToolExecution {
                    effect: ToolEffect::LocalWrite,
                    model_usage: ModelUsage::None,
                    unattended: UnattendedPolicy::Allowed,
                    cancellation: crate::extension::CancellationMode::Reliable,
                    verification: VerificationMode::None,
                    compensation: CompensationMode::None,
                    ..ToolExecution::default()
                }),
            )
            .unwrap();
    }
    let operation_store = Arc::new(
        crate::runtime::operation::operations::OperationStore::open(&config.storage.database)
            .unwrap(),
    );
    let artifacts = Arc::new(
        crate::runtime::store::artifacts::ArtifactStore::new(root.join("attachments"), 1024 * 1024)
            .unwrap(),
    );
    let operations = Arc::new(crate::runtime::operation::operations::OperationEngine::new(
        operation_store,
        artifacts,
    ));
    let supervisor = Supervisor::new_bound_only(
        config.clone(),
        Arc::new(crate::extension::skills::SkillRegistry::default()),
        Arc::new(tools),
        operations,
        vec![],
    )
    .unwrap();
    let backend = Arc::new(FakeTasks::default());
    let service = Arc::new(Scheduler {
        store: Store::open(&config.storage.database).unwrap(),
        backend: backend.clone(),
        config_path: path,
        supervisor: OnceLock::new(),
        enabled: AtomicBool::new(true),
        stop: CancellationToken::new(),
        worker: Mutex::new(None),
        mutation: Mutex::new(()),
        allowed_calls: Mutex::new(HashSet::new()),
        default_actions: Mutex::new(Default::default()),
    });
    let a = save_shortcut(&supervisor, "A", "内容命中关键字");
    let b = save_shortcut(&supervisor, "B", "");
    service
        .register_action("fixture.run", "plugin:fixture:local")
        .unwrap();
    service.attach(&supervisor).unwrap();
    Fixture {
        service,
        supervisor,
        backend,
        calls,
        a,
        b,
    }
}

#[test]
fn default_closers_follow_nearest_task_and_continue_after_error() {
    let fixture = fixture();
    fixture
        .service
        .register_default_action(
            "fixture",
            DefaultAction::CloseGame,
            "fixture.close_game",
            json!({}),
        )
        .unwrap();
    fixture
        .service
        .register_default_action(
            "fixture",
            DefaultAction::CloseToolSoftware,
            "fixture.close_tool",
            json!({}),
        )
        .unwrap();
    let original = input(&fixture);
    let mut value = original.clone();
    value.entries = vec![
        original.entries[0].clone(),
        EntryInput {
            entry_id: None,
            binding: Binding::CloseGame,
        },
        original.entries[1].clone(),
        EntryInput {
            entry_id: None,
            binding: Binding::CloseToolSoftware,
        },
    ];
    let schedule = fixture.service.create(value).unwrap();
    assert_eq!(schedule.entries[1].workflow_id, fixture.a);
    assert_eq!(schedule.entries[3].workflow_id, fixture.b);
    assert_eq!(
        schedule.view()["entries"][1]["binding"],
        json!({"kind":"closeGame"})
    );
    let occurrence = fixture
        .service
        .run_now(&schedule.id, 1, &uuid::Uuid::new_v4().to_string())
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let state = fixture.service.store.occurrence(&occurrence.id).unwrap();
        if !matches!(state.state.as_str(), "queued" | "running") {
            assert_eq!(state.results.len(), 4);
            assert_eq!(state.results[0].state, "failed");
            assert_eq!(state.results[1].state, "succeeded");
            assert_eq!(state.results[3].state, "succeeded");
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{state:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        *fixture.calls.lock().unwrap(),
        vec!["A", "close-game", "B", "close-tool"]
    );
    let mut reordered = original;
    reordered.entries = vec![
        reordered.entries[1].clone(),
        EntryInput {
            entry_id: Some(schedule.entries[1].entry_id.clone()),
            binding: Binding::CloseGame,
        },
    ];
    let updated = fixture.service.update(&schedule.id, 1, reordered).unwrap();
    assert_eq!(updated.entries[1].workflow_id, fixture.b);
    assert_eq!(
        fixture
            .supervisor
            .tasks
            .published_revision(&fixture.a)
            .unwrap()
            .unwrap()
            .nodes
            .len(),
        1
    );
}

#[test]
fn default_closers_require_a_supported_anchor_and_reject_target_parameters() {
    let fixture = fixture();
    let mut value = input(&fixture);
    value.entries = vec![EntryInput {
        entry_id: None,
        binding: Binding::CloseGame,
    }];
    assert!(
        fixture
            .service
            .preview(&value)
            .unwrap_err()
            .user_message()
            .contains("前请先")
    );
    value.entries.insert(0, input(&fixture).entries[0].clone());
    assert!(
        fixture
            .service
            .preview(&value)
            .unwrap_err()
            .user_message()
            .contains("尚未支持")
    );
    let mut payload = serde_json::to_value(&value).unwrap();
    payload["entries"][1]["binding"]["processId"] = json!(123);
    assert!(
        super::super::tools::dispatch(&fixture.service, "scheduler.preview", &payload).is_err()
    );
    let defaults = super::super::tools::dispatch(
        &fixture.service,
        "scheduler.actions.list",
        &json!({"category":"defaults"}),
    )
    .unwrap();
    assert_eq!(defaults["items"].as_array().unwrap().len(), 2);
    assert!(fixture.backend.values.lock().unwrap().is_empty());
    assert!(fixture.calls.lock().unwrap().is_empty());
}

fn save_shortcut(supervisor: &Supervisor, name: &str, description: &str) -> String {
    let binding = ShortcutBinding {
        action: Some(ShortcutCall {
            tool: "fixture.run".into(),
            arguments: json!({"name":name}),
        }),
        steps: vec![],
        prepare: vec![],
        target_name: name.into(),
        application_name: "Fixture".into(),
    };
    save_binding(supervisor, name, description, binding)
}

fn save_binding(
    supervisor: &Supervisor,
    name: &str,
    description: &str,
    binding: ShortcutBinding,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = crate::runtime::types::now();
    let nodes =
        crate::runtime::operation::shortcuts::nodes(&binding, &catalog(supervisor)).unwrap();
    let mut revision =
        task::compile(&id, 1, name, description, nodes, None, &catalog(supervisor)).unwrap();
    revision.shortcut = Some(binding);
    assert!(
        revision.validation.publishable(),
        "{:?}",
        revision.validation
    );
    supervisor
        .tasks
        .create_definition(&WorkflowDefinition {
            id: id.clone(),
            name: name.into(),
            description: description.into(),
            source_conversation_id: None,
            source_message_id: None,
            source_title_snapshot: "".into(),
            published_revision: Some(1),
            draft_revision: None,
            pinned: false,
            last_run_id: None,
            created_at: now.clone(),
            updated_at: now,
        })
        .unwrap();
    supervisor.tasks.save_revision(&revision).unwrap();
    id
}

fn input(fixture: &Fixture) -> ScheduleInput {
    ScheduleInput {
        name: "重复顺序".into(),
        trigger: Trigger::Daily {
            local_times: vec!["20:00".into()],
            interval_days: 1,
            start_date: None,
        },
        entries: [&fixture.a, &fixture.b, &fixture.a]
            .iter()
            .map(|id| EntryInput {
                entry_id: None,
                binding: Binding::Shortcut {
                    shortcut_id: (*id).clone(),
                    expected_published_revision: 1,
                },
            })
            .collect(),
    }
}

#[test]
fn shortcut_wait_runs_in_order_and_cancellation_skips_the_following_action() {
    let fixture = fixture();
    let binding:ShortcutBinding=serde_json::from_value(json!({"applicationName":"Fixture","targetName":"顺序执行","steps":[{"title":"前项","action":{"tool":"fixture.run","arguments":{"name":"before-wait"}}},{"title":"等待十秒","waitSeconds":10},{"title":"后项","action":{"tool":"fixture.run","arguments":{"name":"after-wait"}}}]})).unwrap();
    let id = save_binding(&fixture.supervisor, "先执行再等待", "", binding);
    let mut value = input(&fixture);
    value.entries = vec![EntryInput {
        entry_id: None,
        binding: Binding::Shortcut {
            shortcut_id: id,
            expected_published_revision: 1,
        },
    }];
    let schedule = fixture.service.create(value).unwrap();
    for cancel in [false, true] {
        fixture.calls.lock().unwrap().clear();
        let occurrence = fixture
            .service
            .run_now(&schedule.id, 1, &uuid::Uuid::new_v4().to_string())
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while fixture.calls.lock().unwrap().is_empty() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        let first_seen = std::time::Instant::now();
        if cancel {
            std::thread::sleep(Duration::from_millis(150));
            fixture.service.stop_occurrence(&occurrence.id).unwrap();
        }
        loop {
            let item = fixture.service.store.occurrence(&occurrence.id).unwrap();
            if !matches!(item.state.as_str(), "queued" | "running" | "cancelling") {
                assert_eq!(
                    item.state,
                    if cancel { "cancelled" } else { "succeeded" },
                    "{item:?}"
                );
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{item:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
        if cancel {
            assert_eq!(*fixture.calls.lock().unwrap(), vec!["before-wait"]);
            assert!(first_seen.elapsed() < Duration::from_secs(5));
        } else {
            assert_eq!(
                *fixture.calls.lock().unwrap(),
                vec!["before-wait", "after-wait"]
            );
            assert!(first_seen.elapsed() >= Duration::from_secs(9));
        }
    }
}

#[test]
fn deleted_shortcut_cannot_create_a_queued_run_from_a_stale_snapshot() {
    let fixture = fixture();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    let bound = &schedule.entries[0];
    let before = fixture.supervisor.journal.list().unwrap().len();
    fixture.supervisor.delete_workflow(&fixture.a).unwrap();
    assert!(
        fixture
            .supervisor
            .submit_workflow(&fixture.a, Some(1), "late-manual", None)
            .is_err()
    );
    assert!(
        fixture
            .supervisor
            .submit_bound_workflow(bound.workflow.clone(), "late-bound", &bound.contract_hash)
            .is_err()
    );
    assert_eq!(
        fixture.supervisor.journal.list().unwrap().len(),
        before,
        "不能留下稍后执行的孤立队列项"
    );
    assert!(fixture.calls.lock().unwrap().is_empty());
    assert!(fixture.supervisor.tasks.definition(&fixture.b).is_ok());
}

#[test]
fn repeated_tasks_really_execute_in_order_after_error() {
    let fixture = fixture();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    assert_eq!(schedule.entries.len(), 3);
    assert_ne!(schedule.entries[0].entry_id, schedule.entries[2].entry_id);
    let key = uuid::Uuid::new_v4().to_string();
    let queued = fixture.service.run_now(&schedule.id, 1, &key).unwrap();
    assert_eq!(
        fixture.service.run_now(&schedule.id, 1, &key).unwrap().id,
        queued.id
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    let finished = loop {
        let state = fixture.service.store.occurrence(&queued.id).unwrap();
        if !matches!(state.state.as_str(), "queued" | "running") {
            break state;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{:?}, calls {:?}",
            state,
            fixture.calls.lock().unwrap()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        &*fixture.calls.lock().unwrap(),
        &["A", "B", "A"],
        "{:?}",
        finished.results
    );
    assert_eq!(finished.results.len(), 3);
    assert_eq!(
        finished
            .results
            .iter()
            .map(|result| result.state.as_str())
            .collect::<Vec<_>>(),
        vec!["failed", "succeeded", "succeeded"]
    );
    assert_eq!(finished.state, "partial");
    let ids = finished
        .results
        .iter()
        .filter_map(|result| result.run_id.as_ref())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 3);
    for id in ids {
        let run = fixture.supervisor.journal.get(id).unwrap();
        assert_eq!(run.decisions, 0);
        assert_eq!(run.input_tokens, 0);
    }
}

#[test]
fn task_library_excludes_ineligible_bindings_before_search_and_pagination() {
    let fixture = fixture();
    let unavailable = save_binding(
        &fixture.supervisor,
        "不可定时入口",
        "只匹配不可用项",
        ShortcutBinding {
            action: Some(ShortcutCall {
                tool: "fixture.close_tool".into(),
                arguments: json!({}),
            }),
            steps: vec![],
            prepare: vec![],
            target_name: "未登记的动作".into(),
            application_name: "Fixture".into(),
        },
    );
    let first = fixture.service.actions("", 0, 1).unwrap();
    let second = fixture.service.actions("", 1, 1).unwrap();
    assert_eq!(first["total"], 2);
    assert_eq!(first["nextOffset"], 1);
    assert_eq!(second["nextOffset"], Value::Null);
    for page in [first, second] {
        assert_eq!(page["items"].as_array().unwrap().len(), 1);
        assert_eq!(page["items"][0]["available"], true);
        assert_ne!(page["items"][0]["id"], unavailable);
    }
    assert_eq!(
        fixture.service.actions("只匹配不可用项", 0, 10).unwrap()["total"],
        0
    );
    let mut direct = input(&fixture);
    direct.entries = vec![EntryInput {
        entry_id: None,
        binding: Binding::Shortcut {
            shortcut_id: unavailable,
            expected_published_revision: 1,
        },
    }];
    assert!(fixture.service.preview(&direct).is_err());
    assert!(fixture.service.create(direct).is_err());
    assert!(fixture.backend.values.lock().unwrap().is_empty());
}

#[test]
fn content_search_and_reorder_do_not_consume_originals() {
    let fixture = fixture();
    let found = fixture.service.actions("内容命中", 0, 10).unwrap();
    assert_eq!(found["total"], 1);
    assert_eq!(found["items"][0]["id"], fixture.a);
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    let mut updated = input(&fixture);
    updated.entries.swap(0, 1);
    for (entry, id) in updated.entries.iter_mut().zip([
        &schedule.entries[1].entry_id,
        &schedule.entries[0].entry_id,
        &schedule.entries[2].entry_id,
    ]) {
        entry.entry_id = Some(id.clone());
    }
    let saved = fixture.service.update(&schedule.id, 1, updated).unwrap();
    assert_eq!(saved.entries[0].workflow_id, fixture.b);
    assert!(
        fixture
            .service
            .update(&schedule.id, 1, input(&fixture))
            .is_err()
    );
    assert_eq!(fixture.service.actions("", 0, 20).unwrap()["total"], 2);
    fixture.service.delete(&schedule.id, 2).unwrap();
    assert!(fixture.supervisor.tasks.definition(&fixture.a).is_ok());
}

#[test]
fn external_task_changes_are_never_overwritten() {
    let fixture = fixture();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    fixture
        .backend
        .values
        .lock()
        .unwrap()
        .insert(schedule.id.clone(), "externally-changed".into());
    assert!(fixture.service.set_enabled(&schedule.id, 1, false).is_err());
    assert!(fixture.service.delete(&schedule.id, 1).is_err());
    assert_eq!(fixture.service.store.get(&schedule.id).unwrap().revision, 1);
}

#[test]
fn schema_rejects_raw_commands_and_preserves_duplicate_bindings() {
    let fixture = fixture();
    let schema = super::super::tools::schedule_schema();
    let mut value = serde_json::to_value(input(&fixture)).unwrap();
    assert!(crate::extension::validate(&value, &schema, "$").is_empty());
    value["command"] = json!("powershell.exe");
    assert!(!crate::extension::validate(&value, &schema, "$").is_empty());
    assert!(serde_json::from_value::<ScheduleInput>(value).is_err());
    let past = Trigger::Once {
        at: "2020-01-01T00:00:00Z".into(),
    };
    assert!(past.validate(OffsetDateTime::now_utc()).is_err());
    assert!(super::super::model::parse_time("24:00").is_err());
    assert!(super::super::model::parse_time("12:60").is_err());
}

#[test]
fn monthly_dates_and_last_day_include_leap_year_without_duplicates() {
    let monthly = Trigger::Monthly {
        days_of_month: vec![31],
        last_day_of_month: false,
        local_times: vec!["20:00".into()],
    };
    let date = |year, month, day| {
        time::Date::from_calendar_date(year, time::Month::try_from(month).unwrap(), day).unwrap()
    };
    assert!(monthly.matches_date(date(2026, 1, 31)));
    assert!(!monthly.matches_date(date(2026, 4, 30)));
    let last = Trigger::Monthly {
        days_of_month: vec![31],
        last_day_of_month: true,
        local_times: vec!["20:00".into()],
    };
    assert!(last.matches_date(date(2028, 2, 29)));
    assert!(!last.matches_date(date(2028, 2, 28)));
    assert!(last.matches_date(date(2026, 2, 28)));
    let after = super::super::system::from_local(date(2026, 1, 30).with_hms(0, 0, 0).unwrap())
        .unwrap()
        .unwrap();
    let runs = last.next(after, 3).unwrap();
    assert_eq!(runs.len(), 3);
    assert!(runs.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        super::super::system::to_local(runs[1]).unwrap().date(),
        date(2026, 2, 28)
    );
}

#[test]
fn daily_intervals_keep_the_calendar_anchor_across_months_and_years() {
    let date =
        |y, m, d| time::Date::from_calendar_date(y, time::Month::try_from(m).unwrap(), d).unwrap();
    let trigger = |start: &str, days| Trigger::Daily {
        local_times: vec!["04:30".into()],
        interval_days: days,
        start_date: Some(start.into()),
    };
    let every_two = trigger("2026-11-09", 2);
    assert!(!every_two.matches_date(date(2026, 11, 8)));
    assert!(every_two.matches_date(date(2026, 11, 9)));
    assert!(!every_two.matches_date(date(2026, 11, 10)));
    assert!(every_two.matches_date(date(2026, 11, 11)));
    let boundary = trigger("2026-12-30", 2);
    assert!(boundary.matches_date(date(2027, 1, 1)));
    assert!(!boundary.matches_date(date(2027, 1, 2)));
    let leap = trigger("2028-02-28", 2);
    assert!(!leap.matches_date(date(2028, 2, 29)));
    assert!(leap.matches_date(date(2028, 3, 1)));
    let local = date(2026, 11, 9).with_hms(4, 31, 0).unwrap();
    let after = super::super::system::from_local(local).unwrap().unwrap();
    let next = every_two.next(after, 3).unwrap();
    let dates: Vec<_> = next
        .into_iter()
        .map(|at| super::super::system::to_local(at).unwrap().date())
        .collect();
    assert_eq!(
        dates,
        vec![date(2026, 11, 11), date(2026, 11, 13), date(2026, 11, 15)]
    );
    let far = trigger("2040-01-01", 365);
    let next = far.next(after, 16).unwrap();
    assert_eq!(next.len(), 16);
    for pair in next.windows(2) {
        let a = super::super::system::to_local(pair[0]).unwrap().date();
        let b = super::super::system::to_local(pair[1]).unwrap().date();
        assert_eq!((b - a).whole_days(), 365);
    }
}

#[test]
fn daily_interval_schema_legacy_roundtrip_and_native_xml_agree() {
    let old = json!({"kind":"daily","localTimes":["04:30"]});
    let legacy: Trigger = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(legacy).unwrap(), old);
    let base = json!({"name":"间隔执行","entries":[{"binding":{"kind":"shortcut","shortcutId":"existing","expectedPublishedRevision":1}}],"trigger":{"kind":"daily","intervalDays":2,"startDate":"2026-11-09","localTimes":["04:30"]}});
    let schema = super::super::tools::schedule_schema();
    assert!(crate::extension::validate(&base, &schema, "$").is_empty());
    let input: ScheduleInput = serde_json::from_value(base.clone()).unwrap();
    input.trigger.validate(OffsetDateTime::now_utc()).unwrap();
    for invalid in [
        json!({"kind":"daily","intervalDays":0,"localTimes":["04:30"]}),
        json!({"kind":"daily","intervalDays":366,"startDate":"2026-11-09","localTimes":["04:30"]}),
        json!({"kind":"daily","intervalDays":2,"localTimes":["04:30"]}),
        json!({"kind":"daily","intervalDays":2,"startDate":"2026-02-30","localTimes":["04:30"]}),
    ] {
        let value: Trigger = serde_json::from_value(invalid).unwrap();
        assert!(value.validate(OffsetDateTime::now_utc()).is_err());
    }
    let schedule = Schedule {
        id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        name: "按天间隔".into(),
        trigger: input.trigger,
        entries: vec![],
        enabled: true,
        registration: "pending".into(),
        system_hash: None,
        error: None,
        created_at: "2026-11-08T00:00:00Z".into(),
    };
    let backend = super::super::system::WindowsTasks {
        config: std::path::PathBuf::from(r"C:\SleepyDoll\config.json"),
        executable: std::env::current_exe().unwrap(),
    };
    let xml = backend.xml(&schedule, "S-1-5-18").unwrap();
    assert!(xml.contains("<DaysInterval>2</DaysInterval>"));
    assert!(xml.contains("<StartBoundary>2026-11-09T04:30:00</StartBoundary>"));
    if let Some(path) = std::env::var_os("SLEEPY_DOLL_SCHEDULER_XML_CAPTURE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, xml).unwrap();
    }
}

#[test]
fn daily_interval_anchor_is_preserved_by_rename_and_disable_enable() {
    let fixture = fixture();
    let trigger = Trigger::Daily {
        local_times: vec!["04:30".into()],
        interval_days: 2,
        start_date: Some("2040-01-01".into()),
    };
    let mut value = input(&fixture);
    value.trigger = trigger.clone();
    let schedule = fixture.service.create(value.clone()).unwrap();
    value.name = "只改名称".into();
    let renamed = fixture.service.update(&schedule.id, 1, value).unwrap();
    assert_eq!(renamed.trigger, trigger);
    let disabled = fixture
        .service
        .set_enabled(&schedule.id, renamed.revision, false)
        .unwrap();
    let enabled = fixture
        .service
        .set_enabled(&schedule.id, disabled.revision, true)
        .unwrap();
    assert_eq!(enabled.trigger, trigger);
    assert_eq!(
        fixture.service.store.get(&schedule.id).unwrap().trigger,
        trigger
    );
}

#[test]
fn schedule_names_allow_punctuation_but_reject_invalid_xml_characters() {
    let mut value = ScheduleInput {
        name: "计划 / : * ? \" < > | &".into(),
        trigger: Trigger::Daily {
            local_times: vec!["04:30".into()],
            interval_days: 1,
            start_date: None,
        },
        entries: vec![EntryInput {
            entry_id: None,
            binding: Binding::CloseGame,
        }],
    };
    value.validate(OffsetDateTime::now_utc()).unwrap();
    for ch in ['\u{0}', '\u{1}', '\u{b}', '\u{fffe}', '\u{ffff}'] {
        value.name = format!("任务{ch}");
        assert!(value.validate(OffsetDateTime::now_utc()).is_err());
    }
}

#[test]
fn generated_xml_has_fixed_admin_principal_and_escaped_paths() {
    let fixture = fixture();
    let mut schedule = fixture.service.create(input(&fixture)).unwrap();
    schedule.name = "A<&B".into();
    let backend = WindowsTasks {
        config: PathBuf::from("E:\\config folder\\config.json"),
        executable: PathBuf::from("E:\\Sleepy Doll\\sleepy-doll.exe"),
    };
    let xml = backend.xml(&schedule, "S-1-5-21-1").unwrap();
    assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
    assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(xml.contains("A&lt;&amp;B"));
    assert!(xml.contains("--run-schedule"));
    assert!(!xml.contains("powershell"));
}

#[test]
fn plugin_loads_real_mcp_contracts_and_writes_through_them() {
    let fixture = fixture();
    let mut manager = crate::extension::plugins::PluginManager::default();
    manager.provide_mcp(PLUGIN_ID, fixture.service.server().unwrap());
    let mut registry = ToolRegistry::default();
    let mut skills = crate::extension::skills::SkillRegistry::default();
    manager
        .load(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")],
            &[PLUGIN_ID.into()],
            &["bgi".into()],
            &mut registry,
            &mut skills,
        )
        .unwrap();
    let definitions = registry.definitions();
    let skill = skills.get("schedule-task").unwrap();
    assert_eq!(skill.source.plugin(), Some(PLUGIN_ID));
    assert!(skill.always_load);
    assert!(!skill.user_visible);
    for reference in ["references/conversation.md", "references/api.md"] {
        assert!(
            !skills
                .read_reference("schedule-task", reference)
                .unwrap()
                .is_empty()
        );
    }
    let clock = registry.call("scheduler.clock", &json!({}));
    assert_eq!(clock["ok"], true, "{clock}");
    assert!(clock["value"]["localNow"].as_str().is_some());
    assert!(fixture.service.store.list().unwrap().is_empty());
    assert!(fixture.backend.values.lock().unwrap().is_empty());
    assert_eq!(
        definitions
            .iter()
            .find(|tool| tool.name == "scheduler.clock")
            .unwrap()
            .execution
            .effect,
        ToolEffect::ReadOnly
    );
    assert_eq!(
        definitions
            .iter()
            .find(|tool| tool.name == "scheduler.create")
            .unwrap()
            .execution
            .effect,
        ToolEffect::ExternalWrite
    );
    assert_eq!(
        definitions
            .iter()
            .find(|tool| tool.name == "scheduler.actions.list")
            .unwrap()
            .execution
            .effect,
        ToolEffect::ReadOnly
    );
    let value = registry.call(
        "scheduler.create",
        &serde_json::to_value(input(&fixture)).unwrap(),
    );
    assert_eq!(value["ok"], true, "{value}");
    assert_eq!(value["value"]["entries"].as_array().unwrap().len(), 3);
    assert!(value["value"]["entries"][0].get("workflow").is_none());
    let found = registry.call("scheduler.actions.list", &json!({"query":"内容命中"}));
    assert_eq!(found["ok"], true, "{found}");
    assert_eq!(found["value"]["total"], 1);
    let mut invalid = serde_json::to_value(input(&fixture)).unwrap();
    invalid["executable"] = json!("cmd.exe");
    assert_eq!(registry.call("scheduler.create", &invalid)["ok"], false);
    let mut disabled_tools = ToolRegistry::default();
    let mut disabled_skills = crate::extension::skills::SkillRegistry::default();
    manager
        .load(
            &[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")],
            &[],
            &["bgi".into()],
            &mut disabled_tools,
            &mut disabled_skills,
        )
        .unwrap();
    assert!(
        !disabled_tools
            .definitions()
            .iter()
            .any(|tool| tool.name.starts_with("scheduler."))
    );
    assert!(disabled_skills.get("schedule-task").is_none());
}

#[test]
fn scheduling_clock_resolves_local_dates_and_rejects_invalid_dates() {
    let value = super::super::clock::read(None).unwrap();
    let now = parse_at(value["now"].as_str().unwrap()).unwrap();
    let local = parse_at(value["localNow"].as_str().unwrap()).unwrap();
    assert_eq!(now.unix_timestamp(), local.unix_timestamp());
    assert_eq!(value["weekday"], local.weekday().number_from_monday());
    assert_eq!(value["localDate"], local.date().to_string());
    let date = local.date() + time::Duration::days(2);
    let requested = date.with_hms(18, 30, 0).unwrap();
    let value = super::super::clock::read(Some(&format!("{date}T18:30"))).unwrap();
    let resolved = parse_at(value["resolved"]["at"].as_str().unwrap()).unwrap();
    assert_eq!(super::super::system::to_local(resolved).unwrap(), requested);
    assert_eq!(value["resolved"]["future"], true);
    let past = super::super::clock::read(Some("2000-01-01T12:00")).unwrap();
    assert_eq!(past["resolved"]["future"], false);
    for text in [
        "2027-02-30T20:00",
        "2027-01-01T25:00",
        "明晚八点",
        "2027-01-01T20:00+08:00",
    ] {
        assert!(super::super::clock::read(Some(text)).is_err(), "{text}");
    }
}

#[test]
fn creation_retry_and_manual_retry_do_not_duplicate_work() {
    let fixture = fixture();
    let first = fixture.service.create(input(&fixture)).unwrap();
    let second = fixture.service.create(input(&fixture)).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(fixture.service.store.list().unwrap().len(), 1);
    fixture.service.shutdown();
    let queued = fixture
        .service
        .run_now(&first.id, 1, &uuid::Uuid::new_v4().to_string())
        .unwrap();
    fixture.service.stop_occurrence(&queued.id).unwrap();
    assert_eq!(
        fixture.service.store.occurrence(&queued.id).unwrap().state,
        "cancelled"
    );
    assert!(fixture.calls.lock().unwrap().is_empty());
}

#[test]
fn changing_tool_contract_invalidates_bound_scope() {
    let fixture = fixture();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    let entry = &schedule.entries[0];
    let mut definitions = fixture.supervisor.execution_tool_definitions();
    definitions
        .iter_mut()
        .find(|tool| tool.name == "fixture.run")
        .unwrap()
        .execution
        .risk = crate::extension::RiskLevel::High;
    assert_ne!(
        bound_fingerprint(&entry.workflow, &definitions).unwrap(),
        entry.contract_hash
    );
    let mut workflow = entry.workflow.clone();
    if let TaskNode::Tool(node) = &mut workflow.nodes[0] {
        node.arguments["name"] = json!("outside-scope");
    }
    assert!(
        fixture
            .supervisor
            .submit_bound_workflow(workflow, "tampered", &entry.contract_hash)
            .is_err()
    );
    assert!(fixture.calls.lock().unwrap().is_empty());
}

#[test]
#[cfg(target_os = "windows")]
fn native_registration_rejects_missing_elevation_before_com_writes() {
    if super::super::system::identity().unwrap().1 {
        return;
    }
    let fixture = fixture();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    let backend = WindowsTasks::new(&fixture.service.config_path).unwrap();
    assert!(
        backend
            .apply(&schedule)
            .unwrap_err()
            .user_message()
            .contains("管理员")
    );
}

#[test]
fn headless_scheduler_does_not_run_or_cancel_queued_chats() {
    let fixture = fixture();
    let chat = fixture
        .supervisor
        .journal
        .create(
            "留给用户继续的普通对话",
            "pending-chat",
            "pending-chat-key",
            300,
            None,
        )
        .unwrap();
    let schedule = fixture.service.create(input(&fixture)).unwrap();
    let occurrence = fixture
        .service
        .run_now(&schedule.id, 1, &uuid::Uuid::new_v4().to_string())
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let value = fixture.service.store.occurrence(&occurrence.id).unwrap();
        if !matches!(value.state.as_str(), "queued" | "running") {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        fixture.supervisor.journal.get(&chat.id).unwrap().state,
        RunState::Queued
    );
    fixture.service.shutdown();
    fixture.supervisor.shutdown_bound_only();
    assert_eq!(
        fixture.supervisor.journal.get(&chat.id).unwrap().state,
        RunState::Queued
    );
}

#[test]
fn plugin_manifest_flags_alone_do_not_authorize_scheduled_actions() {
    let fixture = fixture();
    fixture.service.allowed_calls.lock().unwrap().clear();
    assert!(
        fixture
            .service
            .create(input(&fixture))
            .unwrap_err()
            .user_message()
            .contains("尚未登记")
    );
    assert!(fixture.calls.lock().unwrap().is_empty());
}

#[test]
fn actual_bgi_group_wrapper_is_bounded_and_waits_without_changing_shortcut() {
    let fixture = fixture();
    let config = crate::AppConfig::load(&fixture.service.config_path).unwrap();
    let mut tools = ToolRegistry::default();
    crate::bridge::register_tools(
        &mut tools,
        Arc::new(crate::bridge::BgiClient::new(config.bridge)),
    )
    .unwrap();
    assert_eq!(
        tools.call("bgi.host.close", &json!({"processId":std::process::id()}))["ok"],
        false
    );
    fixture.supervisor.update_extensions(
        Arc::new(crate::extension::skills::SkillRegistry::default()),
        Arc::new(tools),
        vec![],
    );
    let id = uuid::Uuid::new_v4().to_string();
    let mut definition = fixture.supervisor.tasks.definition(&fixture.a).unwrap();
    definition.id = id.clone();
    definition.name = "配置组任务".into();
    let binding = ShortcutBinding {
        action: Some(ShortcutCall {
            tool: "bgi.api.invoke".into(),
            arguments: json!({"methodId":"bgi.run_script_groups","arguments":{"groupNames":["已有配置组"]}}),
        }),
        steps: vec![],
        prepare: vec![],
        target_name: "已有配置组".into(),
        application_name: "BetterGI".into(),
    };
    let nodes =
        crate::runtime::operation::shortcuts::nodes(&binding, &catalog(&fixture.supervisor))
            .unwrap();
    let mut revision = task::compile(
        &id,
        1,
        "配置组任务",
        "",
        nodes,
        None,
        &catalog(&fixture.supervisor),
    )
    .unwrap();
    revision.shortcut = Some(binding);
    fixture
        .supervisor
        .tasks
        .create_definition(&definition)
        .unwrap();
    fixture.supervisor.tasks.save_revision(&revision).unwrap();
    let schedule = fixture
        .service
        .create(ScheduleInput {
            name: "定时配置组".into(),
            trigger: Trigger::Daily {
                local_times: vec!["20:00".into()],
                interval_days: 1,
                start_date: None,
            },
            entries: vec![EntryInput {
                entry_id: None,
                binding: Binding::Shortcut {
                    shortcut_id: id.clone(),
                    expected_published_revision: 1,
                },
            }],
        })
        .unwrap();
    assert_eq!(schedule.entries[0].workflow.model_usage, ModelUsage::None);
    if let TaskNode::Tool(node) = &schedule.entries[0].workflow.nodes[0] {
        assert_eq!(node.arguments["arguments"]["waitForCompletion"], true);
    } else {
        panic!("节点类型不符");
    }
    let saved = fixture.supervisor.tasks.revision(&id, 1).unwrap();
    for action in [DefaultAction::CloseGame, DefaultAction::CloseToolSoftware] {
        let close = fixture
            .service
            .default_workflow(
                &schedule.entries[0].workflow,
                action,
                &fixture.supervisor.execution_tool_definitions(),
            )
            .unwrap();
        let TaskNode::Tool(node) = &close.nodes[0] else {
            panic!("关闭动作不是固定工具节点")
        };
        match action {
            DefaultAction::CloseGame => assert_eq!(
                node.arguments,
                json!({"methodId":"bgi.exit_game","arguments":{}})
            ),
            DefaultAction::CloseToolSoftware => {
                assert_eq!(node.tool.as_deref(), Some("bgi.host.close"));
                assert_eq!(node.arguments, json!({}));
            }
        }
    }
    if let TaskNode::Tool(node) = &saved.nodes[0] {
        assert!(node.arguments["arguments"]["waitForCompletion"].is_null());
    }
    let mut unsafe_revision = schedule.entries[0].workflow.clone();
    if let TaskNode::Tool(node) = &mut unsafe_revision.nodes[0] {
        node.arguments["methodId"] = json!("bgi.delete_local_resource");
    }
    assert!(
        fixture
            .service
            .prepare_workflow(
                &mut unsafe_revision,
                &fixture.supervisor.execution_tool_definitions()
            )
            .is_err()
    );
}

fn catalog(supervisor: &Supervisor) -> task::ToolCatalog {
    let contracts = supervisor
        .execution_tool_definitions()
        .into_iter()
        .map(|definition| {
            (
                definition.name,
                task::ToolContract {
                    execution: definition.execution,
                    provider_version: definition.provider_version,
                    input_schema: definition.input_schema,
                },
            )
        })
        .collect::<Vec<_>>();
    crate::runtime::operation::task_store::contract_catalog(&contracts)
}

#[test]
fn scheduled_runs_are_private_without_hiding_normal_chat_titles() {
    let fixture = fixture();
    fixture.service.shutdown();
    fixture.supervisor.shutdown_bound_only();
    let entry = fixture.service.create(input(&fixture)).unwrap().entries[0].clone();
    let run = fixture
        .supervisor
        .journal
        .create_bound_workflow_run(entry.workflow, "fixture-origin", &entry.contract_hash, 60)
        .unwrap();
    let normal = fixture
        .supervisor
        .journal
        .create(
            "定时执行：普通聊天",
            "ordinary-chat",
            "ordinary-key",
            300,
            None,
        )
        .unwrap();
    let chats = fixture.supervisor.journal.conversations().unwrap();
    assert!(chats.iter().any(|chat| chat.id == normal.conversation_id));
    assert!(!chats.iter().any(|chat| chat.id == run.conversation_id));
    assert_eq!(
        fixture
            .supervisor
            .journal
            .conversation_runs(&run.conversation_id)
            .unwrap()
            .len(),
        1
    );
    assert!(fixture.supervisor.journal.get(&run.id).is_ok());
}

#[test]
fn stop_history_requires_confirmation_and_repairs_legacy_results_without_writes() {
    let fixture = fixture();
    fixture.service.shutdown();
    fixture.supervisor.shutdown_bound_only();
    let mut input = input(&fixture);
    input.entries.truncate(1);
    let schedule = fixture.service.create(input).unwrap();
    fixture
        .service
        .store
        .enqueue(schedule.clone(), "stop-fixture".into())
        .unwrap();
    let mut occurrence = fixture.service.store.claim().unwrap().unwrap();
    let entry = &schedule.entries[0];
    let mut run = fixture
        .supervisor
        .journal
        .create_bound_workflow_run(
            entry.workflow.clone(),
            &format!("{}:{}", occurrence.id, entry.entry_id),
            &entry.contract_hash,
            60,
        )
        .unwrap();
    fixture
        .supervisor
        .journal
        .save(&mut run, RunState::Preflighting)
        .unwrap();
    fixture
        .supervisor
        .journal
        .save(&mut run, RunState::Executing)
        .unwrap();
    occurrence.results.push(EntryResult {
        entry_id: entry.entry_id.clone(),
        title: entry.title.clone(),
        state: "cancelled".into(),
        run_id: Some(run.id.clone()),
        error: Some("本项已停止等待，已请求取消，后续项继续按各自运行条件执行".into()),
        stop_state: None,
    });
    fixture
        .service
        .store
        .update_occurrence(&occurrence)
        .unwrap();
    assert!(fixture.service.store.cancel(&occurrence.id).unwrap());
    let read = fixture
        .service
        .history(Some(&schedule.id), 20)
        .unwrap()
        .remove(0);
    assert_eq!(read.state, "cancelling");
    assert_eq!(read.results[0].state, "cancelling");
    assert_eq!(read.results[0].error, None);
    let mut attempt = fixture
        .supervisor
        .journal
        .prepare(&run, "stop-call", json!({}), "fixture")
        .unwrap();
    attempt.outcome = "unknown".into();
    fixture.supervisor.journal.attempt(&attempt).unwrap();
    fixture
        .supervisor
        .journal
        .save(&mut run, RunState::Cancelled)
        .unwrap();
    let read = fixture
        .service
        .history(Some(&schedule.id), 20)
        .unwrap()
        .remove(0);
    assert_eq!(read.state, "unknown");
    assert_eq!(read.results[0].stop_state.as_deref(), Some("unknown"));
    assert_eq!(
        fixture
            .service
            .store
            .occurrence(&occurrence.id)
            .unwrap()
            .state,
        "cancelling",
        "只读历史查询不能写入状态"
    );
    fixture.service.reconcile_stopping().unwrap();
    assert_eq!(
        fixture
            .service
            .store
            .occurrence(&occurrence.id)
            .unwrap()
            .state,
        "unknown"
    );
    attempt.outcome = "cancelled".into();
    attempt.evidence = json!({"verified":true});
    fixture.supervisor.journal.attempt(&attempt).unwrap();
    let read = fixture
        .service
        .history(Some(&schedule.id), 20)
        .unwrap()
        .remove(0);
    assert_eq!(read.state, "cancelled");
    assert_eq!(read.results[0].stop_state.as_deref(), Some("confirmed"));
    assert_eq!(read.results[0].error, None);
    fixture.service.store.update_occurrence(&read).unwrap();
    assert_eq!(
        fixture.service.stop_occurrence(&read.id).unwrap()["cancelRequested"],
        false
    );
}
