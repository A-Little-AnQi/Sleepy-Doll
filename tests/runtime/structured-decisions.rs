//! 原始 HTTP 响应验证，不使用模拟端的正文到工具调用便捷转换。
use super::*;

fn decision_fixture_tool() -> ToolDefinition {
    ToolDefinition {
        name: "fixture.decision".into(),
        label: "协议测试".into(),
        description: "协议测试".into(),
        input_schema: json!({"type":"object","additionalProperties":false,"required":["decision"],"properties":{"decision":{"type":"string","enum":["complete","continue","reject"]}}}),
        output_schema: None,
        source: "fixture".into(),
        provider_version: None,
        execution: ToolExecution {
            effect: ToolEffect::InternalState,
            ..Default::default()
        },
    }
}

#[tokio::test]
async fn scoped_tool_arguments_are_authoritative_and_text_is_not_parsed() {
    let fixture = ModelFixture::with_headers(|request| {
        let name = gateway::wire_name("fixture.decision");
        assert_eq!(request["tool_choice"]["function"]["name"], name);
        assert_eq!(request["tools"].as_array().unwrap().len(), 1);
        let mut body = tool_calls(vec![("fixture.decision", json!({"decision":"continue"}))]);
        body["choices"][0]["message"]["content"] = json!("```json\n{\"decision\":\"reject\"}\n```");
        (200, body, String::new())
    });
    let supervisor = supervisor(&fixture);
    let run = supervisor
        .journal
        .create("核对", "typed-valid", "typed-valid", 60, None)
        .unwrap();
    let definition = decision_fixture_tool();
    let response = supervisor
        .complete_auxiliary_tool(
            &run,
            "scope",
            supervisor.model_for(&run).unwrap(),
            &[message(Role::User, "核对")],
            &definition,
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(response.tool_calls[0].name, definition.name);
    assert_eq!(response.tool_calls[0].arguments["decision"], "continue");
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    assert!(
        !supervisor
            .definitions(&HashSet::new())
            .iter()
            .any(|d| d.name == definition.name)
    );
    assert!(
        !supervisor
            .journal
            .events(&run.conversation_id, 0)
            .unwrap()
            .iter()
            .any(|e| e.kind == "tool.started")
    );
}

#[tokio::test]
async fn invalid_decisions_are_not_repaired_from_text_or_retried() {
    for variant in 0..6 {
        let fixture = ModelFixture::with_headers(move |_| {
            let body = match variant {
                0 => answer(r#"{"decision":"continue"}"#),
                1 => tool_calls(vec![(
                    "fixture.decision",
                    json!({"decision":"execute_everything"}),
                )]),
                2 => tool_calls(vec![("runtime.other", json!({"decision":"continue"}))]),
                3 => tool_calls(vec![(
                    "fixture.decision",
                    json!({"decision":"continue","extra":"untrusted"}),
                )]),
                4 => tool_calls(vec![
                    ("fixture.decision", json!({"decision":"continue"})),
                    ("fixture.decision", json!({"decision":"reject"})),
                ]),
                _ => {
                    let mut body =
                        tool_calls(vec![("fixture.decision", json!({"decision":"continue"}))]);
                    body["choices"][0]["finish_reason"] = json!("length");
                    body
                }
            };
            (200, body, String::new())
        });
        let supervisor = supervisor(&fixture);
        let run = supervisor
            .journal
            .create("核对", "typed-invalid", "typed-invalid", 60, None)
            .unwrap();
        let result = supervisor
            .complete_auxiliary_tool(
                &run,
                "scope",
                supervisor.model_for(&run).unwrap(),
                &[message(Role::User, "核对")],
                &decision_fixture_tool(),
                &CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(result, Err(Error::ModelProtocol(_))),
            "variant={variant}"
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        let usage = supervisor.journal.model_usage(&run.id).unwrap();
        assert_eq!(usage["byPurpose"][0]["requests"], 1);
    }
}

#[tokio::test]
async fn tool_schema_is_included_in_auxiliary_window_budget() {
    let fixture = ModelFixture::with_headers(|_| panic!("超窗请求不应发出"));
    let supervisor = supervisor(&fixture);
    let run = supervisor
        .journal
        .create("核对", "typed-window", "typed-window", 60, None)
        .unwrap();
    let mut model = supervisor.model_for(&run).unwrap();
    model.options.context_window = 2048;
    model.options.max_output_tokens = Some(512);
    let mut tool = decision_fixture_tool();
    tool.description = "x".repeat(20_000);
    let result = supervisor
        .complete_auxiliary_tool(
            &run,
            "scope",
            model,
            &[message(Role::User, "短输入")],
            &tool,
            &CancellationToken::new(),
        )
        .await;
    assert!(matches!(result, Err(Error::Tool(_))));
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[test]
fn memory_contract_only_accepts_observed_tools_and_methods() {
    let tool = structured::memory(
        &std::collections::BTreeSet::from(["workspace.read".into()]),
        &std::collections::BTreeSet::new(),
    );
    let mut value = json!({"goalRelevant":true,"experiences":[{"intent":"读取配置","summary":"读取已定位文件","conditions":["当前契约匹配"],"steps":[{"tool":"workspace.read","methodId":null,"action":"读取"}],"verification":["本轮结果为准"]}]});
    let response = |value: Value| crate::model::ModelResponse {
        text: String::new(),
        tool_calls: vec![ToolCall {
            id: "result".into(),
            name: tool.name.clone(),
            arguments: value,
        }],
        reasoning: None,
        finish_reason: Some("tool_calls".into()),
        usage: Default::default(),
    };
    assert!(structured::validate(&response(value.clone()), &tool).is_ok());
    value["experiences"][0]["steps"][0]["tool"] = json!("invented.execute");
    assert!(structured::validate(&response(value), &tool).is_err());
}
