use std::{path::PathBuf, sync::Arc};

use serde_json::{Value, json};
use sleepy_doll::{AppConfig, app::AppController};

#[test]
fn network_plugin_bootstrap_toggle_and_legacy_skill_setting() {
    let root = std::env::var_os("SLEEPY_DOLL_TEST_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/.tmp/network-plugin")
        })
        .join("bootstrap")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let config_path = root.join("config.json");
    assert!(!config_path.exists(), "验证配置必须隔离，不能覆盖已有数据");
    std::fs::write(
        &config_path,
        serde_json::to_vec(&json!({
            "version":5,"activeModel":"","models":[],
            "agent":{"systemPrompt":"","skillDirectories":[],"disabledSkills":["网络搜索"]},
            "bridge":{"enabled":false,"baseUrl":"http://127.0.0.1:1"},
            "plugins":{"directories":[PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")],"enabled":[],"disabled":["bgi","windows-scheduler"]},
            "runtime":{"memoryEnabled":false},
            "storage":{"database":root.join("journal.sqlite")}
        }))
        .unwrap(),
    )
    .unwrap();
    let app = Arc::new(AppController::load(&config_path).unwrap());
    let call = |method: &str, params: Value| app.handle(method, params, Arc::new(|_, _| {}));
    let verify = |bootstrap: &Value, enabled: bool| {
        let plugin = bootstrap["plugins"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["manifest"]["id"] == "web-search")
            .unwrap();
        assert_eq!(plugin["manifest"]["name"], "网络搜索");
        assert_eq!(plugin["configuredEnabled"], enabled);
        assert_eq!(
            plugin["status"],
            if enabled { "enabled" } else { "disabled" }
        );
        assert_eq!(plugin["bundled"], true);
        assert_eq!(plugin["host"], false);
        let names = bootstrap["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, ["create-schedule", "create-shortcut"]);
        let labels = bootstrap["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|skill| skill["displayName"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(labels, ["创建定时任务", "创建快捷任务"]);
        let tools = bootstrap["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["source"] == "plugin:web-search:native")
            .collect::<Vec<_>>();
        assert_eq!(tools.len(), if enabled { 2 } else { 0 });
    };
    verify(&call("bootstrap", json!({})).unwrap(), true);
    assert!(call("plugin.remove", json!({"id":"web-search"})).is_err());
    call(
        "plugin.setEnabled",
        json!({"id":"web-search","enabled":false}),
    )
    .unwrap();
    verify(&call("bootstrap", json!({})).unwrap(), false);
    assert!(
        !AppConfig::load(&config_path)
            .unwrap()
            .plugin_enabled("web-search")
    );
    call(
        "plugin.setEnabled",
        json!({"id":"web-search","enabled":true}),
    )
    .unwrap();
    verify(&call("bootstrap", json!({})).unwrap(), true);
    assert!(
        AppConfig::load(&config_path)
            .unwrap()
            .agent
            .disabled_skills
            .contains(&"网络搜索".into())
    );
    app.shutdown();
}
