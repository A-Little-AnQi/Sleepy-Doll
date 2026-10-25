//! 显式授权的真实模型验证；游戏动作与搜索派发在 hook 中拦截。
use super::*;
use crate::runtime::host::hooks::{HookEventKind, HookOutcome};

#[tokio::test]
#[ignore = "真实搜索、网页与配置模型辅助提取；只读，不调用任何游戏能力"]
async fn live_web_foundation_search_fetch_and_extract_with_legacy_relay_options() {
    let (mut model, _) = installed();
    model.options.legacy_web_provider = Some(json!("zhipu"));
    model.options.legacy_web_model = Some(json!("removed-extractor"));
    let fixture = ModelFixture::new(|_| panic!("configured real model"));
    let supervisor = supervisor(&fixture);
    supervisor.config.write().unwrap().models[0] = model;
    let run = supervisor
        .journal
        .create(
            "核对角色突破所需的采集物",
            "live-foundation",
            "live-foundation",
            240,
            None,
        )
        .unwrap();
    let token = CancellationToken::new();
    let search = supervisor
        .web_tool(
            &run,
            &ToolCall {
                id: "search".into(),
                name: "web.search".into(),
                arguments: json!({"query":"千织 突破材料","limit":6,"maxTokens":3000}),
            },
            &token,
        )
        .await
        .unwrap();
    assert_eq!(search["searchVerified"], true);
    assert_eq!(search["found"], true);
    assert!(
        supervisor.journal.model_usage(&run.id).unwrap()["byPurpose"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let rows = search["items"].as_array().unwrap();
    let selected = rows
        .iter()
        .find(|row| {
            row["url"]
                .as_str()
                .is_some_and(|url| url.contains("genshin-center.com"))
        })
        .unwrap_or(&rows[0]);
    let page=supervisor.web_tool(&run,&ToolCall{id:"fetch".into(),name:"web.fetch".into(),arguments:json!({"url":selected["url"],"focus":"只核对千织角色等级突破需要采集的地方特产名称和总数量，不扩展天赋或配队。","mode":"extract","maxTokens":3000})},&token).await.unwrap();
    assert_eq!(page["fetchVerified"], true);
    assert_eq!(page["extractionStatus"], "completed");
    assert_eq!(page["sourceRangesValidated"], true);
    assert!(page["pageText"].as_str().unwrap().contains("血斛"));
    let report = json!({"searchVerified":search["searchVerified"],"backend":search["backend"],"sources":rows.len(),
        "sourceUrl":selected["url"],"fetchVerified":page["fetchVerified"],"ranges":page["ranges"],
        "sourceRangesValidated":page["sourceRangesValidated"],"summary":page["summary"],"evidence":page["pageText"],
        "usage":supervisor.journal.model_usage(&run.id).unwrap(),"gameActions":0});
    let root = std::env::var("SLEEPY_DOLL_WEB_REPORT_DIR").expect("report directory");
    std::fs::write(
        std::path::Path::new(&root).join("end-to-end.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!(
        "real search={}, sources={}, fetch={}, extraction={}",
        search["backend"],
        rows.len(),
        page["fetchVerified"],
        page["extractionStatus"]
    );
    supervisor.shutdown();
}

fn selects_target(name: &str, args: &Value, target: &str) -> bool {
    if matches!(name, "bgi.user.resolve" | "bgi.repo.search") {
        args["query"]
            .as_str()
            .is_some_and(|query| query.contains(target))
    } else if name == "search.batch" {
        args["requests"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| {
                selects_target(
                    item["tool"].as_str().unwrap_or(""),
                    &item["arguments"],
                    target,
                )
            })
    } else {
        false
    }
}

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
    model.options.legacy_web_model = None;
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
    let prompt = std::env::var("SLEEPY_DOLL_WEB_TASK_PROMPT")
        .unwrap_or_else(|_| "帮我采集一下莉奈娅的材料".into());
    let expected_target = std::env::var("SLEEPY_DOLL_WEB_EXPECT_TARGET").ok();
    let expected_for_hook = expected_target.clone();
    let expect_no_research =
        std::env::var("SLEEPY_DOLL_WEB_EXPECT_NO_RESEARCH").is_ok_and(|value| value == "1");
    let allow_network =
        std::env::var("SLEEPY_DOLL_WEB_ALLOW_SEARCH").is_ok_and(|value| value == "1");
    let mut run = supervisor
        .journal
        .create(
            &prompt,
            "live-web-materials",
            "live-web-materials",
            if allow_network { 360 } else { 240 },
            None,
        )
        .unwrap();
    let token = CancellationToken::new();
    let stop = token.clone();
    let journal = supervisor.journal.clone();
    let conversation = run.conversation_id.clone();
    let observed = Arc::new(Mutex::new(Vec::<Value>::new()));
    let capture = observed.clone();
    let weak_supervisor = Arc::downgrade(&supervisor);
    let run_id = run.id.clone();
    supervisor.hooks.read().unwrap().register_internal(
        HookEventKind::BeforeToolUse,
        Arc::new(move |event| {
            let name = event.data["tool"].as_str().unwrap_or("");
            let mut call = journal
                .events(&conversation, 0)?
                .into_iter()
                .rev()
                .find(|e| {
                    e.kind == "tool.started"
                        && e.data["name"] == name
                        && json!(hash(&e.data["arguments"])) == event.data["argumentsHash"]
                });
            if name == "web.search"
                && let Some(call) = call.as_mut()
            {
                let validation = weak_supervisor
                    .upgrade()
                    .expect("live supervisor")
                    .authorize_web(&journal.get(&run_id)?, false, &call.data["arguments"]);
                call.data["authorizationPassed"] = json!(validation.is_ok());
                if let Err(error) = validation {
                    call.data["authorizationError"] = json!(error.user_message());
                }
            }
            let resolved = call.as_ref().is_some_and(|call| {
                expected_for_hook
                    .as_ref()
                    .is_some_and(|target| selects_target(name, &call.data["arguments"], target))
            });
            if let Some(call) = call {
                capture.lock().unwrap().push(call.data);
            }
            if (name == "web.search" && !allow_network) || resolved {
                stop.cancel();
                return Ok(HookOutcome {
                    blocked: true,
                    reason: Some(
                        "无头验证：已观察到前置事实检索或明确业务对象，派发前拦截。".into(),
                    ),
                    context: vec![],
                });
            }
            let api_searches = capture
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call["name"] == "bgi.api.search")
                .count();
            if api_searches >= 3 {
                stop.cancel();
                return Ok(HookOutcome {
                    blocked: true,
                    reason: Some("无头验证：尚未明确业务对象即反复查询接口，回归失败。".into()),
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
                        | "web.search"
                        | "web.fetch"
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
    let selected_web = observed.iter().any(|call| {
        call["name"] == "web.search"
            && call["authorizationPassed"] == true
            && call["arguments"]["query"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
    });
    let selected_target = expected_target.as_ref().is_some_and(|target| {
        observed.iter().any(|call| {
            selects_target(
                call["name"].as_str().unwrap_or(""),
                &call["arguments"],
                target,
            )
        })
    });
    let api_searches = observed
        .iter()
        .filter(|call| call["name"] == "bgi.api.search")
        .count();
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    let searches = events
        .iter()
        .filter(|e| e.kind == "web.result" && e.data["result"]["searchVerified"] == true)
        .count();
    let pages = events.iter().filter(|e| e.kind == "web.source").count();
    let texts = supervisor
        .journal
        .conversation_messages(&run.conversation_id)
        .unwrap()
        .into_iter()
        .map(|m| m.message)
        .filter(|m| m.role == Role::Assistant && !m.content.trim().is_empty())
        .map(|m| m.content)
        .collect::<Vec<_>>();
    // 检查是否得到真实资料并用于选择业务对象，不强制模型额外打开已经足够的来源。
    // 网页读取和提取由上面的真实端到端用例单独覆盖。
    let passed = if allow_network {
        searches > 0 && selected_target && api_searches < 3
    } else if expect_no_research {
        run.state == RunState::Answered
            && !observed.iter().any(|call| {
                call["name"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("web.") || name.starts_with("bgi."))
            })
    } else {
        (selected_web || selected_target) && api_searches < 3 && run.decisions <= 8
    };
    let report = json!({"passed":passed,"prompt":prompt,"expectNoResearch":expect_no_research,"selectedWeb":selected_web,"selectedTarget":selected_target,"apiSearches":api_searches,"modelRequests":run.decisions,"toolCalls":run.tool_calls,"observed":observed,"texts":texts,"error":result.err().map(|e|e.user_message()),"gameActions":0,"searchesCompleted":searches,"pagesRead":pages});
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
        "model did not resolve the prerequisite before looking for execution interfaces; inspect report"
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
        arguments: json!({"url":"https://www.rust-lang.org/learn","focus":"页面是否提供帮助安装工具链和练习Rust语法的交互式练习？只提取该资料的名称和用途，不列其他学习资源。","mode":"extract"}),
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
