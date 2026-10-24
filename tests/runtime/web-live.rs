//! 显式授权的真实模型验证；游戏动作与搜索派发在 hook 中拦截。
use super::*;
use crate::runtime::host::hooks::{HookEventKind, HookOutcome};

fn installed() -> (crate::config::ModelConfig, Value) {
    let path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("model config");
    let id = std::env::var("SLEEPY_DOLL_TEST_MODEL").expect("model id");
    let mut raw: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    crate::config::expand_env(&mut raw).unwrap();
    let mut model: crate::config::ModelConfig = serde_json::from_value(
        raw["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["id"] == id)
            .unwrap()
            .clone(),
    )
    .unwrap();
    model.id = "fixture".into();
    model.options.web_model = None;
    (model, raw)
}

#[tokio::test]
#[ignore = "需指定安装版模型配置与报告路径；真实模型，所有游戏动作派发前拦截"]
async fn live_web_task_knowledge_gap_is_selected_without_explicit_search_words() {
    let (model, raw) = installed();
    let fixture = ModelFixture::new(|_| panic!("use real model"));
    let supervisor = supervisor(&fixture);
    {
        let mut config = supervisor.config.write().unwrap();
        config.models[0] = model;
        config.runtime.memory_enabled = false;
        config.runtime.max_decisions = 16;
        config.runtime.max_tools = 32;
        config.bridge = serde_json::from_value(raw["bridge"].clone()).unwrap();
        config.plugins.enabled.push("bgi".into());
    }
    let mut skills = crate::extension::skills::SkillRegistry::default();
    skills
        .load(&[(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/bgi/skills"),
            crate::extension::skills::SkillSource::Plugin("bgi".into()),
        )])
        .unwrap();
    for embedded in crate::extension::skills::EMBEDDED_SKILLS {
        skills.load_embedded(embedded).unwrap();
    }
    let mut tools = ToolRegistry::default();
    crate::bridge::register_tools(
        &mut tools,
        Arc::new(crate::bridge::BgiClient::new(
            supervisor.config.read().unwrap().bridge.clone(),
        )),
    )
    .unwrap();
    let reads = tools
        .definitions()
        .into_iter()
        .filter(|definition| {
            definition.execution.effect == ToolEffect::ReadOnly
                && !definition.execution.effect_is_dynamic
                && definition.name != "bgi.api.read"
        })
        .map(|definition| definition.name)
        .collect::<HashSet<_>>();
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    let prompt = "帮我采集一下莉奈娅的材料";
    let mut run = supervisor
        .journal
        .create(
            prompt,
            "live-web-materials",
            "live-web-materials",
            240,
            None,
        )
        .unwrap();
    let token = CancellationToken::new();
    let stop = token.clone();
    let journal = supervisor.journal.clone();
    let conversation = run.conversation_id.clone();
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = observed.clone();
    supervisor.hooks.read().unwrap().register_internal(
        HookEventKind::BeforeToolUse,
        Arc::new(move |event| {
            let name = event.data["tool"].as_str().unwrap_or("");
            if name == "web.search" {
                if let Some(call) = journal
                    .events(&conversation, 0)?
                    .into_iter()
                    .rev()
                    .find(|e| {
                        e.kind == "tool.started"
                            && e.data["name"] == name
                            && json!(hash(&e.data["arguments"])) == event.data["argumentsHash"]
                    })
                {
                    capture.lock().unwrap().push(call.data);
                }
                stop.cancel();
                return Ok(HookOutcome {
                    blocked: true,
                    reason: Some(
                        "无头验证：已观察到任务知识缺口检索，搜索尚未派发，不执行采集。".into(),
                    ),
                    context: vec![],
                });
            }
            let allowed = reads.contains(name)
                || matches!(
                    name,
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
                        | "bgi.capability.search"
                );
            if allowed {
                Ok(HookOutcome::default())
            } else {
                capture.lock().unwrap().push(json!({"blocked":name}));
                stop.cancel();
                Ok(HookOutcome {
                    blocked: true,
                    reason: Some("无头验证：执行前拦截，未操作游戏或窗口。".into()),
                    context: vec![],
                })
            }
        }),
    );
    let result = supervisor.session(&mut run, &token).await;
    let observed = observed.lock().unwrap().clone();
    let passed = observed.iter().any(|call| {
        call["name"] == "web.search"
            && call["arguments"]["need"] == "taskPrerequisite"
            && call["arguments"]["knowledgeGap"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            && call["arguments"]["dependentAction"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
    });
    let report = json!({"passed":passed,"prompt":prompt,"modelRequests":run.decisions,"toolCalls":run.tool_calls,"observed":observed,"error":result.err().map(|e|e.user_message()),"gameActions":0,"searchRequestsDispatched":0});
    let root = std::env::var("SLEEPY_DOLL_WEB_REPORT_DIR").expect("report directory");
    std::fs::write(
        std::path::Path::new(&root).join("task-gap.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "task gap: passed={passed}, decisions={}, tools={}",
        run.decisions, run.tool_calls
    );
    supervisor.shutdown();
    assert!(
        passed,
        "model did not select prerequisite web search; inspect report"
    );
}

#[tokio::test]
#[ignore = "需指定安装版模型配置；真实公开网页读取和隔离模型提取，不启动宿主或游戏"]
async fn live_web_page_is_read_and_extracted_by_configured_auxiliary_model() {
    let (model, _) = installed();
    let fixture = ModelFixture::new(|_| panic!("use real model"));
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0] = model;
    let prompt = "读取Rust官方学习页面 https://www.rust-lang.org/learn，核对其提供的学习资料";
    let run = supervisor
        .journal
        .create(
            prompt,
            "live-web-extraction",
            "live-web-extraction",
            180,
            None,
        )
        .unwrap();
    let call = ToolCall {
        id: "live-fetch".into(),
        name: "web.fetch".into(),
        arguments: json!({"url":"https://www.rust-lang.org/learn","focus":"页面是否提供帮助安装工具链和练习Rust语法的交互式练习？只提取该资料的名称和用途，不列其他学习资源。","mode":"extract","need":"userRequested","userQuote":prompt}),
    };
    let result = supervisor
        .web_tool(&run, &call, &CancellationToken::new())
        .await;
    let passed = result.as_ref().is_ok_and(|value| {
        value["fetchVerified"] == true
            && value["extractionStatus"] == "completed"
            && value["sourceRangesValidated"] == true
            && !value["pageText"].as_str().unwrap_or("").is_empty()
    });
    let report = json!({"passed":passed,"result":result.as_ref().ok(),"error":result.as_ref().err().map(|e|e.user_message()),"usage":supervisor.journal.model_usage(&run.id).unwrap()});
    let root = std::env::var("SLEEPY_DOLL_WEB_REPORT_DIR").expect("report directory");
    std::fs::write(
        std::path::Path::new(&root).join("page-extraction.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("page extraction: passed={passed}");
    supervisor.shutdown();
    assert!(
        passed,
        "live extraction not confirmed; inspect isolated report"
    );
}
