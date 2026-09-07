use std::{collections::HashMap, fs, sync::Arc, thread, time::Duration};

use serde_json::{Value, json};
use sleepy_doll::{
    app::AppController,
    config::{ModelConfig, ModelOptions, ModelProtocol},
    mock::MockBackend,
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
fn responses_protocol_exposes_short_and_long_layout_scenarios() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let protocol = model(&backend.base_url(), ModelProtocol::OpenaiResponses);
    let request = |content: &str| Message {
        role: Role::User,
        content: content.into(),
        tool_call_id: None,
        tool_calls: vec![],
    };
    let short = protocol.complete(&[request("短回复")], &[]).unwrap();
    let long = protocol.complete(&[request("长回复")], &[]).unwrap();
    assert!(short.text.chars().count() < 20);
    assert!(long.text.chars().count() > 300);
    assert!(long.text.contains("只滚动对话区域"));
    assert_eq!(long.usage.output_tokens.unwrap_or(0), 0);
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
