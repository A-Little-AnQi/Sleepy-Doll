//! 用真实主模型检查三层技能读取；练习应用只接触隔离目录。
use super::*;

#[tokio::test]
#[ignore = "需显式指定模型配置；只使用隔离的练习应用，不连接游戏"]
async fn live_model_reads_plugin_references_and_corrects_existing_task_without_running() {
    let config_path = std::env::var("SLEEPY_DOLL_MODEL_CONFIG").expect("Specify model config");
    let mut config: Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    crate::config::expand_env(&mut config).unwrap();
    let model_name = std::env::var("SLEEPY_DOLL_TEST_MODEL").unwrap_or_default();
    let selected = config["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|model| {
            if model_name.is_empty() {
                model["id"] == config["activeModel"]
            } else {
                model["model"] == model_name || model["id"] == model_name
            }
        })
        .expect("Configured model not found");
    let mut model: ModelConfig = serde_json::from_value(selected.clone()).unwrap();
    model.id = "fixture".into();
    model.options.max_output_tokens = Some(4096);
    model.options.timeout_ms = model.options.timeout_ms.min(90_000);
    let root = crate::test_root("live-progressive-skills");
    let skill_root = root.join("skills/practice-config");
    std::fs::create_dir_all(skill_root.join("references")).unwrap();
    std::fs::write(skill_root.join("SKILL.md"), "---\nname: practice-config\ndescription: 练习工具的晨间任务查询和配置；参数含义与修改流程按需读取。\nrequiresPlugins: practice\n---\n# 练习工具\n配置晨间任务时，用 skills.reference 读取本技能 references/config.md；不要猜参数含义，不运行任务。\n").unwrap();
    std::fs::write(skill_root.join("references/config.md"), "# 晨间任务配置\n用 practice.read 取得当前值与 sha256；任务名字在 title。参数 quota 是重复次数，模式 deliveryMode=2 是只保存结果，不因调整次数改变模式。用 practice.write 写回完整配置及 expectedSha256，保留输出目录 exportFolder 和其他字段；回读核验。未要求运行时不要调用 practice.run。引用过去成功结果不能代替本轮读取。\n").unwrap();
    let target = root.join("morning.json");
    std::fs::write(
        &target,
        json!({"title":"晨间","quota":1,"deliveryMode":2,"exportFolder":"已有目录","enabled":true})
            .to_string(),
    )
    .unwrap();
    let unused = ModelFixture::new(|_| panic!("Use the configured real model"));
    let supervisor = supervisor(&unused);
    {
        let mut current = supervisor.config.write().unwrap();
        current.models[0] = model;
        current.runtime.memory_enabled = false;
        current.runtime.max_decisions = 12;
        current.plugins.enabled.push("practice".into());
    }
    let mut skills = crate::extension::skills::SkillRegistry::default();
    for text in crate::extension::skills::EMBEDDED_SKILLS {
        skills.load_embedded(text).unwrap();
    }
    skills
        .load(&[(
            root.join("skills"),
            crate::extension::skills::SkillSource::Plugin("practice".into()),
        )])
        .unwrap();
    let mut tools = ToolRegistry::default();
    let read = target.clone();
    tools.register(crate::extension::FunctionTool::new("practice.read", "读取练习工具的晨间任务当前配置与完整 SHA", json!({"type":"object","additionalProperties":false}), "plugin:practice", move |_| {
        let text = std::fs::read_to_string(&read)?;
        Ok(json!({"config":serde_json::from_str::<Value>(&text)?,"sha256":hash(&json!(text))}))
    }).with_execution(ToolExecution::read_only())).unwrap();
    let write = target.clone();
    tools.register(crate::extension::FunctionTool::new("practice.write", "按读取版本替换练习工具的晨间任务配置，不运行任务", json!({"type":"object","additionalProperties":false,"required":["config","expectedSha256"],"properties":{"config":{"type":"object"},"expectedSha256":{"type":"string"}}}), "plugin:practice", move |a| {
        let text = std::fs::read_to_string(&write)?;
        if a["expectedSha256"] != hash(&json!(text)) { return Err(Error::Stale("文件已变化，请重新读取".into())); }
        std::fs::write(&write, a["config"].to_string())?;
        Ok(json!({"saved":true,"verification":{"status":"succeeded"}}))
    }).with_execution(ToolExecution {effect:ToolEffect::LocalWrite,model_usage:crate::extension::ModelUsage::None,..Default::default()})).unwrap();
    let starts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let start = starts.clone();
    tools
        .register(
            crate::extension::FunctionTool::new(
                "practice.run",
                "运行练习工具的晨间任务",
                json!({"type":"object"}),
                "plugin:practice",
                move |_| {
                    start.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({"started":true}))
                },
            )
            .with_execution(ToolExecution {
                effect: ToolEffect::LocalWrite,
                ..Default::default()
            }),
        )
        .unwrap();
    supervisor.extensions.write().unwrap().skills = Arc::new(skills);
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    for (prompt, quota) in [
        (
            "把练习工具的晨间任务重复次数改为3，其他设置保持原样。先配好，别运行。",
            3,
        ),
        ("刚才说错了，改成5，还是别运行。", 5),
    ] {
        let mut run = supervisor
            .journal
            .create(
                prompt,
                "live-progressive",
                &format!("live-progressive-{quota}"),
                240,
                None,
            )
            .unwrap();
        supervisor
            .session(&mut run, &CancellationToken::new())
            .await
            .unwrap();
        assert!(
            matches!(run.state, RunState::Answered | RunState::Succeeded),
            "Run must finish successfully"
        );
        assert!(
            run.result
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
        );
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
        assert_eq!(
            value["quota"], quota,
            "Main model must apply the requested correction"
        );
        assert_eq!(value["exportFolder"], "已有目录");
        assert_eq!(value["deliveryMode"], 2);
        assert_eq!(value["enabled"], true);
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        let usage = supervisor.journal.model_usage(&run.id).unwrap();
        assert!(
            usage["byPurpose"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["purpose"] == "foreground")
        );
        println!(
            "Verified real-model turn: decisions={}, tools={}, state={:?}",
            run.decisions, run.tool_calls, run.state
        );
    }
    let events = supervisor.journal.events("live-progressive", 0).unwrap();
    assert!(events.iter().any(|event| event.kind == "tool.started"
        && event.data["name"] == "skills.read"
        && event.data["arguments"]["name"] == "practice-config"));
    assert!(events.iter().any(|event| event.kind == "tool.started"
        && event.data["name"] == "skills.reference"
        && event.data["arguments"]["path"] == "references/config.md"));
    supervisor.shutdown();
}
