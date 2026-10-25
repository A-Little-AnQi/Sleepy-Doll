//! 通用网络工具：简洁契约、缓存、旧配置兼容和来源权限边界。
use super::*;

#[tokio::test]
async fn web_search_cache_is_independent_of_model_protocol_and_has_no_model_usage() {
    let fixture = ModelFixture::new(|_| panic!("search must not call the chat model"));
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0]
        .options
        .legacy_web_provider = Some(json!("disabled"));
    let run = supervisor
        .journal
        .create(
            "查兼容条件",
            "foundation-cache",
            "foundation-cache",
            60,
            None,
        )
        .unwrap();
    let full = json!({"backend":"fixture","complete":true,"untrusted":true,"verified":false,"retrievedAt":unix_now(),
        "items":[{"title":"Primary source","url":"https://docs.example.org/contract","excerpt":"A contract condition is documented here. ".repeat(1800)}],"providerSummary":""});
    let key = hash(&json!({"tool":"web.search","arguments":{"query":"contract"}}));
    let compact = supervisor
        .compact_search(&run, &json!({"query":"contract","maxTokens":256}), full)
        .unwrap();
    supervisor
        .journal
        .emit(
            &run,
            "web.result",
            json!({"requestKey":key,"result":compact}),
        )
        .unwrap();
    let mut call = ToolCall {
        id: "search".into(),
        name: "web.search".into(),
        arguments: json!({"query":"contract","maxTokens":256}),
    };
    let short = supervisor
        .web_tool(&run, &call, &CancellationToken::new())
        .await
        .unwrap();
    call.arguments["maxTokens"] = json!(4096);
    let large = supervisor
        .web_tool(&run, &call, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(large["cacheHit"], true);
    assert!(
        large["items"][0]["excerpt"].as_str().unwrap().len()
            > short["items"][0]["excerpt"].as_str().unwrap().len()
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 0);
    assert!(
        supervisor.journal.model_usage(&run.id).unwrap()["byPurpose"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    supervisor.shutdown();
}

#[test]
fn web_contracts_accept_the_actual_question_and_url_without_attestation_fields() {
    let fixture = ModelFixture::new(|_| panic!("no request"));
    let supervisor = supervisor(&fixture);
    let definitions = supervisor.definitions(&HashSet::new());
    for (name, args, required) in [
        (
            "web.search",
            json!({"query":"official documentation"}),
            json!(["query"]),
        ),
        (
            "web.fetch",
            json!({"url":"https://docs.example.org/guide"}),
            json!(["url"]),
        ),
    ] {
        let tool = definitions.iter().find(|d| d.name == name).unwrap();
        assert_eq!(tool.input_schema["required"], required);
        assert!(crate::extension::validate(&args, &tool.input_schema, "$").is_empty());
        for field in [
            "need",
            "userQuote",
            "knowledgeGap",
            "dependentAction",
            "backend",
        ] {
            assert!(tool.input_schema["properties"].get(field).is_none());
        }
    }
    supervisor.shutdown();
}

#[tokio::test]
async fn web_search_refuses_secrets_cancelled_and_expired_requests_before_network() {
    let fixture = ModelFixture::new(|_| panic!("no request"));
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0].api_key =
        Some("PRIVATE_SEARCH_CONNECTION_KEY".into());
    let mut run = supervisor
        .journal
        .create(
            "查公开资料",
            "foundation-guards",
            "foundation-guards",
            60,
            None,
        )
        .unwrap();
    let mut call = ToolCall {
        id: "secret".into(),
        name: "web.search".into(),
        arguments: json!({"query":"PRIVATE_SEARCH_CONNECTION_KEY"}),
    };
    assert!(
        supervisor
            .web_tool(&run, &call, &CancellationToken::new())
            .await
            .is_err()
    );
    call.arguments = json!({"query":"public"});
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(supervisor.web_tool(&run, &call, &cancel).await.is_err());
    run.deadline = unix_now() - 1;
    assert!(
        supervisor
            .web_tool(&run, &call, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 0);
    supervisor.shutdown();
}

#[test]
fn web_legacy_model_options_load_but_do_not_survive_model_edits() {
    let root = crate::test_root("web-legacy-config");
    let path = root.join("config.json");
    crate::config::seed(&path).unwrap();
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["models"] = json!([{"id":"legacy","name":"Legacy","model":"m","protocol":"anthropic-messages","baseUrl":"https://relay.example/v1","options":{"webProvider":"zhipu","webModel":"unavailable"}}]);
    raw["activeModel"] = json!("legacy");
    std::fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
    let config = crate::config::AppConfig::load(&path).unwrap();
    let serialized = serde_json::to_value(&config.models[0].options).unwrap();
    assert!(serialized.get("webProvider").is_none() && serialized.get("webModel").is_none());
    crate::config::AppConfig::save_model(&path,&json!({"id":"legacy","name":"Legacy","model":"m","protocol":"anthropic-messages","baseUrl":"https://relay.example/v1","apiKey":""})).unwrap();
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert!(saved["models"][0]["options"].get("webProvider").is_none());
    assert!(saved["models"][0]["options"].get("webModel").is_none());
}

#[test]
fn web_tools_are_deferred_low_priority_and_cannot_unlock_internal_mutations() {
    use crate::extension::{FunctionTool, Tool, ToolExposure};
    let fixture = ModelFixture::new(|_| panic!("no requests"));
    let supervisor = supervisor(&fixture);
    let mut exposed = HashSet::new();
    let mut definitions = supervisor.definitions(&exposed);
    definitions.push(
        FunctionTool::new(
            "fixture.write",
            "写入",
            json!({"type":"object"}),
            "core:web",
            |_| unreachable!(),
        )
        .with_execution(ToolExecution {
            effect: ToolEffect::LocalWrite,
            exposure: ToolExposure::AfterRead,
            ..ToolExecution::default()
        })
        .definition(),
    );
    let initial = tool_policy::TurnTools::new(
        &definitions,
        &[],
        &exposed,
        operation::permissions::PermissionMode::FullAccess,
        false,
    );
    assert!(
        !initial
            .select(&definitions)
            .iter()
            .any(|tool| tool.name.starts_with("web."))
    );
    let found = supervisor
        .discover_tools(&json!({"query":"web.search"}), &mut exposed)
        .unwrap();
    assert_eq!(found[0]["name"], "web.search");
    let mixed = supervisor
        .discover_tools(&json!({"query":"web.search 网页搜索"}), &mut exposed)
        .unwrap();
    assert_eq!(mixed[0]["name"], "web.search");
    let web = definitions.iter().find(|d| d.name == "web.search").unwrap();
    let mut local = web.clone();
    local.name = "local.search".into();
    local.source = "fixture:local".into();
    let ranked = discovery::search(vec![web.clone(), local], "搜索", 8);
    assert_eq!(ranked[0].name, "local.search");
    assert_eq!(
        web.execution.model_usage,
        crate::extension::ModelUsage::None
    );
    assert_eq!(web.source, "core:web");
    let mut call = context::message(Role::Assistant, "");
    call.tool_calls.push(ToolCall {
        id: "read-web".into(),
        name: "web.search".into(),
        arguments: json!({}),
    });
    let mut result = context::message(
        Role::Tool,
        json!({"ok":true,"value":{"untrusted":true,"items":[{"url":"https://docs.example.org"}]}})
            .to_string(),
    );
    result.tool_call_id = Some("read-web".into());
    let policy = tool_policy::TurnTools::new(
        &definitions,
        &[call, result],
        &HashSet::new(),
        operation::permissions::PermissionMode::FullAccess,
        false,
    );
    let save = definitions
        .iter()
        .find(|d| d.name == "fixture.write")
        .unwrap();
    assert!(!policy.allows(save));
    supervisor.shutdown();
}
