use serde_json::{Value, json};
use sleepy_doll::{
    app::AppController,
    config::ModelProtocol,
    model::mock::MockBackend,
    model::{Message, Reasoning, Role},
    runtime::{context, gateway::Decoder, store::journal::Journal, types::*},
};
use std::{fs, sync::Arc, thread, time::Duration};

fn journal() -> (tempfile::TempDir, Journal) {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("test.db");
    // The journal owns the whole schema, including conversation storage, so it
    // no longer needs another store to be opened first.
    let j = Journal::open(&path).unwrap();
    (d, j)
}
#[test]
fn durable_submission_is_idempotent_and_conflicts_are_rejected() {
    let (_d, j) = journal();
    let a = j.create("go", "c", "key", 1800, None).unwrap();
    let b = j.create("go", "c", "key", 1800, None).unwrap();
    assert_eq!(a.id, b.id);
    assert!(j.create("other", "c", "key", 1800, None).is_err());
    assert_eq!(j.history(&a).unwrap().len(), 1);
    assert_eq!(j.events("c", 0).unwrap().len(), 1);
}

#[test]
fn repeated_supplement_is_recorded_once_even_after_run_completion() {
    let (_directory, journal) = journal();
    let mut run = journal
        .create("first", "conversation", "run-key", 1800, None)
        .unwrap();
    journal.save(&mut run, RunState::Deciding).unwrap();
    journal
        .input_once(&run.id, "supplement", "second", Some("input-key"))
        .unwrap();
    journal
        .input_once(&run.id, "supplement", "second", Some("input-key"))
        .unwrap();
    assert!(
        journal
            .input_once(&run.id, "supplement", "different", Some("input-key"))
            .is_err()
    );
    assert_eq!(
        journal.conversation_messages("conversation").unwrap().len(),
        2
    );
    journal.save(&mut run, RunState::Cancelled).unwrap();
    journal
        .input_once(&run.id, "supplement", "second", Some("input-key"))
        .unwrap();
    assert_eq!(
        journal.conversation_messages("conversation").unwrap().len(),
        2
    );
}

#[test]
fn structured_checkpoint_tracks_run_revision_and_pending_step() {
    let (_d, j) = journal();
    let mut run = j.create("goal", "c", "checkpoint", 1800, None).unwrap();
    j.save(&mut run, RunState::Deciding).unwrap();
    let plan = PlanRevision {
        revision: 1,
        goal: "goal".into(),
        steps: vec![PlanStep {
            id: "one".into(),
            title: "one".into(),
            capability_id: Some("mock.success".into()),
            tool: None,
            execution: None,
            provider_version: None,
            resource_versions: vec![],
            arguments: json!({}),
            depends_on: vec![],
        }],
    };
    j.save_plan(&run, &plan).unwrap();
    let checkpoint = j.checkpoint(&run.id).unwrap();
    assert_eq!(checkpoint.run_revision, run.revision);
    assert_eq!(checkpoint.plan_revision, Some(1));
    assert_eq!(checkpoint.pending_step.as_deref(), Some("one"));
}

#[test]
fn discovered_tools_survive_run_payload_roundtrip() {
    let (_d, j) = journal();
    let mut run = j.create("goal", "c", "discovered", 1800, None).unwrap();
    run.discovered = vec!["bridge-api:bgi.ping:v1".into()];
    j.save(&mut run, RunState::Deciding).unwrap();
    let loaded = j.get(&run.id).unwrap();
    assert_eq!(loaded.discovered, vec!["bridge-api:bgi.ping:v1"]);
    assert_eq!(
        j.checkpoint(&run.id).unwrap().discovered,
        vec!["bridge-api:bgi.ping:v1"]
    );
}
#[test]
fn stale_revision_and_terminal_transition_cannot_advance_run() {
    let (_d, j) = journal();
    let mut a = j.create("go", "c", "key", 1800, None).unwrap();
    let mut stale = a.clone();
    j.save(&mut a, RunState::Deciding).unwrap();
    assert!(j.save(&mut stale, RunState::Deciding).is_err());
    j.save(&mut a, RunState::Succeeded).unwrap();
    assert!(j.save(&mut a, RunState::Executing).is_err());
    assert_eq!(j.events("c", 0).unwrap().len(), 3);
}
#[test]
fn game_leases_survive_reopen_and_are_not_released_by_other_attempts() {
    let (d, j) = journal();
    let run = j.create("go", "c", "k", 1800, None).unwrap();
    let a = j.prepare(&run, "one", json!({}), "game").unwrap();
    let b = j.prepare(&run, "two", json!({}), "game").unwrap();
    assert!(j.acquire(&a).unwrap());
    assert!(!j.acquire(&b).unwrap());
    j.release(&b).unwrap();
    drop(j);
    let j = Journal::open(&d.path().join("test.db")).unwrap();
    assert!(!j.acquire(&b).unwrap());
    j.release(&a).unwrap();
    assert!(j.acquire(&b).unwrap());
}
#[test]
fn queued_messages_do_not_enter_prior_run_context() {
    let (_d, j) = journal();
    let a = j.create("first", "c", "1", 1800, None).unwrap();
    j.create("second", "c", "2", 1800, None).unwrap();
    assert_eq!(j.history(&a).unwrap().len(), 1);
}

#[test]
fn queued_transcript_groups_answers_under_their_own_run() {
    let (_d, j) = journal();
    let mut a = j.create("first", "c", "1", 1800, None).unwrap();
    j.save(&mut a, RunState::Deciding).unwrap();
    let mut b = j.create("second", "c", "2", 1800, None).unwrap();
    j.append_message(&a, &context::message(Role::Assistant, "first answer"))
        .unwrap();
    assert_eq!(j.conversation_messages("c").unwrap().len(), 2);
    j.save(&mut b, RunState::Deciding).unwrap();
    let text = j
        .history(&b)
        .unwrap()
        .into_iter()
        .map(|m| m.content)
        .collect::<Vec<_>>();
    assert_eq!(text, vec!["first", "first answer", "second"]);
}

#[test]
fn conversation_fork_copies_completed_history_only() {
    let (_d, j) = journal();
    let mut completed = j.create("first", "c", "fork-1", 1800, None).unwrap();
    j.save(&mut completed, RunState::Deciding).unwrap();
    j.append_message(&completed, &context::message(Role::Assistant, "done"))
        .unwrap();
    j.finish(&mut completed, RunState::Answered).unwrap();
    j.create("pending", "c", "fork-2", 1800, None).unwrap();
    let fork = j.fork_conversation("c", None).unwrap();
    let messages = j.conversation_messages(&fork).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "first");
    assert_eq!(messages[1].content, "done");
}
#[test]
fn finish_cannot_drop_an_acknowledged_supplement() {
    let (_d, j) = journal();
    let mut run = j.create("first", "c", "1", 1800, None).unwrap();
    j.save(&mut run, RunState::Deciding).unwrap();
    j.input(&run.id, "supplement", "new constraint").unwrap();
    assert!(!j.finish(&mut run, RunState::Answered).unwrap());
    assert_eq!(j.conversation_messages("c").unwrap().len(), 2);
    assert_eq!(j.drain_inputs(&run.id).unwrap(), vec!["new constraint"]);
    assert!(j.finish(&mut run, RunState::Answered).unwrap());
}
#[test]
fn approval_is_single_use_and_expiring() {
    let (_d, j) = journal();
    let mut run = j.create("approval", "c", "key", 1800, None).unwrap();
    j.save(&mut run, RunState::Deciding).unwrap();
    j.save(&mut run, RunState::Executing).unwrap();
    j.save(&mut run, RunState::AwaitingApproval).unwrap();
    let a = Approval {
        id: "a".into(),
        run_id: run.id,
        request_hash: hash(&json!({"x":1})),
        request: json!({"x":1}),
        expires_at: unix_now() + 30,
        decision: None,
    };
    j.approval(&a).unwrap();
    assert_eq!(j.decide("a", true).unwrap().decision, Some(true));
    assert!(j.decide("a", true).is_err());
    let expired = Approval {
        id: "expired".into(),
        expires_at: unix_now() - 1,
        ..a
    };
    j.approval(&expired).unwrap();
    assert!(j.decide("expired", true).is_err());
}
#[test]
fn stream_requires_completion_and_valid_tool_json() {
    let mut d = Decoder::new(ModelProtocol::OpenaiChat);
    d.push(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"foo","arguments":"{"}}]}}]})).unwrap();
    assert!(d.finish().is_err());
    let mut d = Decoder::new(ModelProtocol::OpenaiChat);
    d.push(json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"foo","arguments":"{"}}]},"finish_reason":"tool_calls"}]})).unwrap();
    assert!(d.finish().is_err());
}
#[test]
fn five_protocols_decode_public_text_only() {
    for p in [
        ModelProtocol::OpenaiResponses,
        ModelProtocol::OpenaiChat,
        ModelProtocol::AnthropicMessages,
        ModelProtocol::Gemini,
        ModelProtocol::OllamaChat,
    ] {
        let mut d = Decoder::new(p);
        let frames = match p {
            ModelProtocol::OpenaiResponses => vec![
                json!({"type":"response.output_text.delta","delta":"ok"}),
                json!({"type":"response.completed","response":{"status":"completed","output":[
                    {"type":"reasoning","id":"rs_1","summary":[{"type":"summary_text","text":"private"}]},
                    {"type":"message","content":[{"type":"output_text","text":"ok"}]}
                ]}}),
            ],
            ModelProtocol::OpenaiChat => vec![
                json!({"choices":[{"delta":{"reasoning_content":"private"},"finish_reason":null}]}),
                json!({"choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}]}),
            ],
            ModelProtocol::AnthropicMessages => vec![
                json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private"}}),
                json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}),
                json!({"type":"content_block_delta","index":1,"delta":{"text":"ok"}}),
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
                json!({"type":"message_stop"}),
            ],
            ModelProtocol::Gemini => vec![
                json!({"candidates":[{"content":{"parts":[{"thought":true,"text":"private"},{"text":"ok"}]},"finishReason":"STOP"}]}),
            ],
            ModelProtocol::OllamaChat => vec![
                json!({"message":{"content":"ok","thinking":"private"},"done":true,"done_reason":"stop"}),
            ],
        };
        for v in frames {
            d.push(v).unwrap();
        }
        let response = d.finish().unwrap();
        assert_eq!(response.text, "ok", "{p:?}");
        // 推理内容必须被收集，且不得混进公开文本。
        let reasoning = response.reasoning.expect("推理内容未收集");
        assert_eq!(reasoning.text, "private", "{p:?}");
        assert!(reasoning.matches(p), "{p:?}");
        assert!(!response.text.contains("private"), "{p:?}");
    }
}

/// 推理帧不能经 `push` 的返回值流进 assistant.delta —— 那是逐字上屏的通路。
#[test]
fn anthropic_reasoning_frames_never_return_a_public_delta() {
    let mut d = Decoder::new(ModelProtocol::AnthropicMessages);
    for v in [
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"私密推理"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
        json!({"type":"message_stop"}),
    ] {
        assert!(d.push(v).unwrap().is_none());
    }
    // 整轮只产出了思考，没有可见回复：拒绝该轮，并说明原因。
    let error = d.finish().unwrap_err().to_string();
    assert!(error.contains("思考内容"), "{error}");
}

/// 只产生思考、没有可见回复的轮次要给出可操作的原因，而不是笼统的
/// "empty model response"。
#[test]
fn thinking_only_turn_reports_a_specific_error() {
    let reasoning = Some(Reasoning {
        protocol: ModelProtocol::AnthropicMessages,
        blocks: vec![json!({"type":"thinking","thinking":"想了很久","signature":"s"})],
        text: "想了很久".into(),
    });
    let empty = sleepy_doll::model::ModelResponse {
        text: String::new(),
        tool_calls: vec![],
        finish_reason: Some("end_turn".into()),
        usage: Default::default(),
        reasoning: reasoning.clone(),
    };
    let error = sleepy_doll::runtime::gateway::validate(&empty)
        .unwrap_err()
        .to_string();
    assert!(error.contains("思考内容"), "{error}");

    let truncated = sleepy_doll::model::ModelResponse {
        text: String::new(),
        tool_calls: vec![],
        finish_reason: Some("max_tokens".into()),
        usage: Default::default(),
        reasoning,
    };
    let error = sleepy_doll::runtime::gateway::validate(&truncated)
        .unwrap_err()
        .to_string();
    assert!(error.contains("输出预算"), "{error}");

    let partial = sleepy_doll::model::ModelResponse {
        text: "先说一半".into(),
        tool_calls: vec![],
        finish_reason: Some("max_tokens".into()),
        usage: Default::default(),
        reasoning: None,
    };
    sleepy_doll::runtime::gateway::validate(&partial).unwrap();
}

/// 估算结果与 `max_tokens` 同单位：中文按字计，不按字节。
#[test]
fn token_estimation_counts_characters_not_bytes() {
    // 非 ASCII 按 1 token/字，ASCII 按 4 字符 1 token。
    assert_eq!(context::estimate_tokens("中文字符"), 4);
    assert_eq!(context::estimate_tokens("abcdefgh"), 2);

    // 满额上下文：48000 个中文字。按字节算约 144000，必然超预算；按 token 算
    // 只有 48000，落在 100000 之内。
    let full = vec![context::message(Role::User, "中".repeat(48_000))];
    let estimated = context::estimate_messages_tokens(&full);
    assert_eq!(estimated, 48_008);
    assert!(estimated < 100_000, "满额中文上下文估算为 {estimated}");

    // 结构化字段按实际长度算，不把 JSON 的括号引号当内容长度。
    let tool = Message {
        role: Role::Tool,
        content: r#"{"ok":true}"#.into(),
        tool_call_id: Some("c".into()),
        tool_calls: vec![],
        reasoning: None,
    };
    assert_eq!(context::estimate_messages_tokens(&[tool]), 11);
}

/// 分叉会话漏拷推理列，会在首个后续请求上静默 400。
#[test]
fn fork_conversation_preserves_thinking_blocks() {
    let (_d, j) = journal();
    let mut run = j.create("go", "c", "fork", 1800, None).unwrap();
    j.save(&mut run, RunState::Deciding).unwrap();
    let reasoning = Reasoning {
        protocol: ModelProtocol::AnthropicMessages,
        blocks: vec![json!({"type":"thinking","thinking":"先看看","signature":"sig-abc"})],
        text: "先看看".into(),
    };
    j.append_message(
        &run,
        &Message {
            role: Role::Assistant,
            content: "好的".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: Some(reasoning.clone()),
        },
    )
    .unwrap();
    // 分叉只拷贝终态运行的轮次。
    j.save(&mut run, RunState::Succeeded).unwrap();

    let forked = j.fork_conversation("c", None).unwrap();
    let copied = j.conversation_messages(&forked).unwrap();
    let stored = copied
        .iter()
        .find(|m| m.role == Role::Assistant)
        .expect("分叉后缺少 assistant 轮次");
    assert_eq!(stored.reasoning.as_ref(), Some(&reasoning));
}
#[test]
fn context_refuses_oversized_recent_messages() {
    assert!(
        context::build(
            "system".into(),
            vec![context::message(Role::User, "a".repeat(5000))],
            4096
        )
        .is_err()
    );
}

#[test]
fn clarification_messages_do_not_break_tool_result_pairing() {
    let mut assistant = context::message(Role::Assistant, "");
    assistant.tool_calls = vec![sleepy_doll::model::ToolCall {
        id: "c".into(),
        name: "user.ask".into(),
        arguments: json!({}),
    }];
    let mut tool = context::message(Role::Tool, "answer");
    tool.tool_call_id = Some("c".into());
    let result = context::build(
        "system".into(),
        vec![assistant, context::message(Role::User, "my choice"), tool],
        10000,
    )
    .unwrap();
    assert_eq!(result[1].role, Role::Assistant);
    assert_eq!(result[2].role, Role::Tool);
    assert_eq!(result[3].role, Role::User);
}

fn call(name: &str, args: Value) -> Value {
    json!({"status":"completed","output":[{"type":"function_call","call_id":"provider-repeat","name":name,"arguments":args.to_string()}],"usage":{"input_tokens":0,"output_tokens":0}})
}
fn answer() -> Value {
    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}],"usage":{"input_tokens":0,"output_tokens":0}})
}

fn truncated(text: &str) -> Value {
    json!({
        "status":"incomplete",
        "incomplete_details":{"reason":"max_output_tokens"},
        "output":[{"type":"message","content":[{"type":"output_text","text":text}]}],
        "usage":{"input_tokens":0,"output_tokens":0}
    })
}

#[test]
fn agent_reads_a_live_api_only_after_loading_its_contract() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("bgi.api.describe", json!({"methodId":"bgi.ping"})),
        call(
            "bgi.api.read",
            json!({"methodId":"bgi.ping","arguments":{}}),
        ),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let app = controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"check bridge","clientKey":"api-read"}),
    );
    let terminal = wait(
        &app,
        run["id"].as_str().unwrap(),
        &["answered", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    let events = ipc(
        &app,
        "events.read",
        json!({"conversationId":run["conversationId"],"after":0}),
    );
    assert!(events.to_string().contains("simulated"));
    assert_eq!(backend.job_count(), 0);
    app.shutdown();
}

#[test]
fn core_job_lookup_is_not_misrouted_as_a_plugin() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("bgi.job.get", json!({"jobId":"missing-job"})),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let app = controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"recover job","clientKey":"job-get-routing"}),
    );
    let terminal = wait(
        &app,
        run["id"].as_str().unwrap(),
        &["answered", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    let history = ipc(
        &app,
        "conversation.get",
        json!({"id":run["conversationId"]}),
    );
    let tool_result = history["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool")
        .and_then(|message| message["content"].as_str())
        .unwrap();
    assert!(tool_result.contains("HTTP error"), "{tool_result}");
    assert!(!tool_result.contains("插件已停用"), "{tool_result}");
}

#[test]
fn agent_dynamic_api_write_requires_approval_and_tracks_a_job() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("bgi.api.describe", json!({"methodId":"mock.config"})),
        call(
            "bgi.api.invoke",
            json!({"methodId":"mock.config","arguments":{}}),
        ),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let app = approval_controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"change test configuration","clientKey":"api-write"}),
    );
    approve_pending(&app, &run);
    let terminal = wait(
        &app,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(backend.job_count(), 1);
    app.shutdown();
}

#[test]
fn agent_plan_can_read_contract_then_submit_a_dependent_api_write() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("plan.update", json!({"goal":"update","steps":[
            {"id":"describe","title":"read contract","tool":"bgi.api.describe","arguments":{"methodId":"mock.config"}},
            {"id":"write","title":"apply change","tool":"bgi.api.invoke","arguments":{"methodId":"mock.config","arguments":{}},"dependsOn":["describe"]}
        ]})),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let app = approval_controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"change test configuration","clientKey":"api-plan"}),
    );
    approve_pending(&app, &run);
    let terminal = wait(
        &app,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(backend.job_count(), 1);
    app.shutdown();
}
fn controller(backend: &MockBackend, d: &tempfile::TempDir) -> Arc<AppController> {
    controller_with_mode(backend, d, "standard")
}

/// 专门验证审批流程的用例显式选「请求审批」，不依赖默认值 —— 默认值是产品决定，
/// 拿它当测试前提会让用例在产品改默认时无声失效。
fn approval_controller(backend: &MockBackend, d: &tempfile::TempDir) -> Arc<AppController> {
    controller_with_mode(backend, d, "askEach")
}

/// 「完全控制」：写操作一律直接执行，不产生任何审批。
fn full_access_controller(backend: &MockBackend, d: &tempfile::TempDir) -> Arc<AppController> {
    controller_with_mode(backend, d, "fullAccess")
}

fn controller_with_mode(
    backend: &MockBackend,
    d: &tempfile::TempDir,
    mode: &str,
) -> Arc<AppController> {
    fs::create_dir_all(d.path().join("capabilities")).unwrap();
    for name in ["mock.success", "mock.unknown", "mock.failure"] {
        fs::write(
            d.path().join("capabilities").join(format!("{name}.json")),
            serde_json::to_vec(
                &json!({"id":name,"description":"test","methodId":name,"catalogVersion":"mock-v1"}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let path = d.path().join("config.json");
    fs::write(&path,serde_json::to_vec(&json!({"version":1,"activeModel":"mock","models":[{"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock-model","baseUrl":format!("{}/v1",backend.base_url()),"options":{"timeoutMs":2000}}],"agent":{"systemPrompt":"test"},"runtime":{"permissionMode":mode},"bridge":{"enabled":true,"baseUrl":backend.base_url(),"token":"mock","instanceId":"mock-bgi","timeoutMs":2000},"storage":{"database":d.path().join("test.db")}})).unwrap()).unwrap();
    Arc::new(AppController::load(path).unwrap())
}

fn plugin_controller(
    backend: &MockBackend,
    d: &tempfile::TempDir,
    execution: Option<Value>,
) -> Arc<AppController> {
    let folder = d.path().join("plugins/sample/.sleepy-doll-plugin");
    fs::create_dir_all(&folder).unwrap();
    let mut tool = json!({
        "name":"lookup",
        "description":"query mock state",
        "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
        "method":"GET",
        "url":format!("{}/bridge/v1/state", backend.base_url())
    });
    if let Some(execution) = execution {
        tool["execution"] = execution;
    }
    fs::write(
        folder.join("plugin.json"),
        json!({"schemaVersion":1,"id":"sample","name":"sample","version":"1","httpTools":[tool]})
            .to_string(),
    )
    .unwrap();
    let path = d.path().join("config.json");
    fs::write(&path,serde_json::to_vec(&json!({"version":1,"activeModel":"mock","models":[{"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock-model","baseUrl":format!("{}/v1",backend.base_url()),"options":{"timeoutMs":2000}}],"agent":{"systemPrompt":"test"},"bridge":{"enabled":false,"baseUrl":backend.base_url()},"plugins":{"directories":[d.path().join("plugins")],"enabled":["sample"]},"storage":{"database":d.path().join("test.db")}})).unwrap()).unwrap();
    Arc::new(AppController::load(path).unwrap())
}
fn ipc(c: &Arc<AppController>, method: &str, params: Value) -> Value {
    c.handle(method, params, Arc::new(|_, _| {})).unwrap()
}
fn wait(c: &Arc<AppController>, id: &str, states: &[&str]) -> Value {
    for _ in 0..250 {
        let r = ipc(c, "run.get", json!({"id":id}));
        if states.contains(&r["state"].as_str().unwrap_or("")) {
            return r;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "state wait timed out: {}",
        ipc(c, "run.get", json!({"id":id}))
    );
}

#[test]
fn explicitly_read_only_plugin_runs_without_write_approval() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("tools.search", json!({"query":"lookup"})),
        call("sample.http.lookup", json!({})),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = plugin_controller(
        &backend,
        &d,
        Some(json!({"effect":"readOnly","concurrencySafe":true,"maxResultChars":2048})),
    );
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"lookup","clientKey":"read-only"}),
    );
    let id = run["id"].as_str().unwrap();
    assert_eq!(wait(&c, id, &["answered"])["state"], "answered");
    let events = ipc(
        &c,
        "events.read",
        json!({"conversationId":run["conversationId"],"after":0}),
    );
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| { event["kind"] != "approval.requested" })
    );
}

#[test]
fn plugin_without_effect_metadata_still_requires_approval() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("tools.search", json!({"query":"lookup"})),
        call("sample.http.lookup", json!({})),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = plugin_controller(&backend, &d, None);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"lookup","clientKey":"unknown-effect"}),
    );
    let id = run["id"].as_str().unwrap();
    assert_eq!(
        wait(&c, id, &["awaitingApproval"])["state"],
        "awaitingApproval"
    );
}
#[test]
fn approval_then_unknown_job_never_becomes_success() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "bgi.capability.describe",
            json!({"methodId":"mock.unknown"}),
        ),
        call(
            "bgi.capability.invoke",
            json!({"methodId":"mock.unknown","arguments":{}}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    // 这个用例验证的就是审批本身，级别显式选「请求审批」。
    let c = approval_controller(&backend, &d);
    let r = ipc(
        &c,
        "run.submit",
        json!({"prompt":"test","clientKey":"request"}),
    );
    let id = r["id"].as_str().unwrap();
    wait(&c, id, &["awaitingApproval"]);
    assert_eq!(backend.job_count(), 0);
    let mut approval = None;
    for _ in 0..100 {
        let events = ipc(
            &c,
            "events.read",
            json!({"conversationId":r["conversationId"],"after":0}),
        );
        approval = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["kind"] == "approval.requested")
            .map(|e| e["data"]["id"].clone());
        if approval.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    ipc(
        &c,
        "approval.respond",
        json!({"id":approval.unwrap(),"approved":true}),
    );
    let end = wait(&c, id, &["needsReview", "failed", "succeeded"]);
    assert_eq!(end["state"], "needsReview");
    assert_eq!(backend.job_count(), 1);
}
#[test]
fn cancellation_while_awaiting_user_does_not_wait_for_model() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![call("user.ask", json!({"question":"which route?"}))]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let r = ipc(
        &c,
        "run.submit",
        json!({"prompt":"test","clientKey":"request"}),
    );
    let id = r["id"].as_str().unwrap();
    wait(&c, id, &["awaitingUser"]);
    ipc(&c, "run.cancel", json!({"id":id}));
    assert_eq!(wait(&c, id, &["cancelled"])["state"], "cancelled");
}

fn approve_pending(c: &Arc<AppController>, run: &Value) {
    let id = run["id"].as_str().unwrap();
    wait(c, id, &["awaitingApproval"]);
    for _ in 0..100 {
        let events = ipc(
            c,
            "events.read",
            json!({"conversationId":run["conversationId"],"after":0}),
        );
        if let Some(event) = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|e| e["kind"] == "approval.requested")
        {
            ipc(
                c,
                "approval.respond",
                json!({"id":event["data"]["id"],"approved":true}),
            );
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("approval not found")
}

#[test]
fn acceptance_response_loss_reuses_original_job() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        lose_next_acceptance: true,
        ..Default::default()
    });
    backend.set_responses(vec![
        call(
            "bgi.capability.describe",
            json!({"methodId":"mock.success"}),
        ),
        call(
            "bgi.capability.invoke",
            json!({"methodId":"mock.success","arguments":{}}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = approval_controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"execute","clientKey":"r"}),
    );
    approve_pending(&c, &run);
    assert_eq!(
        wait(
            &c,
            run["id"].as_str().unwrap(),
            &["succeeded", "failed", "needsReview"]
        )["state"],
        "succeeded"
    );
    assert_eq!(backend.job_count(), 1);
}
#[test]
fn cancelled_model_request_returns_promptly() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        model_delay_ms: 2000,
        ..Default::default()
    });
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(&c, "run.submit", json!({"prompt":"test","clientKey":"r"}));
    let id = run["id"].as_str().unwrap();
    wait(&c, id, &["deciding"]);
    let start = std::time::Instant::now();
    ipc(&c, "run.cancel", json!({"id":id}));
    wait(&c, id, &["cancelled"]);
    assert!(start.elapsed() < Duration::from_millis(1000));
}

#[test]
fn run_deadline_bounds_a_model_that_never_sends_headers() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        model_delay_ms: 5000,
        ..Default::default()
    });
    let directory = tempfile::tempdir().unwrap();
    let controller = controller(&backend, &directory);
    let started = std::time::Instant::now();
    let run = ipc(
        &controller,
        "run.submit",
        json!({"prompt":"test","clientKey":"deadline","durationSec":1}),
    );
    let finished = wait(&controller, run["id"].as_str().unwrap(), &["failed"]);
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(finished["error"].as_str().unwrap().contains("时限"));
    controller.shutdown();
}
#[test]
fn cancelled_job_is_confirmed_before_releasing_lease() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        job_delay_ms: 4000,
        ..Default::default()
    });
    backend.set_responses(vec![
        call(
            "bgi.capability.describe",
            json!({"methodId":"mock.success"}),
        ),
        call(
            "bgi.capability.invoke",
            json!({"methodId":"mock.success","arguments":{}}),
        ),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = approval_controller(&backend, &d);
    let run = ipc(&c, "run.submit", json!({"prompt":"test","clientKey":"r"}));
    approve_pending(&c, &run);
    let id = run["id"].as_str().unwrap();
    wait(&c, id, &["waitingJob"]);
    ipc(&c, "run.cancel", json!({"id":id}));
    assert_eq!(
        wait(&c, id, &["cancelled", "needsReview"])["state"],
        "cancelled"
    );
    let journal = Journal::open(&d.path().join("test.db")).unwrap();
    let a = journal.attempts(id).unwrap();
    assert_eq!(a[0].outcome, "cancelled");
    assert!(journal.acquire(&a[0]).unwrap());
    journal.release(&a[0]).unwrap();
}
#[test]
fn expired_observation_prevents_game_submission() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        stale_state: true,
        ..Default::default()
    });
    backend.set_responses(vec![
        call(
            "bgi.capability.describe",
            json!({"methodId":"mock.success"}),
        ),
        call(
            "bgi.capability.invoke",
            json!({"methodId":"mock.success","arguments":{}}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = approval_controller(&backend, &d);
    let run = ipc(&c, "run.submit", json!({"prompt":"test","clientKey":"r"}));
    approve_pending(&c, &run);
    let end = wait(
        &c,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(end["state"], "failed");
    assert_eq!(backend.job_count(), 0);
}
#[test]
fn truncated_stream_never_succeeds() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        truncate_model_stream: true,
        ..Default::default()
    });
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(&c, "run.submit", json!({"prompt":"test","clientKey":"r"}));
    assert_eq!(
        wait(&c, run["id"].as_str().unwrap(), &["failed", "succeeded"])["state"],
        "failed"
    );
}
#[test]
fn same_conversation_queue_waits_for_active_run() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("user.ask", json!({"question":"which?"})),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let a = ipc(
        &c,
        "run.submit",
        json!({"prompt":"first","conversationId":"c","clientKey":"1"}),
    );
    let id = a["id"].as_str().unwrap();
    wait(&c, id, &["awaitingUser"]);
    let b = ipc(
        &c,
        "run.submit",
        json!({"prompt":"second","conversationId":"c","clientKey":"2"}),
    );
    thread::sleep(Duration::from_millis(150));
    assert_eq!(ipc(&c, "run.get", json!({"id":b["id"]}))["state"], "queued");
    ipc(&c, "run.cancel", json!({"id":id}));
    wait(&c, b["id"].as_str().unwrap(), &["answered"]);
}

#[test]
fn event_long_poll_on_ipc_thread_times_out_without_panicking() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let controller = controller(&backend, &directory);
    let worker = thread::spawn(move || {
        let start = std::time::Instant::now();
        let result = ipc(
            &controller,
            "events.read",
            json!({"conversationId":"empty","after":0,"waitMs":40}),
        );
        assert!(start.elapsed() >= Duration::from_millis(30));
        assert!(result["events"].as_array().unwrap().is_empty());
        controller.shutdown();
    });
    worker
        .join()
        .expect("IPC long polling must not require a caller-owned Tokio runtime");
}

#[test]
fn another_conversation_and_absent_event_subscriber_do_not_stop_a_run() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("user.ask", json!({"question":"continue?"})),
        answer(),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let controller = controller(&backend, &directory);
    let first = ipc(
        &controller,
        "run.submit",
        json!({"prompt":"first","conversationId":"first","clientKey":"background-one"}),
    );
    wait(
        &controller,
        first["id"].as_str().unwrap(),
        &["awaitingUser"],
    );
    let second = ipc(
        &controller,
        "run.submit",
        json!({"prompt":"second","conversationId":"second","clientKey":"background-two"}),
    );
    wait(&controller, second["id"].as_str().unwrap(), &["answered"]);
    assert_eq!(
        ipc(&controller, "run.get", json!({"id":first["id"]}))["state"],
        "awaitingUser"
    );
    ipc(
        &controller,
        "run.input",
        json!({"id":first["id"],"content":"continue"}),
    );
    wait(&controller, first["id"].as_str().unwrap(), &["answered"]);
    let events = ipc(
        &controller,
        "events.read",
        json!({"conversationId":"first","after":0}),
    );
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "assistant.completed")
    );
    assert_eq!(
        ipc(&controller, "task.list", json!({}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    controller.shutdown();
}
#[test]
fn resource_changes_invalidate_binding() {
    let d = tempfile::tempdir().unwrap();
    fs::create_dir(d.path().join("capabilities")).unwrap();
    fs::create_dir(d.path().join("resources")).unwrap();
    fs::write(d.path().join("route.json"), "route").unwrap();
    use sha2::{Digest, Sha256};
    let hash = format!("{:x}", Sha256::digest(b"route"));
    fs::write(
        d.path().join("resources/r.json"),
        json!({"id":"r","name":"采矿路线","kind":"route","path":"route.json","contentHash":hash})
            .to_string(),
    )
    .unwrap();
    fs::write(d.path().join("capabilities/c.json"),json!({"id":"c","description":"采矿","methodId":"run","catalogVersion":"1","resourceFields":["/routeId"]}).to_string()).unwrap();
    let catalog = sleepy_doll::runtime::host::catalog::Catalog::load(d.path()).unwrap();
    assert!(catalog.resolve("c", &json!({"routeId":"r"})).is_ok());
    fs::write(d.path().join("route.json"), "changed").unwrap();
    assert!(catalog.resolve("c", &json!({"routeId":"r"})).is_err());
}
#[test]
fn verifier_will_not_promote_stale_or_missing_evidence() {
    use sleepy_doll::runtime::{host::catalog::Predicate, operation::verifier::verify};
    let p = vec![Predicate::Equals {
        pointer: "/ui/value".into(),
        value: json!("main"),
        max_age_sec: 5,
    }];
    assert_eq!(
        verify(
            &p,
            &json!({"instanceId":"g","observedAt":"2000-01-01T00:00:00Z","ui":{"value":"main"}}),
            "g"
        ),
        "unknown"
    );
    assert_eq!(
        verify(
            &p,
            &json!({"instanceId":"g","observedAt":now(),"ui":{"value":"main"}}),
            "g"
        ),
        "verifiedSucceeded"
    );
}

#[test]
fn restart_reconciles_existing_job_without_repeating_action() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![answer()]);
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("test.db");
    let journal = Journal::open(&db).unwrap();
    let mut run = journal
        .create("recover existing action", "c", "key", 1800, None)
        .unwrap();
    journal.save(&mut run, RunState::Deciding).unwrap();
    journal.save(&mut run, RunState::Executing).unwrap();
    let mut attempt = journal
        .prepare(
            &run,
            "old-call",
            json!({"methodId":"mock.success","arguments":{}}),
            "mock-bgi",
        )
        .unwrap();
    assert!(journal.acquire(&attempt).unwrap());
    let mut response = ureq::post(format!("{}/bridge/v1/invoke", backend.base_url()))
        .send_json(json!({"requestId":attempt.id,"methodId":"mock.success","arguments":{}}))
        .unwrap();
    let accepted: Value = response.body_mut().read_json().unwrap();
    attempt.job_id = accepted["jobId"].as_str().map(str::to_owned);
    attempt.outcome = "running".into();
    journal.attempt(&attempt).unwrap();
    journal.save(&mut run, RunState::WaitingJob).unwrap();
    drop(journal);
    let c = controller(&backend, &d);
    let terminal = wait(&c, &run.id, &["succeeded", "needsReview", "failed"]);
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(backend.job_count(), 1);
}

#[test]
fn deterministic_plan_executes_two_steps_with_one_planning_response() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![call("plan.update",json!({"goal":"two actions","steps":[{"id":"a","title":"first","capabilityId":"mock.success","arguments":{}},{"id":"b","title":"second","capabilityId":"mock.success","arguments":{},"dependsOn":["a"]}]})),answer()]);
    let d = tempfile::tempdir().unwrap();
    let c = approval_controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"two actions","clientKey":"r"}),
    );
    approve_pending(&c, &run);
    // Wait for the first authorization to be consumed before approving the second.
    for _ in 0..200 {
        if backend.job_count() == 1 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    approve_pending(&c, &run);
    let terminal = wait(
        &c,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(terminal["decisions"], 2);
    assert_eq!(backend.job_count(), 2);
}

#[test]
fn generic_plan_can_bind_a_discovered_tool_without_domain_capability_fields() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "plan.update",
            json!({"goal":"observe","steps":[{"id":"state","title":"read state","tool":"bgi.state.get","arguments":{}}]}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"observe","clientKey":"generic-plan"}),
    );
    let terminal = wait(
        &c,
        run["id"].as_str().unwrap(),
        &["answered", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    assert_eq!(terminal["decisions"], 2);
    let checkpoint = ipc(&c, "run.checkpoint", json!({"id":run["id"]}));
    assert_eq!(checkpoint["planRevision"], 1);
    let workflow = ipc(
        &c,
        "workflow.extract",
        json!({"runId":run["id"],"name":"observe again"}),
    );
    let repeated = ipc(
        &c,
        "workflow.run",
        json!({"id":workflow["id"],"clientKey":"generic-workflow"}),
    );
    let repeated = wait(
        &c,
        repeated["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(repeated["state"], "succeeded");
    assert_eq!(repeated["decisions"], 0);
    assert_eq!(repeated["inputTokens"], 0);
    assert_eq!(repeated["outputTokens"], 0);
}

#[test]
fn verified_plan_can_run_again_without_a_model_decision() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "plan.update",
            json!({"goal":"daily route","steps":[{"id":"a","title":"first","capabilityId":"mock.success","arguments":{}}]}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = approval_controller(&backend, &d);
    let original = ipc(
        &c,
        "run.submit",
        json!({"prompt":"daily route","clientKey":"original"}),
    );
    approve_pending(&c, &original);
    let terminal = wait(
        &c,
        original["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "succeeded");
    let strategy = ipc(
        &c,
        "strategy.extract",
        json!({"runId":original["id"],"name":"daily route"}),
    );
    let manual = ipc(
        &c,
        "strategy.run",
        json!({"id":strategy["id"],"clientKey":"manual"}),
    );
    approve_pending(&c, &manual);
    let repeated = wait(
        &c,
        manual["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(repeated["state"], "succeeded");
    assert_eq!(repeated["decisions"], 0);
    assert_eq!(repeated["inputTokens"], 0);
    assert_eq!(repeated["outputTokens"], 0);
    assert_eq!(backend.job_count(), 2);
}

#[test]
fn model_switch_preserves_existing_cancellation_registry() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![call("user.ask", json!({"question":"choose"}))]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"test","clientKey":"switch"}),
    );
    let id = run["id"].as_str().unwrap();
    wait(&c, id, &["awaitingUser"]);
    ipc(
        &c,
        "model.save",
        json!({"model":{"id":"other","name":"Other","protocol":"openai-chat","model":"mock","baseUrl":format!("{}/v1",backend.base_url()),"apiKey":""}}),
    );
    ipc(&c, "model.use", json!({"id":"other"}));
    ipc(&c, "run.cancel", json!({"id":id}));
    assert_eq!(wait(&c, id, &["cancelled"])["state"], "cancelled");
}

#[test]
fn model_fallback_occurs_only_before_any_stream_output() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![answer()]);
    let d = tempfile::tempdir().unwrap();
    fs::create_dir_all(d.path().join("capabilities")).unwrap();
    let path = d.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":2,"activeModel":"unavailable",
            "models":[
                {"id":"unavailable","name":"Unavailable","protocol":"openai-responses","model":"bad","baseUrl":"http://127.0.0.1:1/v1","options":{"timeoutMs":1000}},
                {"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock","baseUrl":format!("{}/v1",backend.base_url()),"options":{"timeoutMs":2000}}
            ],
            "agent":{"systemPrompt":"test","fallbackModels":["mock"]},
            "bridge":{"enabled":false,"baseUrl":backend.base_url()},
            "storage":{"database":d.path().join("test.db")}
        }))
        .unwrap(),
    )
    .unwrap();
    let controller = Arc::new(AppController::load(path).unwrap());
    let run = ipc(
        &controller,
        "run.submit",
        json!({"prompt":"fallback","clientKey":"fallback"}),
    );
    let terminal = wait(
        &controller,
        run["id"].as_str().unwrap(),
        &["answered", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    let events = ipc(
        &controller,
        "events.read",
        json!({"conversationId":run["conversationId"],"after":0}),
    );
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "model.fallback")
    );
}

#[test]
fn truncated_model_output_continues_instead_of_failing_the_run() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![truncated("先说一半"), answer()]);
    let directory = tempfile::tempdir().unwrap();
    let app = controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"continue please","clientKey":"truncate"}),
    );
    let terminal = wait(
        &app,
        run["id"].as_str().unwrap(),
        &["answered", "partial", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    let events = ipc(
        &app,
        "events.read",
        json!({"conversationId":run["conversationId"],"after":0}),
    );
    assert!(
        events["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["kind"] == "output.truncated"),
        "{events}"
    );
    app.shutdown();
}

#[test]
fn rejected_approval_returns_to_the_model_instead_of_failing_the_run() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("bgi.api.describe", json!({"methodId":"mock.config"})),
        call(
            "bgi.api.invoke",
            json!({"methodId":"mock.config","arguments":{}}),
        ),
        answer(),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let app = approval_controller(&backend, &directory);
    let run = ipc(
        &app,
        "run.submit",
        json!({"prompt":"change test configuration","clientKey":"reject-write"}),
    );
    let id = run["id"].as_str().unwrap();
    wait(&app, id, &["awaitingApproval"]);
    for _ in 0..100 {
        let events = ipc(
            &app,
            "events.read",
            json!({"conversationId":run["conversationId"],"after":0}),
        );
        if let Some(event) = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|event| event["kind"] == "approval.requested")
        {
            ipc(
                &app,
                "approval.respond",
                json!({"id":event["data"]["id"],"approved":false}),
            );
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let terminal = wait(&app, id, &["answered", "failed", "needsReview"]);
    assert_eq!(terminal["state"], "answered");
    assert_eq!(backend.job_count(), 0);
    app.shutdown();
}

#[test]
fn disabled_skill_cannot_be_read_by_an_existing_run() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call("user.ask", json!({"question":"choose"})),
        call("skills.read", json!({"name":"secret"})),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let directory = d.path().join("skills/secret");
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("SKILL.md"),
        "---\nname: secret\ndescription: unrelated\n---\nPRIVATE_SKILL_CONTENT",
    )
    .unwrap();
    let path = d.path().join("config.json");
    let mut raw: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    raw["agent"]["skillDirectories"] = json!([d.path().join("skills")]);
    fs::write(path, raw.to_string()).unwrap();
    ipc(&c, "extensions.reload", json!({}));
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"test","clientKey":"skill"}),
    );
    let id = run["id"].as_str().unwrap();
    wait(&c, id, &["awaitingUser"]);
    ipc(
        &c,
        "skill.setEnabled",
        json!({"name":"secret","enabled":false}),
    );
    ipc(&c, "run.input", json!({"id":id,"content":"continue"}));
    wait(&c, id, &["answered"]);
    let history = ipc(&c, "conversation.get", json!({"id":run["conversationId"]}));
    assert!(!history.to_string().contains("PRIVATE_SKILL_CONTENT"));
}

/// GEN-01 / FLOW-01：模型生成并保存快捷任务，运行时一个模型请求都不发。
///
/// 「生成」阶段可以用模型和只读工具，「运行」阶段只解释已发布的定义。两边的
/// 模型请求数分别计数 —— 界面上的「不调用模型」标签不能当证据。
#[test]
fn saved_task_runs_without_a_single_model_request() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "task.save",
            json!({
                "name":"观察状态",
                "description":"读取一次 BetterGI 状态并记录耗时。",
                "nodes":[{
                    "kind":"sequence","id":"steps","title":"观察",
                    "nodes":[{
                        "kind":"tool","id":"state","title":"读取状态",
                        "tool":"bgi.state.get","arguments":{}
                    }]
                }]
            }),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"做一个快捷任务，以后点一下读一次状态，不要现在运行","clientKey":"gen"}),
    );
    let terminal = wait(
        &c,
        run["id"].as_str().unwrap(),
        &["answered", "failed", "needsReview"],
    );
    assert_eq!(terminal["state"], "answered");
    let list = ipc(&c, "workflow.list", json!({}));
    let task = list
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["name"] == "观察状态")
        .expect("任务应当已保存");
    assert_eq!(task["state"], "readyUnverified");
    assert_eq!(task["actionLabel"], "运行");
    assert_eq!(task["zeroToken"], true);
    // 生成阶段只保存，没有真的去读状态：这次运行除保存外没有任何执行证据。
    let checkpoint = ipc(&c, "run.checkpoint", json!({"id":run["id"]}));
    assert_eq!(
        checkpoint["completedSteps"].as_array().unwrap().len(),
        0,
        "生成阶段不应产生任何已完成的执行步骤"
    );
    assert_eq!(checkpoint["evidenceRefs"].as_array().unwrap().len(), 0);

    let generation_requests = backend.model_requests();
    assert!(generation_requests > 0, "生成阶段本来就要用模型");

    backend.reset_model_requests();
    let first = ipc(
        &c,
        "workflow.run",
        json!({"id":task["id"],"clientKey":"run-1"}),
    );
    let first = wait(
        &c,
        first["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(first["state"], "succeeded");
    assert_eq!(first["decisions"], 0);
    assert_eq!(first["inputTokens"], 0);
    assert_eq!(first["outputTokens"], 0);

    // 第二次点击是新的 Run，但仍是同一个定义版本，且依然零模型请求。
    let second = ipc(
        &c,
        "workflow.run",
        json!({"id":task["id"],"clientKey":"run-2"}),
    );
    assert_ne!(first["id"], second["id"]);
    let second = wait(
        &c,
        second["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview"],
    );
    assert_eq!(second["state"], "succeeded");
    assert_eq!(second["toolCalls"], 1);
    assert_eq!(
        backend.model_requests(),
        0,
        "快捷任务运行时不得向模型网关发出任何请求"
    );
}

/// TASK-06：过期版本不暗跑。
#[test]
fn stale_expected_version_refuses_instead_of_running_something_else() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "task.save",
            json!({
                "name":"读状态",
                "description":"读取一次状态。",
                "nodes":[{"kind":"tool","id":"s","title":"读","tool":"bgi.state.get","arguments":{}}]
            }),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"做个任务","clientKey":"g"}),
    );
    wait(&c, run["id"].as_str().unwrap(), &["answered"]);
    let list = ipc(&c, "workflow.list", json!({}));
    let id = list[0]["id"].as_str().unwrap();
    let error = c
        .handle(
            "workflow.run",
            json!({"id":id,"expectedPublishedRevision":99,"clientKey":"stale"}),
            Arc::new(|_, _| {}),
        )
        .unwrap_err();
    assert!(error.to_string().contains("第 1 版"), "{error}");
}

/// GEN-11 / GEN-08：非法定义不发布、不执行，并且报出具体节点。
#[test]
fn invalid_task_is_rejected_with_the_offending_node() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "task.save",
            json!({
                "name":"缺分支",
                "description":"条件没有 unknown 分支。",
                "nodes":[{
                    "kind":"condition","id":"branch","title":"判断",
                    "condition":{"kind":"always","value":true},
                    "then":[],"otherwise":[],"unknown":[]
                }]
            }),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"做个任务","clientKey":"bad"}),
    );
    wait(&c, run["id"].as_str().unwrap(), &["answered"]);
    let list = ipc(&c, "workflow.list", json!({}));
    let task = &list[0];
    assert_eq!(task["state"], "draft");
    assert_eq!(task["runnable"], false);
    assert!(
        task["issue"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown"),
        "{}",
        task["issue"]
    );
}

/// RUN-14：游标早于该会话仍保留的事件时，要求重取快照而不是静默漏掉终态。
#[test]
fn stale_event_cursor_asks_for_a_snapshot() {
    let (_d, j) = journal();
    let early = j.create("first", "a", "k1", 1800, None).unwrap();
    for _ in 0..3 {
        j.emit(&early, "noise", json!({})).unwrap();
    }
    let late = j.create("second", "b", "k2", 1800, None).unwrap();
    assert!(late.message_boundary > 0);

    // 会话 a 的事件仍在窗口里，从 0 或 1 续读都安全。
    assert!(!j.cursor_expired("a", 0).unwrap());
    assert!(!j.cursor_expired("a", 1).unwrap());
    // 会话 b 最早的事件晚于游标 1：中间那段已经读不回来了。
    let earliest = j.events("b", 0).unwrap()[0].sequence;
    assert!(earliest > 1);
    assert!(j.cursor_expired("b", 1).unwrap());
    assert!(!j.cursor_expired("b", earliest).unwrap());
    // 游标 0 表示「从头开始」，永远不需要快照。
    assert!(!j.cursor_expired("b", 0).unwrap());
    assert_eq!(j.events("b", 0).unwrap().len(), 1);
    assert_eq!(j.events("a", 0).unwrap()[0].run_id, early.id);
}

/// PERM-01：普通写入不再逐项弹确认。
#[test]
fn ordinary_write_runs_without_an_approval_round_trip() {
    use sleepy_doll::extension::{RiskLevel, ToolEffect, UnattendedPolicy};
    use sleepy_doll::runtime::operation::permissions::{
        ChangeScope, PermissionDecision, PermissionEngine, PermissionMode, PermissionRequest,
    };

    let request = PermissionRequest {
        provider_id: "core:bgi",
        resource_ids: &[],
        resource_kinds: &[],
        effect: ToolEffect::LocalWrite,
        risk: RiskLevel::High,
        unattended: UnattendedPolicy::Forbidden,
        scope: Some(ChangeScope::fields(1)),
    };
    assert_eq!(
        PermissionEngine::decide(PermissionMode::default(), &request, &[]),
        PermissionDecision::Allow
    );
    let bulk = PermissionRequest {
        scope: Some(ChangeScope {
            fields: 12,
            objects: 3,
            ..ChangeScope::default()
        }),
        ..request
    };
    assert_eq!(
        PermissionEngine::decide(PermissionMode::default(), &bulk, &[]),
        PermissionDecision::Ask
    );
}

/// 完全控制：宿主动作不再弹审批，直接执行。
#[test]
fn full_access_runs_host_actions_without_an_approval() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        call(
            "bgi.capability.describe",
            json!({"methodId":"mock.success"}),
        ),
        call(
            "bgi.capability.invoke",
            json!({"methodId":"mock.success","arguments":{}}),
        ),
        answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = full_access_controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"execute","clientKey":"full-access"}),
    );
    // 没有 approve_pending：完全控制下不该出现任何等待审批的中间状态。
    let terminal = wait(
        &c,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "needsReview", "awaitingApproval"],
    );
    assert_ne!(
        terminal["state"], "awaitingApproval",
        "完全控制下不应要求确认"
    );
    assert_eq!(terminal["state"], "succeeded");
}
