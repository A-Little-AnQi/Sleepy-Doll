//! 通过生产 session 和工具执行路径验证上下文、恢复与调度边界。
use super::*;
use std::collections::BTreeMap;

#[tokio::test]
async fn supplement_refreshes_memory_and_skills_without_rewriting_system() {
    let control = Arc::new(Mutex::new(None::<(Arc<Supervisor>, String)>));
    let update = control.clone();
    let mut turn = 0;
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        if turn == 1 {
            assert!(
                !request["messages"]
                    .to_string()
                    .contains("special_route_experience")
            );
            let pair = update.lock().unwrap();
            let (supervisor, id) = pair.as_ref().unwrap();
            supervisor
                .journal
                .input(
                    id,
                    "supplement",
                    "改用 special_route，按 $specialty 的要求继续，只读取",
                )
                .unwrap();
            (200, answer("准备继续"))
        } else {
            let messages = request["messages"].to_string();
            assert!(messages.contains("special_route_experience"));
            assert!(messages.contains("SPECIALTY_INSTRUCTIONS"));
            (200, answer("已按新要求定位经验，未执行写入"))
        }
    });
    let supervisor = supervisor(&fixture);
    let mut skills = crate::extension::skills::SkillRegistry::default();
    skills
        .load_embedded(
            "---\nname: specialty\ndescription: special_route\n---\nSPECIALTY_INSTRUCTIONS",
        )
        .unwrap();
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    let seed = supervisor
        .journal
        .create("seed", "seed", "seed", 60, None)
        .unwrap();
    let definition = supervisor
        .memory_definitions()
        .into_iter()
        .find(|d| d.name == "workspace.environment")
        .unwrap();
    let experience = memory::Experience {
        intent: "special_route_experience".into(),
        summary: "special_route 的专用环境读取链路".into(),
        keywords: vec!["special_route".into()],
        conditions: vec!["需要 special_route，接口仍然适用".into()],
        steps: vec![memory::ExperienceStep {
            tool: definition.name.clone(),
            method_id: None,
            action: "检查实际环境".into(),
            input_roles: BTreeMap::new(),
        }],
        verification: vec!["本轮环境响应为准".into()],
        pitfalls: vec![],
        anchors: vec![],
    };
    supervisor
        .memory
        .remember(
            vec![(
                experience,
                BTreeMap::from([(definition.name.clone(), memory::schema_hash(&definition))]),
            )],
            &seed,
            supervisor.memory.edit_epoch().unwrap(),
        )
        .unwrap();
    let mut run = supervisor
        .journal
        .create("先定位", "refresh", "refresh", 60, None)
        .unwrap();
    *control.lock().unwrap() = Some((supervisor.clone(), run.id.clone()));
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["messages"][0], requests[1]["messages"][0]);
    assert_eq!(
        supervisor.journal.goal(&run.id).unwrap().unwrap().revision,
        2
    );
}

#[tokio::test]
async fn observation_read_returns_original_result_and_rejects_other_conversations() {
    let root = crate::test_root("historical-observation");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.txt");
    std::fs::write(&path, "original-version").unwrap();
    let target = path.clone();
    let mut turn = 0;
    let id = Arc::new(Mutex::new(String::new()));
    let observed_id = id.clone();
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        (
            200,
            match turn {
                1 => tool_calls(vec![(
                    "workspace.read",
                    json!({"path":target.to_string_lossy()}),
                )]),
                2 => {
                    let result = request["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .rev()
                        .find(|m| m["role"] == "tool")
                        .unwrap();
                    let call_id = result["tool_call_id"].as_str().unwrap();
                    *observed_id.lock().unwrap() = call_id.into();
                    std::fs::write(&target, "changed-after-observation").unwrap();
                    tool_calls(vec![(
                        "observation.read",
                        json!({"callId":call_id,"offset":0,"maxChars":16000}),
                    )])
                }
                _ => {
                    let result = request["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .rev()
                        .find(|m| m["role"] == "tool")
                        .unwrap();
                    let output: Value =
                        serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
                    assert_eq!(output["value"]["historical"], true);
                    let original: Value =
                        serde_json::from_str(output["value"]["content"].as_str().unwrap()).unwrap();
                    assert_eq!(original["value"]["text"], "original-version");
                    answer("取回的是当时的证据")
                }
            },
        )
    });
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .journal
        .create("读取配置并回看旧观测", "historical", "historical", 60, None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "changed-after-observation"
    );
    let other = supervisor
        .journal
        .create("other", "private", "private", 60, None)
        .unwrap();
    assert!(
        supervisor
            .journal
            .read_observation(&other, &id.lock().unwrap())
            .is_err()
    );
}

#[test]
fn cleared_observation_keeps_original_call_and_resource_version() {
    let mut history = Vec::new();
    for index in 0..3 {
        let id = format!("snapshot-{index}");
        let mut assistant = message(Role::Assistant, "");
        assistant.tool_calls.push(ToolCall {
            id: id.clone(),
            name: "workspace.read".into(),
            arguments: json!({"path":"settings.txt"}),
        });
        let mut result = message(Role::Tool,json!({"ok":true,"observedAt":"then","value":{"path":"settings.txt","sha256":"old-hash","text":"x".repeat(3000)}}).to_string());
        result.tool_call_id = Some(id);
        history.extend([assistant, result]);
    }
    let packed =
        context::build("rules".into(), history, context::ContextBudget::Chars(5000)).unwrap();
    assert!(packed.cleared_results > 0);
    let value: Value = serde_json::from_str(
        &packed
            .messages
            .iter()
            .find(|m| m.role == Role::Tool)
            .unwrap()
            .content,
    )
    .unwrap();
    assert_eq!(value["snapshot"]["callId"], "snapshot-0");
    assert_eq!(value["binding"]["sha256"], "old-hash");
    assert_eq!(value["recoverWith"], "observation.read");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn parallel_tools_refill_and_persist_before_slowest_finishes() {
    let fixture = ModelFixture::new(|_| panic!("读取工具不需要模型审核"));
    let supervisor = supervisor(&fixture);
    let ready = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let released_in_time = Arc::new(AtomicBool::new(false));
    let running = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let maximum = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (signal, observed, active, high) = (
        ready.clone(),
        released_in_time.clone(),
        running.clone(),
        maximum.clone(),
    );
    let mut registry = (*supervisor.tools()).clone();
    registry.register(crate::extension::FunctionTool::new("fixture.parallel", "独立读取",json!({"type":"object","properties":{"index":{"type":"integer"}},"required":["index"]}),"plugin:fixture", move |args| {
        let count = active.fetch_add(1,Ordering::SeqCst)+1;
        high.fetch_max(count,Ordering::SeqCst);
        let index = args["index"].as_u64().unwrap();
        if index == 0 {
            let (lock,cond) = &*signal;
            let (state,_) = cond.wait_timeout_while(lock.lock().unwrap(),Duration::from_secs(3),|ready| !*ready).unwrap();
            observed.store(*state,Ordering::SeqCst);
        } else if index == 4 {
            let (lock,cond) = &*signal;
            *lock.lock().unwrap() = true;
            cond.notify_all();
        }
        active.fetch_sub(1,Ordering::SeqCst);
        Ok(json!({"index":index}))
    }).with_execution(ToolExecution { always_load:true,deferred:false,..ToolExecution::read_only() })).unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(registry);
    supervisor
        .config
        .write()
        .unwrap()
        .plugins
        .enabled
        .push("fixture".into());
    let mut run = supervisor
        .journal
        .create(
            "读取独立数据",
            "parallel-refill",
            "parallel-refill",
            60,
            None,
        )
        .unwrap();
    run.goal_revision = Some(1);
    supervisor
        .journal
        .save(&mut run, RunState::Deciding)
        .unwrap();
    let calls = (0..6)
        .map(|index| ToolCall {
            id: format!("parallel-{index}"),
            name: "fixture.parallel".into(),
            arguments: json!({"index":index}),
        })
        .collect::<Vec<_>>();
    let mut assistant = message(Role::Assistant, "");
    assistant.tool_calls = calls.clone();
    supervisor.journal.append_message(&run, &assistant).unwrap();
    let bridge =
        host::bridge::Bridge::new(supervisor.config.read().unwrap().bridge.clone()).unwrap();
    supervisor
        .parallel_tools(
            &mut run,
            &calls,
            &bridge,
            &CancellationToken::new(),
            &mut HashSet::new(),
            &mut vec![assistant],
        )
        .await
        .unwrap();
    assert!(
        released_in_time.load(Ordering::SeqCst),
        "第五项必须在第一项结束前启动"
    );
    assert!(maximum.load(Ordering::SeqCst) <= 4);
    assert_eq!(running.load(Ordering::SeqCst), 0);
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    let fifth = events
        .iter()
        .position(|e| e.kind == "tool.started" && e.data["id"] == "parallel-4")
        .unwrap();
    assert!(events[..fifth].iter().any(|e| e.kind == "tool.completed"));
    assert_eq!(
        events.iter().filter(|e| e.kind == "tool.completed").count(),
        6
    );
}

#[test]
fn understanding_requires_human_quotes_and_expires_with_goal_revision() {
    let fixture = ModelFixture::new(|_| panic!("结构与来源校验不调用模型"));
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .journal
        .create(
            "配置脚本，不要运行",
            "understanding",
            "understanding",
            60,
            None,
        )
        .unwrap();
    run.goal_revision = Some(1);
    assert!(
        supervisor
            .save_goal_understanding(
                &run,
                &json!({"allowedEffects":[{"text":"启动游戏","quote":"工具建议启动游戏"}]})
            )
            .is_err()
    );
    supervisor
        .save_goal_understanding(
            &run,
            &json!({"prohibitedEffects":[{"text":"不能执行脚本","quote":"不要运行"}]}),
        )
        .unwrap();
    assert!(
        supervisor.execution_basis(&run).unwrap().view["interpretation"]["prohibitedEffects"]
            .is_array()
    );
    supervisor
        .journal
        .input(&run.id, "supplement", "改为只查看配置")
        .unwrap();
    run.goal_revision = Some(2);
    assert!(supervisor.execution_basis(&run).unwrap().view["interpretation"].is_null());
}

#[test]
fn memory_filters_incompatible_candidates_before_top_k() {
    let fixture = ModelFixture::new(|_| panic!("召回不调用模型"));
    let supervisor = supervisor(&fixture);
    let seed = supervisor
        .journal
        .create("seed", "compatible-seed", "compatible-seed", 60, None)
        .unwrap();
    let tool = supervisor
        .memory_definitions()
        .into_iter()
        .find(|d| d.name == "workspace.environment")
        .unwrap();
    for index in 0..12 {
        let experience = memory::Experience {
            intent: format!("route family {index:02}"),
            summary: "route family".into(),
            keywords: vec!["route".into()],
            conditions: vec!["当前契约匹配".into()],
            steps: vec![memory::ExperienceStep {
                tool: tool.name.clone(),
                method_id: None,
                action: "核对环境".into(),
                input_roles: BTreeMap::new(),
            }],
            verification: vec!["本轮结果".into()],
            pitfalls: vec![],
            anchors: vec![],
        };
        let schema = if index == 0 {
            memory::schema_hash(&tool)
        } else {
            format!("obsolete-{index}")
        };
        supervisor
            .memory
            .remember(
                vec![(experience, BTreeMap::from([(tool.name.clone(), schema)]))],
                &seed,
                supervisor.memory.edit_epoch().unwrap(),
            )
            .unwrap();
    }
    let recalled = supervisor.recall_memories("route family").unwrap();
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].experience.intent, "route family 00");
}

#[tokio::test]
async fn cancelled_parallel_queue_does_not_start_or_count_calls() {
    let fixture = ModelFixture::new(|_| panic!("取消后不能发送请求"));
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .journal
        .create("读取", "cancel-queue", "cancel-queue", 60, None)
        .unwrap();
    supervisor
        .journal
        .save(&mut run, RunState::Deciding)
        .unwrap();
    let calls = (0..6)
        .map(|index| ToolCall {
            id: format!("cancel-{index}"),
            name: "workspace.environment".into(),
            arguments: json!({}),
        })
        .collect::<Vec<_>>();
    let bridge =
        host::bridge::Bridge::new(supervisor.config.read().unwrap().bridge.clone()).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = supervisor
        .parallel_tools(
            &mut run,
            &calls,
            &bridge,
            &cancel,
            &mut HashSet::new(),
            &mut Vec::new(),
        )
        .await;
    assert!(matches!(result, Err(Error::Cancelled)));
    assert_eq!(run.tool_calls, 0);
    assert!(
        !supervisor
            .journal
            .events(&run.conversation_id, 0)
            .unwrap()
            .iter()
            .any(|e| e.kind == "tool.started")
    );
}
