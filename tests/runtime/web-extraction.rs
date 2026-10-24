use super::*;

fn source(supervisor: &Supervisor, run: &Run, text: &str) -> String {
    let id = supervisor
        .operations
        .artifacts
        .put(text.as_bytes())
        .unwrap();
    supervisor
        .operations
        .store
        .link_artifact(&id, "run", &run.id, "webSource")
        .unwrap();
    supervisor.journal.emit(run,"web.source",json!({"url":"https://docs.example.org/guide","finalUrl":"https://docs.example.org/guide","sourceId":id,"fetchedAt":unix_now(),"title":"Guide","contentType":"text/plain","links":["https://docs.example.org/reference"]})).unwrap();
    id
}
fn call(quote: &str) -> ToolCall {
    ToolCall {
        id: "web-fetch".into(),
        name: "web.fetch".into(),
        arguments: json!({"url":"https://docs.example.org/guide","userQuote":quote,"need":"userRequested","focus":"needle contract","mode":"extract"}),
    }
}
fn extracted(request: &Value) -> Value {
    let messages = request["messages"].as_array().unwrap();
    let user = messages.iter().find(|m| m["role"] == "user").unwrap()["content"]
        .as_str()
        .unwrap();
    assert!(!user.contains("PRIVATE_CONVERSATION_MARKER"));
    assert_eq!(request["tools"].as_array().unwrap().len(), 1);
    let number = regex::Regex::new(r"(?m)^(\d+): Needle contract is valid for version 7")
        .unwrap()
        .captures(user)
        .unwrap()[1]
        .parse::<usize>()
        .unwrap();
    tool_calls(vec![(
        "runtime.web_extract",
        json!({"summary":"该契约适用于版本7。","relevant":true,"ranges":[{"startLine":number,"endLine":number}]}),
    )])
}

#[tokio::test]
async fn web_extraction_isolated_model_keeps_verbatim_evidence_and_caches_per_question() {
    let fixture = ModelFixture::new(|request| (200, extracted(request)));
    let supervisor = supervisor(&fixture);
    let quote = "读取这份说明 https://docs.example.org/guide";
    let run = supervisor
        .journal
        .create(quote, "web-extraction", "web-extraction", 60, None)
        .unwrap();
    supervisor
        .journal
        .append_message(
            &run,
            &context::message(Role::User, "PRIVATE_CONVERSATION_MARKER"),
        )
        .unwrap();
    let text = format!(
        "{}\nNeedle contract is valid for version 7\n",
        (0..300)
            .map(|i| format!("unrelated boilerplate {i}\n\n"))
            .collect::<String>()
    );
    let id = source(&supervisor, &run, &text);
    let mut request = call(quote);
    request.arguments["maxInputTokens"] = json!(1024);
    let value = supervisor
        .web_tool(&run, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["extractionStatus"], "completed");
    assert_eq!(value["sourceRangesValidated"], true);
    assert_eq!(value["artifactId"], id);
    assert!(
        value["pageText"]
            .as_str()
            .unwrap()
            .contains("Needle contract is valid for version 7")
    );
    assert!(!value["pageText"].as_str().unwrap().contains("boilerplate"));
    assert_eq!(value["untrusted"], true);
    assert_eq!(value["verified"], false);
    assert_eq!(value["coverage"]["complete"], false);
    assert!(!value["sourceRangesValidated"].is_null());
    let cached = supervisor
        .web_tool(&run, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(cached["extractionCacheHit"], true);
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    let mut raw = request.clone();
    raw.arguments["mode"] = json!("raw");
    raw.arguments["sourceId"] = json!(id);
    raw.arguments["startLine"] = value["ranges"][0]["startLine"].clone();
    raw.arguments["lineCount"] = json!(1);
    let page = supervisor
        .web_tool(&run, &raw, &CancellationToken::new())
        .await
        .unwrap();
    assert!(page["pageText"].as_str().unwrap().contains("version 7"));
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    assert_eq!(
        supervisor.journal.model_usage(&run.id).unwrap()["byPurpose"][0]["purpose"],
        "web.extract"
    );
    assert_eq!(
        String::from_utf8(supervisor.operations.artifacts.get(id.as_str()).unwrap()).unwrap(),
        text
    );
    supervisor.shutdown();
}

#[tokio::test]
async fn web_extraction_rejects_invented_lines_and_does_not_cache_failed_extractions() {
    let fixture = ModelFixture::new(|_| {
        (
            200,
            tool_calls(vec![(
                "runtime.web_extract",
                json!({"summary":"unsupported claim","relevant":true,"ranges":[{"startLine":999,"endLine":999}]}),
            )]),
        )
    });
    let supervisor = supervisor(&fixture);
    let quote = "读取这份说明 https://docs.example.org/guide";
    let run = supervisor
        .journal
        .create(quote, "web-invalid-lines", "web-invalid-lines", 60, None)
        .unwrap();
    source(
        &supervisor,
        &run,
        "Needle contract is valid for version 7\n",
    );
    for _ in 0..2 {
        let value = supervisor
            .web_tool(&run, &call(quote), &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(value["extractionStatus"], "failed");
        assert_eq!(value["complete"], false);
        assert_eq!(value["pageText"], "");
        assert!(
            !value["summary"]
                .as_str()
                .unwrap()
                .contains("unsupported claim")
        );
        assert_eq!(value["fetchVerified"], true);
    }
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_extraction_uses_selected_helper_and_reuses_page_cache_across_turns() {
    let primary = ModelFixture::new(|_| panic!("primary model should not extract"));
    let helper = ModelFixture::new(|request| (200, extracted(request)));
    let supervisor = supervisor(&primary);
    {
        let mut config = supervisor.config.write().unwrap();
        let mut model = config.models[0].clone();
        model.id = "extractor".into();
        model.base_url = helper.url.clone();
        config.models[0].options.web_model = Some(model.id.clone());
        config.models.push(model);
    }
    let quote = "读取这份说明 https://docs.example.org/guide";
    let first = supervisor
        .journal
        .create(quote, "web-cross-turn", "web-cross-turn-first", 60, None)
        .unwrap();
    let id = source(
        &supervisor,
        &first,
        "Needle contract is valid for version 7\n",
    );
    supervisor
        .web_tool(&first, &call(quote), &CancellationToken::new())
        .await
        .unwrap();
    let second = supervisor
        .journal
        .create(quote, "web-cross-turn", "web-cross-turn-second", 60, None)
        .unwrap();
    let value = supervisor
        .web_tool(&second, &call(quote), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(value["pageCacheHit"], true);
    assert_eq!(value["extractionCacheHit"], true);
    assert!(
        supervisor
            .operations
            .store
            .can_read_artifact(&id, "run", &second.id)
            .unwrap()
    );
    assert_eq!(helper.requests.lock().unwrap().len(), 1);
    assert_eq!(primary.requests.lock().unwrap().len(), 0);
    supervisor.shutdown();
}

#[tokio::test]
async fn web_source_identifier_does_not_read_another_conversations_artifact() {
    let fixture = ModelFixture::new(|_| panic!("must not call model"));
    let supervisor = supervisor(&fixture);
    let quote = "读取这份说明 https://docs.example.org/guide";
    let first = supervisor
        .journal
        .create(quote, "web-private-first", "web-private-first", 60, None)
        .unwrap();
    let id = source(
        &supervisor,
        &first,
        "private source in another conversation",
    );
    let second = supervisor
        .journal
        .create(quote, "web-private-second", "web-private-second", 60, None)
        .unwrap();
    let mut request = call(quote);
    request.arguments["sourceId"] = json!(id);
    assert!(
        supervisor
            .web_tool(&second, &request, &CancellationToken::new())
            .await
            .is_err()
    );
    supervisor.shutdown();
}

#[tokio::test]
async fn web_skill_switch_blocks_network_but_raw_read_does_not_need_a_helper_model() {
    let fixture = ModelFixture::new(|_| panic!("must not call model"));
    let supervisor = supervisor(&fixture);
    let quote = "读取这份说明 https://docs.example.org/guide";
    let run = supervisor
        .journal
        .create(quote, "web-skill-switch", "web-skill-switch", 60, None)
        .unwrap();
    source(&supervisor, &run, "Original document line\n");
    let mut automatic = call(quote);
    automatic.arguments.as_object_mut().unwrap().remove("mode");
    let short = supervisor
        .web_tool(&run, &automatic, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(short["contentKind"], "pageText");
    assert_eq!(fixture.requests.lock().unwrap().len(), 0);
    supervisor.config.write().unwrap().models[0]
        .options
        .web_model = Some("missing-extractor".into());
    let mut request = call(quote);
    request.arguments["mode"] = json!("raw");
    let value = supervisor
        .web_tool(&run, &request, &CancellationToken::new())
        .await
        .unwrap();
    assert!(
        value["pageText"]
            .as_str()
            .unwrap()
            .contains("Original document line")
    );
    supervisor
        .config
        .write()
        .unwrap()
        .agent
        .disabled_skills
        .push("网络搜索".into());
    assert!(
        supervisor
            .web_tool(&run, &request, &CancellationToken::new())
            .await
            .is_err()
    );
    supervisor.shutdown();
}
