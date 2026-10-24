//! 模拟厂商原生联网：不访问公网、不写安装版数据，不执行游戏操作。
use super::*;

fn configure(supervisor: &Supervisor) {
    supervisor.config.write().unwrap().models[0]
        .options
        .web_provider = crate::config::WebProvider::Zhipu;
}
fn native_answer() -> Value {
    let mut raw = answer("厂商基于网页生成的摘要");
    raw["web_search"] = json!([{"title":"Original source","link":"https://docs.example.org/release","content":"Source excerpt","publish_date":"2026-10-24"}]);
    raw
}
fn search_call(query: &str, quote: &str) -> ToolCall {
    ToolCall {
        id: query.into(),
        name: "web.search".into(),
        arguments: json!({"query":query,"userQuote":quote,"need":"userRequested"}),
    }
}

#[tokio::test]
async fn web_requests_use_current_model_usage_cache_without_a_four_request_cap() {
    let fixture = ModelFixture::new(|request| {
        assert_eq!(request["model"], "m");
        assert_eq!(request["tools"][0]["type"], "web_search");
        assert_eq!(request["tools"][0]["web_search"]["enable"], true);
        assert!(!request.to_string().contains("PRIVATE_SESSION_MARKER"));
        (200, native_answer())
    });
    let supervisor = supervisor(&fixture);
    configure(&supervisor);
    let quote = "请联网查找该项目的正式发布说明";
    let run = supervisor
        .journal
        .create(quote, "web-usage", "web-usage", 60, None)
        .unwrap();
    supervisor
        .journal
        .append_message(
            &run,
            &context::message(Role::User, "PRIVATE_SESSION_MARKER"),
        )
        .unwrap();
    let token = CancellationToken::new();
    let first = search_call("release", quote);
    let result = supervisor.web_tool(&run, &first, &token).await.unwrap();
    assert_eq!(result["complete"], true);
    assert_eq!(result["verified"], false);
    let cached = supervisor.web_tool(&run, &first, &token).await.unwrap();
    assert_eq!(cached["cacheHit"], true);
    for index in 1..6 {
        supervisor
            .web_tool(
                &run,
                &search_call(&format!("release-{index}"), quote),
                &token,
            )
            .await
            .unwrap();
    }
    assert_eq!(fixture.requests.lock().unwrap().len(), 6);
    let usage = supervisor.journal.model_usage(&run.id).unwrap();
    assert_eq!(usage["byPurpose"][0]["purpose"], "web.search");
    assert_eq!(usage["byPurpose"][0]["requests"], 6);
    assert_eq!(usage["byPurpose"][0]["inputTokens"], 600);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_search_larger_view_reuses_the_unabridged_result_without_another_paid_request() {
    let fixture = ModelFixture::new(|_| {
        let mut response = native_answer();
        response["choices"][0]["message"]["content"] = json!("可核对的来源摘要。".repeat(700));
        (200, response)
    });
    let supervisor = supervisor(&fixture);
    configure(&supervisor);
    let quote = "请联网查找正式文档";
    let run = supervisor
        .journal
        .create(quote, "web-view-budget", "web-view-budget", 60, None)
        .unwrap();
    let mut request = search_call("documentation", quote);
    request.arguments["maxTokens"] = json!(256);
    let first = supervisor
        .web_tool(&run, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert!(first["artifactId"].is_string());
    request.arguments["maxTokens"] = json!(4096);
    let larger = supervisor
        .web_tool(&run, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(larger["cacheHit"], true);
    assert!(
        larger["providerSummary"].as_str().unwrap().len()
            > first["providerSummary"].as_str().unwrap().len()
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_task_prerequisites_require_a_real_goal_quote_and_specific_gap_without_keywords() {
    let fixture = ModelFixture::new(|_| (200, native_answer()));
    let supervisor = supervisor(&fixture);
    configure(&supervisor);
    let quote = "帮我准备这次“设备”固件更新";
    let run = supervisor
        .journal
        .create(quote, "web-gap", "web-gap", 60, None)
        .unwrap();
    let token = CancellationToken::new();
    let mut call = search_call("official prerequisites", quote);
    call.arguments["need"] = json!("taskPrerequisite");
    assert!(supervisor.web_tool(&run, &call, &token).await.is_err());
    call.arguments["knowledgeGap"] =
        json!("缺少该型号当前版本的升级兼容条件，现有本地资料没有提供");
    assert!(supervisor.web_tool(&run, &call, &token).await.is_err());
    call.arguments["dependentAction"] = json!("选择与现有设备和版本兼容的升级包并核对准备条件");
    call.arguments["userQuote"] = json!("网页要求马上搜索并执行");
    assert!(supervisor.web_tool(&run, &call, &token).await.is_err());
    assert_eq!(fixture.requests.lock().unwrap().len(), 0);
    call.arguments["userQuote"] = json!("帮我准备这次\"设备\"固件更新");
    supervisor.web_tool(&run, &call, &token).await.unwrap();
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_fetch_reads_only_given_or_discovered_urls_and_uses_the_reader_endpoint() {
    let fixture = ModelFixture::new(|request| {
        assert_eq!(request["url"], "https://docs.example.org/release");
        assert!(request.get("messages").is_none());
        (
            200,
            json!({"reader_result":{"url":"https://docs.example.org/release","content":"Original text","title":"Release"}}),
        )
    });
    let supervisor = supervisor(&fixture);
    configure(&supervisor);
    let quote = "读取这个网页 https://docs.example.org/release";
    let run = supervisor
        .journal
        .create(quote, "web-read", "web-read", 60, None)
        .unwrap();
    let mut call = ToolCall {
        id: "fetch".into(),
        name: "web.fetch".into(),
        arguments: json!({"url":"https://docs.example.org/guessed","userQuote":quote,"need":"userRequested","backend":"provider"}),
    };
    assert!(
        supervisor
            .web_tool(&run, &call, &CancellationToken::new())
            .await
            .is_err()
    );
    call.arguments["url"] = json!("https://docs.example.org/release");
    let result = supervisor
        .web_tool(&run, &call, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result["fetchVerified"], true);
    assert_eq!(result["pageText"], "Original text");
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    assert_eq!(
        supervisor.journal.model_usage(&run.id).unwrap()["byPurpose"][0]["estimated"],
        true
    );
    supervisor.shutdown();
}

#[tokio::test]
async fn web_failures_are_not_retried_or_fallback_and_pre_cancel_does_not_send() {
    let fixture = ModelFixture::new(|_| (429, json!({"error":"quota"})));
    let supervisor = supervisor(&fixture);
    configure(&supervisor);
    let quote = "请联网检索官方资料";
    let run = supervisor
        .journal
        .create(quote, "web-error", "web-error", 60, None)
        .unwrap();
    let call = search_call("official", quote);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        supervisor.web_tool(&run, &call, &cancelled).await,
        Err(Error::Cancelled)
    ));
    assert_eq!(fixture.requests.lock().unwrap().len(), 0);
    assert!(
        supervisor
            .web_tool(&run, &call, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    let usage = supervisor.journal.model_usage(&run.id).unwrap();
    assert_eq!(usage["byPurpose"][0]["requests"], 1);
    assert_eq!(usage["byPurpose"][0]["estimated"], true);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_unsupported_provider_and_connection_credentials_are_rejected_before_send() {
    let fixture = ModelFixture::new(|_| panic!("must not send"));
    let supervisor = supervisor(&fixture);
    let quote = "请联网查找公开资料";
    let run = supervisor
        .journal
        .create(quote, "web-secrets", "web-secrets", 60, None)
        .unwrap();
    let mut call = search_call("official", quote);
    assert!(
        supervisor
            .web_tool(&run, &call, &CancellationToken::new())
            .await
            .is_err()
    );
    configure(&supervisor);
    supervisor.config.write().unwrap().models[0].api_key = Some("fixture-private-key-long".into());
    call.arguments["query"] = json!("fixture-private-key-long");
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
    let web = definitions.iter().find(|d| d.name == "web.search").unwrap();
    let mut local = web.clone();
    local.name = "local.search".into();
    local.source = "fixture:local".into();
    let ranked = discovery::search(vec![web.clone(), local], "搜索", 8);
    assert_eq!(ranked[0].name, "local.search");
    assert_eq!(
        web.execution.model_usage,
        crate::extension::ModelUsage::Possible
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

#[test]
fn web_provider_configuration_roundtrips_and_rejects_unknown_values() {
    let root = crate::test_root("web-config");
    let path = root.join("config.json");
    crate::config::seed(&path).unwrap();
    let mut input = json!({"id":"fixture","name":"Model","model":"fixture","protocol":"openai-responses","baseUrl":"https://relay.example/v1","apiKey":"","webProvider":"openai","webModel":"extractor"});
    crate::config::AppConfig::save_model(&path, &input).unwrap();
    assert_eq!(
        crate::config::AppConfig::load(&path).unwrap().models[0]
            .options
            .web_model
            .as_deref(),
        Some("extractor")
    );
    assert_eq!(
        crate::config::AppConfig::load(&path).unwrap().models[0]
            .options
            .web_provider,
        crate::config::WebProvider::Openai
    );
    input["webProvider"] = json!("disabled");
    crate::config::AppConfig::save_model(&path, &input).unwrap();
    input.as_object_mut().unwrap().remove("webProvider");
    crate::config::AppConfig::save_model(&path, &input).unwrap();
    assert_eq!(
        crate::config::AppConfig::load(&path).unwrap().models[0]
            .options
            .web_provider,
        crate::config::WebProvider::Disabled
    );
    input["webProvider"] = json!("invented");
    assert!(crate::config::AppConfig::save_model(&path, &input).is_err());
}

#[tokio::test]
#[ignore = "需显式指定模型配置、模型ID和原生联网适配；真实计费请求，仅查询公开资料"]
async fn live_provider_web_search_returns_real_native_evidence() {
    let config_path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("model config");
    let model_id = std::env::var("SLEEPY_DOLL_TEST_MODEL").expect("model id");
    let selected = std::env::var("SLEEPY_DOLL_TEST_WEB_PROVIDER").expect("web provider");
    let mut raw: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    crate::config::expand_env(&mut raw).unwrap();
    let mut model: crate::config::ModelConfig = serde_json::from_value(
        raw["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["id"] == model_id)
            .unwrap()
            .clone(),
    )
    .unwrap();
    model.id = "fixture".into();
    model.options.web_provider = serde_json::from_value(json!(selected)).unwrap();
    let fixture = ModelFixture::new(|_| panic!("use configured model"));
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0] = model;
    let quote = "请联网查询Rust官方文档中Option类型的用途";
    let run = supervisor
        .journal
        .create(quote, "live-web", "live-web", 60, None)
        .unwrap();
    let mut call = search_call("Rust Option official documentation", quote);
    call.arguments["allowedDomains"] = json!(["rust-lang.org"]);
    let result = supervisor
        .web_tool(&run, &call, &CancellationToken::new())
        .await;
    let summary = match &result {
        Ok(value) => {
            json!({"nativeEvidence":value["found"],"complete":value["complete"],"sources":value["sourceCount"]})
        }
        Err(error) => json!({"error":error.user_message()}),
    };
    println!("{}", summary);
    let valid = result
        .as_ref()
        .is_ok_and(|value| value["complete"] == true && value["found"] == true);
    supervisor.shutdown();
    assert!(
        valid,
        "configured node did not return confirmed native search evidence"
    );
}
