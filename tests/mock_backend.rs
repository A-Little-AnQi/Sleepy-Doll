use std::{collections::HashMap, fs, path::Path, sync::Arc, thread, time::Duration};

use serde_json::{Value, json};
use sleepy_doll::{
    app::AppController,
    config::{ModelConfig, ModelOptions, ModelProtocol},
    model::mock::MockBackend,
    model::{Message, Model, ProtocolModel, Role},
};

fn model(base_url: &str, protocol: ModelProtocol) -> ProtocolModel {
    let base_url = if protocol == ModelProtocol::OllamaChat {
        base_url.to_owned()
    } else {
        format!("{base_url}/v1")
    };
    ProtocolModel::new(ModelConfig {
        id: "mock".into(),
        name: "Mock".into(),
        protocol,
        model: "mock-model".into(),
        base_url,
        api_key: None,
        headers: HashMap::new(),
        options: ModelOptions {
            timeout_ms: 5_000,
            ..ModelOptions::default()
        },
    })
}

#[test]
fn every_model_protocol_returns_without_tokens() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let messages = [Message {
        role: Role::User,
        content: "普通文本".into(),
        tool_call_id: None,
        tool_calls: vec![],
        reasoning: None,
    }];
    for protocol in [
        ModelProtocol::OpenaiResponses,
        ModelProtocol::OpenaiChat,
        ModelProtocol::AnthropicMessages,
        ModelProtocol::Gemini,
        ModelProtocol::OllamaChat,
    ] {
        let response = model(&backend.base_url(), protocol)
            .complete(&messages, &[])
            .unwrap();
        assert!(response.text.contains("Mock"));
        assert_eq!(response.usage.input_tokens.unwrap_or(0), 0);
        assert_eq!(response.usage.output_tokens.unwrap_or(0), 0);
    }
}

#[test]
fn mock_summaries_do_not_turn_unready_capture_into_success() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let messages = vec![
        Message {
            role: Role::User,
            content: "查看游戏状态".into(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: None,
        },
        Message {
            role: Role::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: vec![sleepy_doll::model::ToolCall {
                id: "state".into(),
                name: "bgi.state.get".into(),
                arguments: json!({}),
            }],
            reasoning: None,
        },
        Message {
            role: Role::Tool,
            content:
                json!({"ok":true,"value":{"runtime":{"captureReady":false,"windowActive":false}}})
                    .to_string(),
            tool_call_id: Some("state".into()),
            tool_calls: vec![],
            reasoning: None,
        },
    ];
    for protocol in [
        ModelProtocol::OpenaiResponses,
        ModelProtocol::OpenaiChat,
        ModelProtocol::AnthropicMessages,
        ModelProtocol::Gemini,
        ModelProtocol::OllamaChat,
    ] {
        let response = model(&backend.base_url(), protocol)
            .complete(&messages, &[])
            .unwrap();
        assert!(response.text.contains("截图未就绪"), "{}", response.text);
        assert!(!response.text.contains("截图可用"));
        assert!(response.text.contains("游戏窗口不在前台"));
    }
}

#[test]
fn responses_protocol_exposes_short_and_long_layout_scenarios() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let protocol = model(&backend.base_url(), ModelProtocol::OpenaiResponses);
    let request = |content: &str| Message {
        role: Role::User,
        content: content.into(),
        tool_call_id: None,
        tool_calls: vec![],
        reasoning: None,
    };
    let short = protocol.complete(&[request("短回复")], &[]).unwrap();
    let long = protocol.complete(&[request("长回复")], &[]).unwrap();
    assert!(short.text.chars().count() < 20);
    assert!(long.text.chars().count() > 300);
    assert!(long.text.contains("只滚动对话区域"));
    assert_eq!(long.usage.output_tokens.unwrap_or(0), 0);
}

#[test]
fn streaming_delivers_early_incremental_chunks_not_a_buffered_final_body() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_faults(sleepy_doll::model::mock::MockFaults {
        stream_chunk_delay_ms: 25,
        ..Default::default()
    });
    let text = "流式测试必须在响应结束前持续显示新的文字。".repeat(6);
    backend.set_responses(vec![json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":text}]}]})]);
    let config = serde_json::from_value(json!({"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock-model","baseUrl":format!("{}/v1",backend.base_url()),"options":{"timeoutMs":5000}})).unwrap();
    let message = Message {
        role: Role::User,
        content: "stream".into(),
        tool_call_id: None,
        tool_calls: vec![],
        reasoning: None,
    };
    let started = std::time::Instant::now();
    let mut deltas = Vec::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let response = runtime
        .block_on(sleepy_doll::runtime::gateway::complete(
            config,
            &[message],
            &[],
            &tokio_util::sync::CancellationToken::new(),
            |text| {
                deltas.push((started.elapsed(), text.to_owned()));
                Ok(())
            },
        ))
        .unwrap();
    assert_eq!(response.text, text);
    assert!(
        deltas.len() > 10,
        "expected token-sized chunks, got {}",
        deltas.len()
    );
    assert!(
        deltas[0].0 < Duration::from_millis(1000),
        "first delta arrived after {:?}",
        deltas[0].0
    );
    assert!(deltas.last().unwrap().0 - deltas[0].0 > Duration::from_millis(500));
    assert_eq!(
        deltas
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<String>(),
        text
    );
}

#[test]
fn full_agent_conversation_uses_mock_model_and_bridge() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("mock.json");
    let config = json!({
        "version":1,
        "activeModel":"mock",
        "models":[{"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock-model","baseUrl":format!("{}/v1",backend.base_url()),"headers":{},"options":{"timeoutMs":5000}}],
        "agent":{"maxTurns":6,"maxToolCallsPerTurn":4,"systemPrompt":"Offline mock agent","skillDirectories":[],"autoLoadSkills":false,"maxLoadedSkills":0,"disabledSkills":[]},
        "bridge":{"enabled":true,"baseUrl":backend.base_url(),"token":"mock-token","instanceId":"mock-bgi","timeoutMs":5000},
        "plugins":{"directories":[],"enabled":[]},
        "storage":{"database":directory.path().join("mock.db")}
    });
    fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    let controller = Arc::new(AppController::load(&config_path).unwrap());
    let ipc_controller = controller.clone();
    backend.set_ipc_handler(move |method, params| {
        ipc_controller.handle(method, params, Arc::new(|_, _| {}))
    });

    let submitted = controller
        .handle(
            "task.submit",
            json!({"prompt":"检查当前 BGI 状态，不要执行写操作"}),
            Arc::new(|_, _| {}),
        )
        .unwrap();
    let id = submitted["id"].as_str().unwrap();
    let mut completed = Value::Null;
    for _ in 0..100 {
        completed = controller
            .handle("task.get", json!({"id":id}), Arc::new(|_, _| {}))
            .unwrap();
        if completed["state"] == "answered" {
            break;
        }
        thread::sleep(Duration::from_millis(30));
    }
    assert_eq!(completed["state"], "answered");
    assert!(
        completed["result"]
            .as_str()
            .unwrap()
            .contains("没有执行任何游戏写操作")
    );

    let mut response = ureq::post(format!("{}/ipc", backend.base_url()))
        .send_json(json!({"id":"test","method":"bootstrap","params":{}}))
        .unwrap();
    let envelope: Value = response.body_mut().read_json().unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["result"]["bridge"]["connected"], true);
}

#[test]
fn all_protocols_stream_real_tool_calls_in_the_mock_scenario() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    for protocol in [
        "openai-responses",
        "openai-chat",
        "anthropic-messages",
        "gemini",
        "ollama-chat",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let base = if protocol == "ollama-chat" {
            backend.base_url()
        } else {
            format!("{}/v1", backend.base_url())
        };
        let config = json!({"version":1,"activeModel":"mock","models":[{"id":"mock","name":"Mock","protocol":protocol,"model":"mock-model","baseUrl":base,"options":{"timeoutMs":5000}}],"agent":{"systemPrompt":"test"},"bridge":{"enabled":true,"baseUrl":backend.base_url(),"token":"mock","instanceId":"mock-bgi","timeoutMs":5000},"storage":{"database":directory.path().join("test.db")}});
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let controller = Arc::new(AppController::load(&path).unwrap());
        let call = |method: &str, params: Value| {
            controller
                .handle(method, params, Arc::new(|_, _| {}))
                .unwrap()
        };
        let run = call("task.submit", json!({"prompt":"查看游戏状态"}));
        let mut completed = Value::Null;
        for _ in 0..150 {
            completed = call("task.get", json!({"id":run["id"]}));
            if ["answered", "failed", "needsReview"]
                .contains(&completed["state"].as_str().unwrap_or(""))
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(completed["state"], "answered", "{protocol}: {completed}");
        let history = call("conversation.get", json!({"id":run["conversationId"]}));
        assert!(
            history["messages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|message| message["role"] == "tool"
                    && message["content"]
                        .as_str()
                        .unwrap_or("")
                        .contains("captureReady")),
            "{protocol} did not invoke a tool"
        );
        assert!(
            completed["result"]
                .as_str()
                .unwrap_or("")
                .contains("Mock BGI"),
            "{protocol}: {completed}"
        );
        controller.shutdown();
    }
}

#[test]
fn always_loaded_bgi_manual_reaches_unrelated_user_phrasings() {
    let directory = tempfile::tempdir().unwrap();
    let skill_root = directory.path().join("skills/bgi-operator");
    fs::create_dir_all(&skill_root).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/bgi-operator/SKILL.md"),
        skill_root.join("SKILL.md"),
    )
    .unwrap();
    let mut registry = sleepy_doll::extension::skills::SkillRegistry::default();
    registry
        .load(&[(directory.path().join("skills"), "test".into())])
        .unwrap();
    let skill = registry.get("bgi-operator").unwrap();
    assert!(skill.always_load);
    for prompt in [
        "有哪些调度器",
        "跑霜仙花",
        "新建锄地配置",
        "修改截图间隔",
        "看看我的脚本",
    ] {
        assert!(
            skill.always_load,
            "BGI manual must not depend on matching phrase: {prompt}"
        );
    }
}

#[test]
fn bridge_jobs_cover_running_success_unknown_failure_and_cancel() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    for (method_id, terminal) in [
        ("mock.success", "completed"),
        ("mock.unknown", "completed"),
        ("mock.failure", "failed"),
    ] {
        let mut accepted = ureq::post(format!("{}/bridge/v1/invoke", backend.base_url()))
            .send_json(json!({"requestId":method_id,"methodId":method_id,"arguments":{}}))
            .unwrap();
        let accepted: Value = accepted.body_mut().read_json().unwrap();
        let job_id = accepted["jobId"].as_str().unwrap();
        let mut running = ureq::get(format!("{}/bridge/v1/jobs/{job_id}", backend.base_url()))
            .call()
            .unwrap();
        let running: Value = running.body_mut().read_json().unwrap();
        assert_eq!(running["state"], "running");
        thread::sleep(Duration::from_millis(270));
        let mut result = ureq::get(format!("{}/bridge/v1/jobs/{job_id}", backend.base_url()))
            .call()
            .unwrap();
        let result: Value = result.body_mut().read_json().unwrap();
        assert_eq!(result["state"], terminal);
    }

    let mut accepted = ureq::post(format!("{}/bridge/v1/invoke", backend.base_url()))
        .send_json(json!({"requestId":"cancel","methodId":"mock.success","arguments":{}}))
        .unwrap();
    let accepted: Value = accepted.body_mut().read_json().unwrap();
    let job_id = accepted["jobId"].as_str().unwrap();
    ureq::post(format!(
        "{}/bridge/v1/jobs/{job_id}/cancel",
        backend.base_url()
    ))
    .send_json(json!({}))
    .unwrap();
    let mut result = ureq::get(format!("{}/bridge/v1/jobs/{job_id}", backend.base_url()))
        .call()
        .unwrap();
    let result: Value = result.body_mut().read_json().unwrap();
    assert_eq!(result["state"], "cancelled");
}
