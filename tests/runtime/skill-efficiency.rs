//! 对比技能发现改动前后的真实主循环；BGI 文件工具只接触隔离 User 目录。
use super::*;

#[tokio::test]
#[ignore = "显式模型配置和报告路径；不连接真实 BetterGI 或游戏"]
async fn live_bgi_skill_efficiency() {
    let path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("model config");
    let mut config: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    crate::config::expand_env(&mut config).unwrap();
    let name = std::env::var("SLEEPY_DOLL_TEST_MODEL").expect("model name");
    let mut model: ModelConfig = serde_json::from_value(
        config["models"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["model"] == name || m["id"] == name)
            .unwrap()
            .clone(),
    )
    .unwrap();
    model.id = "fixture".into();
    model.options.max_output_tokens = Some(4096);
    model.options.timeout_ms = model.options.timeout_ms.min(90_000);
    let phase = std::env::var("SLEEPY_DOLL_BENCH_PHASE").expect("phase label");
    let report_path =
        std::path::PathBuf::from(std::env::var("SLEEPY_DOLL_BENCH_REPORT").expect("report path"));
    let mut rows = Vec::new();
    for repetition in 0..2 {
        for (case, prompt) in [
            (
                "read",
                "看下配置组「晨间采集」现在用哪个队伍，只看，不修改也不运行。",
            ),
            (
                "configure",
                "关闭「晨间采集」配置组地图追踪的自动拾取，其他参数保持原样，别运行。",
            ),
            (
                "reference",
                "只查离线源码资料：BetterGI 的 captureGameRegion 和 ImageRegion 分别是什么，有什么关系？不运行脚本。",
            ),
        ] {
            let root = crate::test_root("skill-efficiency");
            let user = root.join("User");
            std::fs::create_dir_all(user.join("ScriptGroup")).unwrap();
            std::fs::create_dir_all(user.join("AutoPathing/晨间路线")).unwrap();
            let group_path = user.join("ScriptGroup/晨间采集.json");
            let original = json!({"name":"晨间采集","index":1,"version":3,"unknownNote":"preserve-me",
                "config":{"pathingConfig":{"enabled":true,"partyName":"采集队","autoPickEnabled":true,"hurryOnAvatar":""}},
                "projects":[{"type":"Pathing","status":"Enabled","folderName":"晨间路线","name":"花朵.json"}]});
            std::fs::write(&group_path, original.to_string()).unwrap();
            std::fs::write(
                user.join("AutoPathing/晨间路线/花朵.json"),
                json!({"positions":[{"type":"path","action":"pick_around"}]}).to_string(),
            )
            .unwrap();
            // 只提供独立测试目录的位置；不提供问法、答案或操作路线。
            let native = ModelFixture::new(move |body| {
                assert!(body.is_null(), "No native write or run request is allowed");
                (200, json!({"userPath":user}))
            });
            let unused = ModelFixture::new(|_| panic!("Use configured model"));
            let supervisor = supervisor(&unused);
            {
                let mut current = supervisor.config.write().unwrap();
                current.models[0] = model.clone();
                current.runtime.memory_enabled = false;
                current.runtime.max_decisions = 12;
                current.plugins.enabled.push("bgi".into());
            }
            let mut skills = SkillRegistry::default();
            skills
                .load(&[(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/bgi/skills"),
                    crate::extension::skills::SkillSource::Plugin("bgi".into()),
                )])
                .unwrap();
            for text in crate::extension::skills::EMBEDDED_SKILLS {
                skills.load_embedded(text).unwrap();
            }
            let mut tools = ToolRegistry::default();
            let client = crate::bridge::BgiClient::for_tests(
                serde_json::from_value(
                    json!({"enabled":true,"baseUrl":native.url.trim_end_matches("/v1")}),
                )
                .unwrap(),
            );
            crate::bridge::register_tools(&mut tools, Arc::new(client)).unwrap();
            supervisor.extensions.write().unwrap().tools = Arc::new(tools);
            supervisor.extensions.write().unwrap().skills = Arc::new(skills);
            let mut run = supervisor
                .journal
                .create(prompt, "efficiency", "case", 180, None)
                .unwrap();
            let started = std::time::Instant::now();
            let result = supervisor
                .session(&mut run, &CancellationToken::new())
                .await;
            let elapsed = started.elapsed().as_secs_f64();
            let saved: Value =
                serde_json::from_str(&std::fs::read_to_string(&group_path).unwrap()).unwrap();
            let mut expected = original;
            if case == "configure" {
                expected["config"]["pathingConfig"]["autoPickEnabled"] = json!(false);
            }
            let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
            let calls = events
                .iter()
                .filter(|e| e.kind == "tool.started")
                .map(|e| json!({"name":e.data["name"],"arguments":e.data["arguments"]}))
                .collect::<Vec<_>>();
            let failures = events
                .iter()
                .filter(|e| e.kind == "tool.completed" && e.data["result"]["ok"] == false)
                .count();
            let answer = run.result.clone().unwrap_or_default();
            let correct = result.is_ok()
                && matches!(run.state, RunState::Answered | RunState::Succeeded)
                && saved == expected
                && !answer.trim().is_empty()
                && (case != "read" || answer.contains("采集队"))
                && (case != "reference"
                    || (answer.contains("ImageRegion") && answer.contains("captureGameRegion")));
            let row = json!({"case":case,"repetition":repetition,"runId":run.id,"passed":correct,"state":run.state,
                "modelRequests":run.decisions,"toolCalls":run.tool_calls,"toolFailures":failures,
                "elapsedSec":elapsed,"usage":supervisor.journal.model_usage(&run.id).unwrap(),"calls":calls,"answer":answer,
                "error":result.err().map(|e|e.to_string())});
            println!(
                "{phase} {case} #{repetition}: passed={correct}, requests={}, tools={}, elapsed={elapsed:.2}s",
                run.decisions, run.tool_calls
            );
            rows.push(row);
            let report = json!({"phase":phase,"model":name,"scope":"real Supervisor and model, production BGI file tools, isolated User; no game", "rows":rows});
            std::fs::write(&report_path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
            supervisor.shutdown();
        }
    }
    assert!(
        rows.iter().all(|row| row["passed"] == true),
        "A benchmark task failed; inspect report"
    );
}
