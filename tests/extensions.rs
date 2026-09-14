use serde_json::json;
use sleepy_doll::{
    config::AppConfig, extension::ToolRegistry, extension::plugins::PluginManager,
    extension::skills::SkillRegistry,
};
use std::fs;

fn config(root: &std::path::Path) -> AppConfig {
    serde_json::from_value(json!({"version":2,"activeModel":"local","models":[{"id":"local","name":"local","protocol":"ollama-chat","model":"test","baseUrl":"http://127.0.0.1:1"}],"agent":{"systemPrompt":"test"},"bridge":{"enabled":false,"baseUrl":"http://127.0.0.1:1"},"plugins":{"directories":[root.join("plugins")]},"storage":{"database":root.join("test.db")}})).unwrap()
}
#[test]
fn local_import_update_and_removal_preserve_retired_files() {
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source");
    fs::create_dir_all(source.join(".sleepy-doll-plugin")).unwrap();
    fs::write(
        source.join(".sleepy-doll-plugin/plugin.json"),
        json!({"schemaVersion":1,"id":"sample","name":"sample","version":"1"}).to_string(),
    )
    .unwrap();
    let config = config(d.path());
    assert_eq!(
        sleepy_doll::runtime::host::installation::install(&config, &source).unwrap(),
        "sample"
    );
    assert!(
        d.path()
            .join("plugins/sample/.sleepy-doll-plugin/plugin.json")
            .exists()
    );
    fs::write(
        source.join(".sleepy-doll-plugin/plugin.json"),
        json!({"schemaVersion":1,"id":"sample","name":"sample","version":"2"}).to_string(),
    )
    .unwrap();
    sleepy_doll::runtime::host::installation::install(&config, &source).unwrap();
    assert_eq!(
        fs::read_dir(d.path().join("plugins/.retired"))
            .unwrap()
            .count(),
        1
    );
    let retired = sleepy_doll::runtime::host::installation::remove(&config, "sample").unwrap();
    assert!(retired.exists());
    assert!(!d.path().join("plugins/sample").exists());
}
#[test]
fn plugin_failure_does_not_register_partial_tools_or_expose_headers() {
    let d = tempfile::tempdir().unwrap();
    let folder = d.path().join("sample/.sleepy-doll-plugin");
    fs::create_dir_all(&folder).unwrap();
    let tool = |name| json!({"name":name,"description":"test","inputSchema":{"type":"object"},"method":"GET","url":"https://example.invalid","headers":{"Authorization":"private-test-credential"}});
    fs::write(folder.join("plugin.json"),json!({"schemaVersion":1,"id":"sample","name":"sample","version":"1","httpTools":[tool("okay"),tool("illegal/name")]}).to_string()).unwrap();
    let mut plugins = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    plugins
        .load(
            &[d.path().into()],
            &["sample".into()],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    assert!(tools.definitions().is_empty());
    assert_eq!(plugins.list()[0].status, "failed");
    assert!(
        !plugins
            .public_list()
            .to_string()
            .contains("private-test-credential")
    );
}
#[test]
fn chinese_skill_matching_and_reference_containment() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("skill");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        "---\nname: gathering\ndescription: 挖矿采集路线\n---\nRead guide.md",
    )
    .unwrap();
    fs::write(root.join("guide.md"), "guide").unwrap();
    fs::write(d.path().join("private.md"), "private").unwrap();
    let mut skills = SkillRegistry::default();
    skills.load(&[(d.path().into(), "test".into())]).unwrap();
    assert_eq!(skills.search("帮我运行采集路线", 4).len(), 1);
    assert_eq!(
        skills.read_reference("gathering", "guide.md").unwrap(),
        "guide"
    );
    assert!(skills.read_reference("gathering", "../private.md").is_err());
}

#[test]
fn skill_conditions_require_available_domain_context() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("conditional");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("SKILL.md"),
        "---\nname: conditional\ndescription: domain helper\nrequiresPlugins: domain\nrequiresCapabilities: domain.inspect\nresourceKinds: profile\nplatforms: linux, windows\nallowedTools: domain.inspect\n---\nInstructions",
    )
    .unwrap();
    let mut registry = SkillRegistry::default();
    registry.load(&[(d.path().into(), "test".into())]).unwrap();
    let skill = registry.get("conditional").unwrap();
    let plugins = std::collections::HashSet::from(["domain".into()]);
    let capabilities = std::collections::HashSet::from(["domain.inspect".into()]);
    let kinds = std::collections::HashSet::from(["profile".into()]);
    assert!(registry.eligible(
        skill,
        &sleepy_doll::extension::skills::SkillContext {
            plugins: &plugins,
            capabilities: &capabilities,
            resource_kinds: &kinds,
            providers: &std::collections::HashSet::new(),
            platform: "windows",
        },
    ));
    assert!(!registry.eligible(
        skill,
        &sleepy_doll::extension::skills::SkillContext {
            plugins: &std::collections::HashSet::new(),
            capabilities: &capabilities,
            resource_kinds: &kinds,
            providers: &std::collections::HashSet::new(),
            platform: "windows",
        },
    ));
}

#[test]
fn plugin_tool_execution_contract_is_explicit_and_fail_closed() {
    let d = tempfile::tempdir().unwrap();
    let folder = d.path().join("sample/.sleepy-doll-plugin");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("plugin.json"),
        json!({
            "schemaVersion": 1,
            "id": "sample",
            "name": "sample",
            "version": "1",
            "httpTools": [
                {"name":"safe","description":"read","inputSchema":{"type":"object"},"url":"https://example.invalid","execution":{"effect":"readOnly","concurrencySafe":true,"deferred":true,"maxResultChars":2048}},
                {"name":"unknown","description":"unknown","inputSchema":{"type":"object"},"url":"https://example.invalid"}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let mut plugins = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    plugins
        .load(
            &[d.path().into()],
            &["sample".into()],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    let definitions = tools.definitions();
    let safe = definitions
        .iter()
        .find(|tool| tool.name == "sample.http.safe")
        .unwrap();
    assert!(safe.execution.can_run_concurrently());
    assert_eq!(safe.execution.max_result_chars, 2048);
    let unknown = definitions
        .iter()
        .find(|tool| tool.name == "sample.http.unknown")
        .unwrap();
    assert!(!unknown.execution.can_run_concurrently());
}

#[test]
fn adapter_process_negotiates_protocol_and_registers_read_only_tools() {
    let d = tempfile::tempdir().unwrap();
    let folder = d.path().join("sample/.sleepy-doll-plugin");
    fs::create_dir_all(&folder).unwrap();
    let script = d.path().join("adapter.py");
    fs::write(
        &script,
        r#"import json,sys
for line in sys.stdin:
 r=json.loads(line); m=r.get('method'); p=r.get('params',{}); result={}
 if m=='initialize': result={'protocolVersion':'sleepy-adapter/1'}
 elif m=='health': result={'status':'ready'}
 elif m=='capabilities/list': result={'tools':[{'name':'inspect','description':'inspect','inputSchema':{'type':'object'},'execution':{'effect':'readOnly','concurrencySafe':True,'maxResultChars':2048,'deferred':False,'alwaysLoad':True}}]}
 elif m=='tools/call': result={'verification':{'status':'succeeded'},'value':p.get('arguments')}
 elif m=='mutations/plan': result={'plan':{'id':'plan','providerId':'sample/domain','resources':[{'resourceId':'resource','expectedVersion':'1','expectedHash':'old'}],'execution':{'effect':'localWrite','verification':'state','compensation':'snapshotRestore'},'verification':{'providerId':'sample/domain','method':'verify','input':{}},'compensation':{'restoreSnapshots':[]}},'stagedOutputs':[{'kind':'replaceResource','resourceId':'resource','expectedHash':'old','contentBase64':'bmV3'}]}
 print(json.dumps({'jsonrpc':'2.0','id':r.get('id'),'result':result}),flush=True)
"#,
    )
    .unwrap();
    fs::write(
        folder.join("plugin.json"),
        json!({
            "schemaVersion":1,"id":"sample","name":"sample","version":"1",
            "adapters":[{"id":"domain","version":"1","command":if cfg!(windows){"python"}else{"python3"},"args":[script]}]
        })
        .to_string(),
    )
    .unwrap();
    let mut plugins = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    plugins
        .load(
            &[d.path().into()],
            &["sample".into()],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    assert_eq!(plugins.adapters().len(), 1);
    let tool = tools
        .definitions()
        .into_iter()
        .find(|tool| tool.name == "sample.adapter.inspect")
        .unwrap();
    assert!(tool.execution.can_run_concurrently());
    assert_eq!(
        tools.call("sample.adapter.inspect", &json!({"x":1}))["ok"],
        true
    );
    let artifacts = sleepy_doll::runtime::store::artifacts::ArtifactStore::new(
        d.path().join("artifacts"),
        1024,
    )
    .unwrap();
    let snapshot = sleepy_doll::runtime::operation::kernel::ResourceSnapshot {
        id: "snapshot".into(),
        resource_id: "resource".into(),
        resource_version: "1".into(),
        content_artifact: "0".repeat(64),
        content_hash: "old".into(),
        metadata: serde_json::Value::Null,
        created_at: "now".into(),
    };
    let plan = plugins.adapters()[0]
        .plan(&[(snapshot, b"old".to_vec())], json!({}), &artifacts)
        .unwrap();
    let sleepy_doll::runtime::operation::kernel::StagedOutput::ReplaceResource {
        content_artifact,
        ..
    } = &plan.staged_outputs[0]
    else {
        panic!("expected replacement")
    };
    assert_eq!(artifacts.get(content_artifact).unwrap(), b"new");
}
