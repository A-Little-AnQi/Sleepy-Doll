//! 首轮固定开销与真实发现、调用路径的隔离回归。
use super::*;

#[tokio::test]
async fn matching_catalog_keywords_do_not_preload_skill_bodies() {
    let mut turn = 0;
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        let messages = request["messages"].to_string();
        if turn == 1 {
            assert!(messages.contains("参考手册简介"));
            assert!(!messages.contains("PRIVATE_DETAIL_MARKER"));
            (
                200,
                tool_calls(vec![("skills.read", json!({"name":"details-guide"}))]),
            )
        } else {
            assert!(messages.contains("PRIVATE_DETAIL_MARKER"));
            (200, answer("已读到本次需要的具体说明。"))
        }
    });
    let supervisor = supervisor(&fixture);
    let mut skills = crate::extension::skills::SkillRegistry::default();
    skills.load_embedded("---\nname: details-guide\ndescription: 参考手册简介\ntags: 参考手册\n---\nPRIVATE_DETAIL_MARKER：真实技能正文。\n").unwrap();
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    let mut run = supervisor
        .journal
        .create(
            "参考手册的具体说明是什么",
            "progressive-body",
            "progressive-body",
            60,
            None,
        )
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(run.tool_calls, 1);
    assert_eq!(fixture.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn context_usage_only_changes_after_a_completed_primary_response() {
    let control = Arc::new(Mutex::new(None::<(Arc<Supervisor>, String)>));
    let inspect = control.clone();
    let mut turn = 0;
    let fixture = ModelFixture::new(move |_| {
        turn += 1;
        let control = inspect.lock().unwrap();
        let (supervisor, id) = control.as_ref().unwrap();
        let inflight = supervisor.journal.get(id).unwrap();
        match turn {
            1 => {
                assert_eq!(inflight.context_tokens, 0);
                assert_eq!(inflight.context_window, 0);
                let mut response = tool_calls(vec![(
                    "tools.search",
                    json!({"query":"workspace.environment"}),
                )]);
                response["usage"] = json!({"prompt_tokens":6000,"completion_tokens":10,
                    "prompt_tokens_details":{"cached_tokens":1000}});
                (200, response)
            }
            2 => {
                assert_eq!(inflight.context_tokens, 6000);
                assert_eq!(inflight.cache_read_tokens, 1000);
                assert_eq!(inflight.context_window, 128_000);
                // 缺失 usage 时也等本次响应结束后才提交对应估算。
                (200, tool_calls(vec![("workspace.environment", json!({}))]))
            }
            3 => {
                let profiles = supervisor
                    .journal
                    .events(&inflight.conversation_id, 0)
                    .unwrap();
                let completed_estimate = profiles
                    .iter()
                    .filter(|event| event.kind == "context.request.profile")
                    .nth(1)
                    .unwrap()
                    .data["totalEstimatedTokens"]
                    .as_u64()
                    .unwrap();
                assert_eq!(inflight.context_tokens, completed_estimate);
                assert_eq!(inflight.cache_read_tokens, 0);
                let mut response = answer("已读取环境");
                response["usage"] = json!({"prompt_tokens":3000,"completion_tokens":20,
                    "prompt_tokens_details":{"cached_tokens":500}});
                (200, response)
            }
            _ => panic!("unexpected request"),
        }
    });
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0]
        .options
        .context_window = 128_000;
    let mut run = supervisor
        .journal
        .create(
            "查看环境",
            "usage-checkpoints",
            "usage-checkpoints",
            60,
            None,
        )
        .unwrap();
    *control.lock().unwrap() = Some((supervisor.clone(), run.id.clone()));
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.context_tokens, 3000);
    assert_eq!(run.cache_read_tokens, 500);
    assert_eq!(fixture.requests.lock().unwrap().len(), 3);
    *control.lock().unwrap() = None;
    supervisor.shutdown();
}

#[tokio::test]
async fn greeting_only_offers_discovery_and_does_not_load_plugin_manuals() {
    let fixture = ModelFixture::new(|request| {
        let tools = request["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 4);
        for name in [
            "tools.search",
            "skills.search",
            "skills.read",
            "skills.reference",
        ] {
            assert!(
                tools
                    .iter()
                    .any(|tool| tool["function"]["name"] == gateway::wire_name(name))
            );
        }
        assert!(
            !request["messages"]
                .to_string()
                .contains("PLUGIN_MANUAL_BODY")
        );
        assert!(!request["messages"].to_string().contains("当前执行依据"));
        (200, answer("你好！"))
    });
    let supervisor = supervisor(&fixture);
    let mut skills = crate::extension::skills::SkillRegistry::default();
    skills
        .load_mcp_instructions("bgi", "bridge", &"PLUGIN_MANUAL_BODY".repeat(200))
        .unwrap();
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    {
        let mut config = supervisor.config.write().unwrap();
        config.plugins.enabled.push("bgi".into());
    }
    let definitions = supervisor.definitions(&HashSet::new());
    let policy = tool_policy::TurnTools::new(
        &definitions,
        &[],
        &HashSet::new(),
        operation::permissions::PermissionMode::FullAccess,
        false,
    );
    let previous = definitions
        .iter()
        .filter(|d| policy.allows(d))
        .cloned()
        .collect::<Vec<_>>();
    let offered = policy.select(&definitions);
    assert!(context::estimate_tool_tokens(&offered) < context::estimate_tool_tokens(&previous) / 4);
    let mut run = supervisor
        .journal
        .create("你好", "light-greeting", "light-greeting", 60, None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(run.tool_calls, 0);
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    let profile = events
        .iter()
        .find(|event| event.kind == "context.request.profile")
        .unwrap();
    assert_eq!(profile.data["declaredTools"], 4);
    assert_eq!(
        profile.data["toolNames"],
        json!([
            "skills.read",
            "skills.reference",
            "skills.search",
            "tools.search"
        ])
    );
    assert!(profile.data["toolSchemaEstimatedTokens"].as_u64().unwrap() < 1200);
}

#[tokio::test]
async fn discovery_loads_real_plugin_contract_and_manual_before_execution() {
    let mut turn = 0;
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        (
            200,
            match turn {
                1 => tool_calls(vec![("tools.search", json!({"query":"fixture.status"}))]),
                2 => {
                    assert!(
                        request["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|t| t["function"]["name"] == gateway::wire_name("fixture.status"))
                    );
                    assert!(
                        request["messages"]
                            .to_string()
                            .contains("PLUGIN_MANUAL_BODY")
                    );
                    tool_calls(vec![("fixture.status", json!({}))])
                }
                _ => answer("已读取实际状态"),
            },
        )
    });
    let supervisor = supervisor(&fixture);
    supervisor
        .config
        .write()
        .unwrap()
        .plugins
        .enabled
        .push("fixture".into());
    let mut skills = crate::extension::skills::SkillRegistry::default();
    skills
        .load_mcp_instructions("fixture", "native", "PLUGIN_MANUAL_BODY：以当前状态为准")
        .unwrap();
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let read = count.clone();
    let mut tools = (*supervisor.tools()).clone();
    tools
        .register(
            crate::extension::FunctionTool::new(
                "fixture.status",
                "读取演示状态",
                json!({"type":"object"}),
                "plugin:fixture",
                move |_| {
                    read.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({"status":"ready"}))
                },
            )
            .with_execution(ToolExecution::read_only()),
        )
        .unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    let mut run = supervisor
        .journal
        .create("读取状态", "light-discovery", "light-discovery", 60, None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(run.tool_calls, 2);
    assert_eq!(fixture.requests.lock().unwrap().len(), 3);
}

#[test]
fn used_contracts_remain_visible_without_granting_unread_writes() {
    let fixture = ModelFixture::new(|_| panic!("无需模型请求"));
    let supervisor = supervisor(&fixture);
    let definitions = supervisor.definitions(&HashSet::new());
    let mut assistant = message(Role::Assistant, "");
    assistant.tool_calls.push(ToolCall {
        id: "known".into(),
        name: "workspace.environment".into(),
        arguments: json!({}),
    });
    let policy = tool_policy::TurnTools::new(
        &definitions,
        &[assistant],
        &HashSet::new(),
        operation::permissions::PermissionMode::PlanOnly,
        false,
    );
    let offered = policy.select(&definitions);
    assert!(offered.iter().any(|d| d.name == "workspace.environment"));
    assert!(!offered.iter().any(|d| d.name == "workspace.write"));
}
