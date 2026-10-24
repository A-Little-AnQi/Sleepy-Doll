//! 显式授权下的真实 GLM、真实只读目录测试。任何执行和窗口调用在 hook 中拦截。
use super::*;
use crate::runtime::host::hooks::{HookEventKind, HookOutcome};

#[tokio::test]
#[ignore = "需显式模型配置和报告路径；真实模型与只读宿主目录，执行前 hook 拦截"]
async fn live_glm_search_batches_with_execution_intercept() {
    let config_path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("model config");
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    crate::config::expand_env(&mut config).unwrap();
    let requested = std::env::var("SLEEPY_DOLL_TEST_MODEL").expect("model id");
    let model: ModelConfig = serde_json::from_value(
        config["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == requested)
            .expect("configured GLM")
            .clone(),
    )
    .unwrap();
    assert!(model.model.to_lowercase().contains("glm"));
    let bridge_config: crate::config::BridgeConfig =
        serde_json::from_value(config["bridge"].clone()).unwrap();
    let client = Arc::new(crate::bridge::BgiClient::new(bridge_config.clone()));
    // 只读连通性检查；不启动、重连、注入或打开任何窗口。
    let probe_client = client.clone();
    let native_catalog = tokio::task::spawn_blocking(move || probe_client.catalog("兑换码"))
        .await
        .unwrap()
        .expect("当前宿主目录必须可只读查询");
    assert!(native_catalog["total"].as_u64().unwrap_or(0) > 0);
    let report_path =
        std::path::PathBuf::from(std::env::var("SLEEPY_DOLL_BENCH_REPORT").expect("report path"));
    let mut rows = Vec::new();
    for (case, prompt, expected) in [
        (
            "parallel_all",
            "用批量检索同时从当前 BetterGI 原生接口目录和离线功能索引查找兑换码入口，必须等两边结果都回来再汇总候选，选择合适的并发数。只查接口，不执行任务、不打开窗口。",
            "all",
        ),
        (
            "first_hit",
            "用批量检索并行搜索当前 BetterGI 原生目录和离线功能索引，找到第一个明确的兑换码接口候选就继续，不必等待另一个来源；列出候选即可，不执行任务或打开窗口。",
            "firstHit",
        ),
        (
            "conditional",
            "用一批条件检索按顺序查询：先在 BGI 功能索引搜索 qzxv9f45c8d0728291qzxv，只有明确为空才在同一索引搜索兑换码。不要无条件查第二项。最后列出候选，只检索，不执行。",
            "conditional",
        ),
        (
            "redeem_intercept",
            "帮我兑换下国服兑换码，今天的",
            "intercept",
        ),
    ] {
        if std::env::var("SLEEPY_DOLL_TEST_CASE")
            .ok()
            .is_some_and(|selected| selected != case)
        {
            continue;
        }
        let unused = ModelFixture::new(|_| panic!("Use real configured GLM"));
        let supervisor = supervisor(&unused);
        {
            let mut current = supervisor.config.write().unwrap();
            current.models[0] = model.clone();
            current.models[0].id = "fixture".into();
            current.bridge = bridge_config.clone();
            current.runtime.memory_enabled = false;
            current.runtime.max_decisions = 10;
            current.runtime.max_tools = 24;
            current.plugins.enabled.push("bgi".into());
        }
        let mut skills = crate::extension::skills::SkillRegistry::default();
        skills
            .load(&[(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/bgi/skills"),
                crate::extension::skills::SkillSource::Plugin("bgi".into()),
            )])
            .unwrap();
        for source in crate::extension::skills::EMBEDDED_SKILLS {
            skills.load_embedded(source).unwrap();
        }
        let mut tools = ToolRegistry::default();
        crate::bridge::register_tools(&mut tools, client.clone()).unwrap();
        supervisor.extensions.write().unwrap().tools = Arc::new(tools);
        supervisor.extensions.write().unwrap().skills = Arc::new(skills);
        let mut run = supervisor
            .journal
            .create(prompt, case, case, 180, None)
            .unwrap();
        let journal = supervisor.journal.clone();
        let conversation_id = run.conversation_id.clone();
        let definitions = supervisor.tools().definitions();
        let token = CancellationToken::new();
        let stop = token.clone();
        let intercepted = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = intercepted.clone();
        let armed = Arc::new(AtomicBool::new(false));
        let arm = armed.clone();
        let hooks = supervisor.hooks.read().unwrap().clone();
        hooks.register_internal(
            HookEventKind::BeforeToolUse,
            Arc::new(move |event| {
                let tool = event.data["tool"].as_str().unwrap_or("");
                // api.read 也不放行，避免 UI 查询中隐式创建窗口上下文。
                let allowed = matches!(
                    tool,
                    "tools.search"
                        | "search.batch"
                        | "skills.search"
                        | "skills.read"
                        | "skills.reference"
                        | "conversation.inputs"
                        | "plan.update"
                        | "bgi.api.search"
                        | "bgi.api.describe"
                        | "bgi.feature.search"
                        | "bgi.feature.read"
                        | "bgi.repo.search"
                        | "bgi.repo.read"
                );
                if allowed {
                    return Ok(HookOutcome::default());
                }
                // hook 早于参数检查。畸形调用应反馈修正，不提前判作已到执行节点。
                let arguments = journal.events(&conversation_id, 0)?.into_iter().rev().find_map(|e| {
                    (e.kind == "tool.started"
                        && e.data["name"] == tool
                        && json!(hash(&e.data["arguments"])) == event.data["argumentsHash"])
                        .then(|| e.data["arguments"].clone())
                });
                if let Some(arguments) = arguments {
                    let invalid_shape = definitions.iter().find(|d| d.name == tool).is_some_and(|d| {
                        !crate::extension::validate(&arguments, &d.input_schema, "$").is_empty()
                    });
                    let tool_as_method = arguments["methodId"].as_str().is_some_and(|id| {
                        definitions.iter().any(|d| d.name == id)
                    });
                    if invalid_shape {
                        // 仍由生产参数校验给出真实字段错误；该分支不会派发工具。
                        return Ok(HookOutcome::default());
                    }
                    if tool_as_method {
                        return Err(Error::Tool("methodId 指向了 Agent 工具，未指向真实宿主候选；本次未派发。请核对工具名与宿主标识的层级。".into()));
                    }
                }
                captured.lock().unwrap().push(tool.to_owned());
                if arm.load(Ordering::SeqCst) {
                    stop.cancel();
                }
                Ok(HookOutcome {
                    blocked: true,
                    reason: Some("无头检索验证：派发前拦截，不执行窗口、写入或游戏操作。".into()),
                    context: vec![],
                })
            }),
        );
        let native = host::bridge::Bridge::new(bridge_config.clone()).unwrap();
        let mut exposed = HashSet::from(["bgi.api.invoke".into()]);
        let probe = ToolCall {
            id: "guard-probe".into(),
            name: "bgi.api.invoke".into(),
            arguments: json!({"methodId":"bgi.start_game","arguments":{}}),
        };
        let policy = supervisor.config.read().unwrap().runtime.clone();
        assert!(
            supervisor
                .tool(&mut run, &probe, &native, &policy, &token, &mut exposed)
                .await
                .is_err()
        );
        assert_eq!(*intercepted.lock().unwrap(), vec!["bgi.api.invoke"]);
        intercepted.lock().unwrap().clear();
        armed.store(true, Ordering::SeqCst);
        let started = std::time::Instant::now();
        let result = supervisor.session(&mut run, &token).await;
        let elapsed = started.elapsed().as_secs_f64();
        let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
        let calls = events
            .iter()
            .filter(|e| e.kind == "tool.started")
            .map(|e| json!({"name":e.data["name"],"arguments":e.data["arguments"]}))
            .collect::<Vec<_>>();
        let batches = calls
            .iter()
            .filter(|c| c["name"] == "search.batch")
            .map(|c| c["arguments"].clone())
            .collect::<Vec<_>>();
        let native_call_ids = events
            .iter()
            .filter(|e| e.kind == "tool.started" && e.data["name"] == "bgi.api.search")
            .filter_map(|e| e.data["id"].as_str().map(str::to_owned))
            .collect::<HashSet<_>>();
        let known_native = events
            .iter()
            .filter(|e| {
                e.kind == "tool.completed"
                    && e.data["callId"]
                        .as_str()
                        .is_some_and(|id| native_call_ids.contains(id))
            })
            .any(|e| {
                let data = e.data.to_string();
                data.contains("cmd.feed_window.auto_redeem_item")
                    || data.contains("cmd.task_settings_page.switch_auto_redeem_code")
            });
        let blocked = intercepted.lock().unwrap().clone();
        let final_answer = run.result.clone().unwrap_or_default();
        let passed = if expected == "intercept" {
            known_native
                && blocked
                    .iter()
                    .any(|tool| matches!(tool.as_str(), "bgi.api.invoke" | "bgi.api.read"))
        } else {
            result.is_ok()
                && run.state == RunState::Answered
                && blocked.is_empty()
                && !final_answer.is_empty()
                && match expected {
                    "conditional" => batches.iter().any(|b| {
                        b["order"] == "sequential"
                            && b["requests"]
                                .as_array()
                                .is_some_and(|r| r.iter().any(|r| r["when"] == "allEmpty"))
                    }),
                    mode => batches
                        .iter()
                        .any(|b| b["completion"] == mode && b["order"] == "parallel"),
                }
        };
        let row = json!({"case":case,"passed":passed,"model":model.model,"state":run.state,"modelRequests":run.decisions,
            "toolCalls":run.tool_calls,"elapsedSec":elapsed,"guardProbeBlocked":true,"intercepted":blocked,
            "foundNativeRedemption":known_native,"contractDescriptionRead":calls.iter().any(|c|c["name"]=="bgi.api.describe"),"batches":batches,"calls":calls,"answer":final_answer,
            "error":result.err().map(|e|e.user_message()),"usage":supervisor.journal.model_usage(&run.id).unwrap(),
            "realGameActions":0,"windowOperations":0});
        println!(
            "{case}: passed={passed}, requests={}, tools={}, elapsed={elapsed:.2}s",
            run.decisions, run.tool_calls
        );
        rows.push(row);
        std::fs::write(&report_path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
        supervisor.shutdown();
    }
    assert!(!rows.is_empty(), "No matching live model case");
    assert!(
        rows.iter().all(|r| r["passed"] == true),
        "Live model search case failed; inspect isolated report"
    );
}
