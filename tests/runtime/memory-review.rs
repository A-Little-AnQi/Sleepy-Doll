use super::*;

#[tokio::test]
async fn memory_review_validates_candidates_individually_and_drops_unobserved_optional_hints() {
    let fixture = ModelFixture::new(|_| {
        (200,answer(&json!({"goalRelevant":true,"experiences":[
        {"intent":"不真实的入口","conditions":["示例"],"steps":[{"tool":"unobserved","action":"不能保存"}],"verification":["示例"]},
        {"intent":"读取当前配置","conditions":["已定位本次文件"],"steps":[{"tool":"workspace.read","action":"读取当前内容","inputRoles":{"path":"由本次目标取得"}}],"verification":["看到当前返回内容"],"anchors":["invented-location"]}
    ]}).to_string()))
    });
    let supervisor = supervisor(&fixture);
    let run = observed(
        &supervisor,
        "读当前配置",
        "row-validation",
        RunState::Answered,
    );
    supervisor.enqueue_experience_learning(&run).unwrap();
    let (count, _, _) = supervisor
        .learn_experience(
            &run.id,
            &CancellationToken::new(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(count, 1);
    let entries = supervisor.memory.list(10).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].experience.summary, entries[0].experience.intent);
    assert!(entries[0].experience.anchors.is_empty());
    supervisor.shutdown();
}

fn observed(supervisor: &Supervisor, prompt: &str, key: &str, state: RunState) -> Run {
    let mut run = supervisor
        .journal
        .create(prompt, key, key, 600, None)
        .unwrap();
    let mut assistant = message(Role::Assistant, "");
    assistant.tool_calls.push(ToolCall {
        id: "fact".into(),
        name: "workspace.read".into(),
        arguments: json!({"path":"current.json"}),
    });
    supervisor.journal.append_message(&run, &assistant).unwrap();
    let mut result = message(
        Role::Tool,
        json!({"ok":true,"value":{"text":"current configuration","version":"observed"}})
            .to_string(),
    );
    result.tool_call_id = Some("fact".into());
    supervisor.journal.append_message(&run, &result).unwrap();
    if state.terminal() {
        supervisor.journal.finish(&mut run, state).unwrap();
    } else {
        supervisor.journal.save(&mut run, state).unwrap();
    }
    run
}

fn reusable_read() -> Value {
    json!({"goalRelevant":true,"experiences":[{"intent":"读取当前配置","conditions":["已定位本次文件"],"steps":[{"tool":"workspace.read","action":"读取当前内容","inputRoles":{"path":"由本次目标取得"}}],"verification":["看到当前返回内容"]}]})
}

#[tokio::test]
async fn memory_summary_supplement_invalidates_old_checkpoint_and_allows_fresh_review() {
    let fixture = ModelFixture::new(|_| panic!("new input must be processed first"));
    let supervisor = supervisor(&fixture);
    let mut run = observed(
        &supervisor,
        "读取当前配置",
        "summary-supplement",
        RunState::Deciding,
    );
    supervisor
        .journal
        .prepare_summary(&run, RunState::Answered, None)
        .unwrap();
    let mut source = run.clone();
    source.state = RunState::Answered;
    supervisor.enqueue_experience_learning(&source).unwrap();
    supervisor
        .journal
        .input(&run.id, "supplement", "改为另一个当前文件")
        .unwrap();
    assert!(
        !supervisor
            .resume_summary(&mut run, &CancellationToken::new())
            .await
            .unwrap()
    );
    assert!(
        supervisor
            .journal
            .summary_checkpoint(&run.id)
            .unwrap()
            .is_none()
    );
    assert!(
        supervisor
            .memory
            .learning_status(&run.id)
            .unwrap()
            .is_none()
    );
    assert!(supervisor.journal.has_inputs(&run.id).unwrap());
    assert!(!supervisor.journal.get(&run.id).unwrap().state.terminal());
    assert!(fixture.requests.lock().unwrap().is_empty());
    supervisor.shutdown();
}

#[tokio::test]
async fn memory_summary_reviews_before_terminal_and_commits_reply_once() {
    let fixture = ModelFixture::new(|_| (200, answer(&reusable_read().to_string())));
    let supervisor = supervisor(&fixture);
    let mut run = observed(
        &supervisor,
        "读取当前配置",
        "summary-inline",
        RunState::Deciding,
    );
    run.result = Some("已读取当前配置。".into());
    assert!(
        supervisor
            .summarize(
                &mut run,
                RunState::Answered,
                Some(message(Role::Assistant, "已读取当前配置。")),
                &CancellationToken::new()
            )
            .await
            .unwrap()
    );
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(supervisor.memory.list(10).unwrap().len(), 1);
    assert!(
        supervisor
            .journal
            .summary_checkpoint(&run.id)
            .unwrap()
            .is_none()
    );
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    let learned = events
        .iter()
        .position(|e| e.kind == "memory.learned")
        .unwrap();
    let reply = events
        .iter()
        .position(|e| e.kind == "assistant.completed")
        .unwrap();
    let terminal = events
        .iter()
        .rposition(|e| e.kind == "run.changed")
        .unwrap();
    assert!(learned < reply && reply < terminal);
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    supervisor.shutdown();
}

#[tokio::test]
async fn memory_summary_recovery_after_checkpoint_only_resumes_review() {
    let fixture = ModelFixture::new(|request| {
        assert!(
            request["tools"][0]["function"]["name"]
                .as_str()
                .unwrap()
                .starts_with("runtime_memory_experiences_")
        );
        (200, answer(&reusable_read().to_string()))
    });
    let supervisor = supervisor(&fixture);
    let mut run = observed(
        &supervisor,
        "读取当前配置",
        "summary-checkpoint-only",
        RunState::Deciding,
    );
    run.result = Some("已读取当前配置。".into());
    supervisor
        .journal
        .prepare_summary(
            &run,
            RunState::Answered,
            Some(message(Role::Assistant, "已读取当前配置。")),
        )
        .unwrap();
    supervisor
        .journal
        .save(&mut run, RunState::Recovering)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(supervisor.memory.list(10).unwrap().len(), 1);
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    supervisor.shutdown();
}

#[tokio::test]
async fn memory_summary_recovery_reuses_durable_response_without_business_or_model_replay() {
    let fixture = ModelFixture::new(|_| panic!("must reuse persisted result"));
    let supervisor = supervisor(&fixture);
    let mut run = observed(
        &supervisor,
        "读取当前配置",
        "summary-recovery",
        RunState::Deciding,
    );
    run.result = Some("已读取当前配置。".into());
    supervisor
        .journal
        .prepare_summary(
            &run,
            RunState::Answered,
            Some(message(Role::Assistant, "已读取当前配置。")),
        )
        .unwrap();
    let mut source = run.clone();
    source.state = RunState::Answered;
    supervisor.enqueue_experience_learning(&source).unwrap();
    assert!(supervisor.memory.claim_learning(&run.id).unwrap());
    supervisor
        .memory
        .save_learning_result(
            &run.id,
            supervisor.memory.edit_epoch().unwrap(),
            &reusable_read(),
            120,
            80,
        )
        .unwrap();
    // 模拟结果落盘、经验合并后，尚未提交任务终态就退出。
    assert_eq!(
        supervisor
            .learn_experience(
                &run.id,
                &CancellationToken::new(),
                &CancellationToken::new()
            )
            .await
            .unwrap()
            .0,
        1
    );
    supervisor
        .journal
        .save(&mut run, RunState::Recovering)
        .unwrap();
    // 恢复已落盘响应不依赖原模型配置仍然存在。
    supervisor.config.write().unwrap().models.clear();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(supervisor.memory.list(10).unwrap().len(), 1);
    let history = supervisor.journal.run_history(&run).unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|m| m.role == Role::Assistant && m.content == "已读取当前配置。")
            .count(),
        1
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(
        supervisor
            .memory
            .learning_status(&run.id)
            .unwrap()
            .as_deref(),
        Some("complete")
    );
    supervisor.shutdown();
}

#[tokio::test]
async fn memory_summary_unknown_interruption_does_not_repeat_billed_request() {
    let fixture = ModelFixture::new(|_| panic!("unknown request must not replay"));
    let supervisor = supervisor(&fixture);
    let mut run = observed(
        &supervisor,
        "读取当前配置",
        "summary-unknown",
        RunState::Deciding,
    );
    run.result = Some("已读取当前配置。".into());
    supervisor
        .journal
        .prepare_summary(
            &run,
            RunState::Answered,
            Some(message(Role::Assistant, "已读取当前配置。")),
        )
        .unwrap();
    let mut source = run.clone();
    source.state = RunState::Answered;
    supervisor.enqueue_experience_learning(&source).unwrap();
    supervisor.memory.claim_learning(&run.id).unwrap();
    supervisor
        .journal
        .begin_model_request(
            &run,
            "memory",
            &supervisor.model_for(&run).unwrap(),
            120,
            80,
        )
        .unwrap();
    supervisor
        .journal
        .save(&mut run, RunState::Recovering)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert!(supervisor.memory.list(10).unwrap().is_empty());
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(
        supervisor
            .memory
            .learning_status(&run.id)
            .unwrap()
            .as_deref(),
        Some("failed")
    );
    supervisor.shutdown();
}

#[tokio::test]
async fn memory_automatic_review_can_decline_without_frontend_request_and_does_not_repeat() {
    let fixture = ModelFixture::new(|request| {
        let data = request["messages"].to_string();
        assert!(data.contains("learningRequest"));
        assert!(!data.contains("evidenceCallIds"));
        (
            200,
            answer(&json!({"goalRelevant":true,"experiences":[]}).to_string()),
        )
    });
    let supervisor = supervisor(&fixture);
    let run = observed(&supervisor, "查当前配置", "auto-review", RunState::Answered);
    assert!(!supervisor.memory_learning_requested(&run).unwrap());
    assert!(supervisor.enqueue_experience_learning(&run).unwrap());
    assert!(supervisor.memory.list(10).unwrap().is_empty());
    assert!(supervisor.memory.claim_learning(&run.id).unwrap());
    let (count, input, output) = supervisor
        .learn_experience(
            &run.id,
            &CancellationToken::new(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(count, 0);
    supervisor
        .memory
        .finish_learning(&run.id, true, input, output)
        .unwrap();
    supervisor.backfill_latest_experience_learning().unwrap();
    supervisor.enqueue_experience_learning(&run).unwrap();
    assert!(supervisor.memory.pending().unwrap().is_none());
    assert!(supervisor.memory.list(10).unwrap().is_empty());
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    supervisor.shutdown();
}

#[test]
fn memory_startup_backfill_skips_plain_chat_and_only_admits_latest_observed_task() {
    let fixture = ModelFixture::new(|_| panic!("must not dispatch synchronously"));
    let supervisor = supervisor(&fixture);
    let first = observed(
        &supervisor,
        "读更早的配置",
        "older-review",
        RunState::Answered,
    );
    let latest = observed(
        &supervisor,
        "读最近的配置",
        "latest-review",
        RunState::Answered,
    );
    let mut chat = supervisor
        .journal
        .create("你好", "plain-chat", "plain-chat", 60, None)
        .unwrap();
    supervisor
        .journal
        .finish(&mut chat, RunState::Answered)
        .unwrap();
    assert!(!supervisor.enqueue_experience_learning(&chat).unwrap());
    supervisor.backfill_latest_experience_learning().unwrap();
    assert_eq!(
        supervisor.memory.pending().unwrap(),
        Some(latest.id.clone())
    );
    assert!(supervisor.memory.claim_learning(&latest.id).unwrap());
    supervisor
        .memory
        .finish_learning(&latest.id, true, 0, 0)
        .unwrap();
    supervisor.backfill_latest_experience_learning().unwrap();
    assert!(supervisor.memory.pending().unwrap().is_none());
    assert!(supervisor.journal.get(&first.id).is_ok());
    supervisor.shutdown();
}

#[test]
fn memory_review_respects_disabled_cancelled_and_fixed_workflows_but_keeps_failed_partial_facts() {
    let fixture = ModelFixture::new(|_| panic!("no request"));
    let supervisor = supervisor(&fixture);
    let run = observed(
        &supervisor,
        "读配置后运行失败",
        "review-guards",
        RunState::Failed,
    );
    supervisor.update_memory_enabled(false).unwrap();
    assert!(!supervisor.enqueue_experience_learning(&run).unwrap());
    supervisor.update_memory_enabled(true).unwrap();
    let mut cancelled = run.clone();
    cancelled.state = RunState::Cancelled;
    assert!(!supervisor.enqueue_experience_learning(&cancelled).unwrap());
    let mut fixed = run.clone();
    fixed.source = RunSource::SavedWorkflow {
        workflow_id: "saved".into(),
        workflow_revision: 1,
    };
    assert!(!supervisor.enqueue_experience_learning(&fixed).unwrap());
    assert!(supervisor.enqueue_experience_learning(&run).unwrap());
    assert_eq!(supervisor.memory.pending().unwrap(), Some(run.id));
    supervisor.shutdown();
}

#[tokio::test]
#[ignore = "读取安装版真实历史并由配置模型提炼到隔离库，不写正式数据、不执行工具"]
async fn live_memory_review_replays_latest_real_implementation_without_manual_request() {
    let config_path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("model config");
    let model_id = std::env::var("SLEEPY_DOLL_TEST_MODEL").expect("model id");
    let data_path = std::env::var("SLEEPY_DOLL_HISTORY_DATABASE").expect("history db");
    let mut raw: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    crate::config::expand_env(&mut raw).unwrap();
    let mut model: crate::config::ModelConfig = serde_json::from_value(
        raw["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == model_id)
            .unwrap()
            .clone(),
    )
    .unwrap();
    model.id = "fixture".into();
    let live = rusqlite::Connection::open_with_flags(
        data_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let payload: String = live
        .query_row(
            "SELECT payload FROM runtime_runs ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let original: Run = serde_json::from_str(&payload).unwrap();
    assert!(matches!(original.source, RunSource::Agent));
    let mut query=live.prepare("SELECT m.role,m.content,m.tool_call_id,m.tool_calls_json FROM messages m JOIN runtime_message_owners o ON o.message_id=m.id WHERE o.run_id=?1 ORDER BY m.id").unwrap();
    let rows = query
        .query_map([&original.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let human = rows
        .iter()
        .filter(|r| r.0 == "user")
        .map(|r| r.1.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    assert!(!human.is_empty());
    let fixture = ModelFixture::new(|_| panic!("configured real model"));
    let supervisor = supervisor(&fixture);
    {
        let mut config = supervisor.config.write().unwrap();
        config.models[0] = model;
        config.runtime.memory_enabled = true;
    }
    let mut tools = ToolRegistry::default();
    crate::bridge::register_tools(
        &mut tools,
        Arc::new(crate::bridge::BgiClient::new(
            serde_json::from_value(raw["bridge"].clone()).unwrap(),
        )),
    )
    .unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    let mut run = supervisor
        .journal
        .create(&human, "live-memory", "live-memory", 600, None)
        .unwrap();
    for (role, content, id, calls) in rows {
        if role != "assistant" && role != "tool" {
            continue;
        }
        let mut message = message(
            if role == "assistant" {
                Role::Assistant
            } else {
                Role::Tool
            },
            content,
        );
        message.tool_call_id = id;
        message.tool_calls = calls
            .map(|c| serde_json::from_str(&c).unwrap())
            .unwrap_or_default();
        supervisor.journal.append_message(&run, &message).unwrap();
    }
    run.result = original.result.clone();
    assert!(!supervisor.memory_learning_requested(&run).unwrap());
    supervisor
        .journal
        .prepare_summary(&run, original.state, None)
        .unwrap();
    let mut source = run.clone();
    source.state = original.state;
    supervisor.enqueue_experience_learning(&source).unwrap();
    supervisor.memory.claim_learning(&run.id).unwrap();
    supervisor
        .learn_experience(
            &run.id,
            &CancellationToken::new(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let candidate_diagnostics = supervisor
        .memory
        .learning_result(&run.id)
        .unwrap()
        .map(|r| r.1);
    supervisor
        .journal
        .save(&mut run, RunState::Recovering)
        .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_secs(240),
            supervisor.resume_summary(&mut run, &CancellationToken::new())
        )
        .await
        .unwrap()
        .unwrap()
    );
    assert_eq!(run.state, original.state);
    let entries = supervisor.memory.list(20).unwrap();
    let recalled = supervisor
        .recall_memories("配置采集路线 指定队伍 同步战斗配置 后台运行任务")
        .unwrap();
    let report = json!({"sourceRun":original.id,"entries":entries,"candidateDiagnostics":candidate_diagnostics,"recalled":recalled.len(),"usage":supervisor.journal.model_usage(&run.id).unwrap(),"gameActions":0,"liveDatabaseWritten":false});
    let output = std::env::var("SLEEPY_DOLL_MEMORY_REPORT").expect("report");
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!(
        "memory entries={}, recalled={}",
        entries.len(),
        recalled.len()
    );
    assert!(
        !entries.is_empty(),
        "inspect report and memory model outcome"
    );
    assert!(!recalled.is_empty());
    for entry in &entries {
        let text = serde_json::to_string(&entry.experience).unwrap();
        for value in ["千织", "精英", "baixi", "起个名字好难的喵"] {
            assert!(
                !text.contains(value),
                "memory must abstract instance values: {value}"
            );
        }
    }
    supervisor.shutdown();
}
