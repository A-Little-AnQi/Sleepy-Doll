//! Opt-in smoke test against the user's dedicated BetterGI test installation.
//! No game commands are executed. Requires a running BetterGI and elevation.
use serde_json::{Value, json};
use sleepy_doll::{AppConfig, app::AppController, bridge::BgiClient};
use std::sync::Arc;

#[test]
#[ignore = "requires the dedicated BetterGI test process and administrator rights"]
fn real_bridge_switch_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    std::fs::create_dir(temp.path().join("catalog")).unwrap();
    let mut config: Value =
        serde_json::from_str(include_str!("../sleepy-doll.config.example.json")).unwrap();
    config["agent"]["skillDirectories"] = json!([]);
    config["plugins"]["directories"] = json!([]);
    config["bridge"]["token"] = Value::Null;
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    let app = Arc::new(AppController::load(&path).unwrap());
    let call =
        |method: &str, params: Value| app.handle(method, params, Arc::new(|_, _| {})).unwrap();
    assert_eq!(call("bootstrap", json!({}))["bridge"]["enabled"], false);
    call("bridge.setEnabled", json!({"enabled":true}));
    let status = call("bootstrap", json!({}));
    assert_eq!(status["bridge"]["connected"], true);
    assert!(
        status["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "bgi.state.get")
    );
    let connection = AppConfig::load(&path).unwrap().bridge;
    let client = BgiClient::new(connection.clone());
    let info = client.info().unwrap();
    let instance = info["instanceId"].clone();
    assert_eq!(info["enabled"], true);
    let probe = client.invoke("bgi.probe", &json!({})).unwrap();
    assert_eq!(probe["result"]["hostLoaded"], true);
    assert!(info["catalogVersion"].is_string(), "live bridge is stale");
    let mut offset = 0;
    let mut total = 0;
    let mut readable = 0;
    let mut run_group_contract = None;
    let mut update_scripts_contract = None;
    let mut sources = std::collections::BTreeMap::<String, usize>::new();
    loop {
        let page = client.catalog_page("", None, offset).unwrap();
        let items = page["items"].as_array().unwrap();
        for item in items {
            let id = item["methodId"].as_str().unwrap();
            assert!(
                item["summary"].as_str().is_some_and(|text| text.len() > 12),
                "{id} purpose missing"
            );
            assert!(
                item["whenToUse"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty()),
                "{id} discovery usage missing"
            );
            assert!(
                item["parameters"].is_array(),
                "{id} discovery parameters missing"
            );
            let detail = client.describe(id).unwrap();
            if id == "bgi.run_script_group" {
                run_group_contract = Some(detail.clone());
            }
            if id == "bgi.update_subscribed_scripts" {
                update_scripts_contract = Some(detail.clone());
            }
            let guide = &detail["guide"];
            assert!(
                guide["verification"].is_string() && guide["rollback"].is_string(),
                "{id} guidance missing"
            );
            if id.starts_with("cmd.") {
                let title = item["displayName"].as_str().unwrap_or("");
                assert!(
                    !["关闭", "打开", "删除", "Activated", "Loaded"].contains(&title),
                    "{id} has an ambiguous title: {title}"
                );
                assert!(
                    !item["summary"].as_str().unwrap_or("").contains("交互处理"),
                    "{id} still uses a method-name placeholder"
                );
            }
            for example in guide["examples"].as_array().unwrap() {
                // Unavailable host-object commands deliberately have no callable JSON example.
                if detail["callable"] == true {
                    assert!(
                        jsonschema::is_valid(&detail["inputSchema"], example),
                        "{id} invalid example"
                    );
                }
            }
            let source = guide["documentationSource"].as_str().unwrap_or("missing");
            *sources.entry(source.to_string()).or_default() += 1;
            assert_ne!(
                source, "type-contract",
                "{id} still lacks business documentation"
            );
            if id.starts_with("setting.") && detail["callable"] == true {
                let response = client.invoke(id, &json!({})).unwrap();
                assert!(
                    response["result"]["path"].is_string(),
                    "{id} failed to read"
                );
                readable += 1;
            }
            total += 1;
        }
        match page["nextOffset"].as_u64() {
            Some(next) => {
                assert!(next > offset);
                offset = next;
            }
            None => {
                assert_eq!(total, page["total"].as_u64().unwrap());
                break;
            }
        }
    }
    let run_group = run_group_contract.expect("stable script-group operation is missing");
    assert_eq!(run_group["group"], "scheduler");
    assert_eq!(run_group["effect"], "gameWrite");
    assert_eq!(run_group["requiresGameReady"], true);
    assert_eq!(run_group["callable"], true);
    let update_scripts =
        update_scripts_contract.expect("stable script update operation is missing");
    assert_eq!(update_scripts["group"], "repository");
    assert_eq!(update_scripts["effect"], "hostCommand");
    assert_eq!(update_scripts["requiresGameReady"], false);
    assert_eq!(update_scripts["callable"], true);
    eprintln!("LIVE CONTRACT AUDIT: {total} APIs; {readable} setting reads; sources={sources:?}");
    let config_file = std::path::Path::new(r"E:\tools\test\BetterGI\User\config.json");
    let before_config: Value =
        serde_json::from_slice(&std::fs::read(config_file).unwrap()).unwrap();
    let current = client
        .invoke("bgi.get_setting", &json!({"path":"detailedErrorLogs"}))
        .unwrap();
    assert_eq!(current["result"]["writable"], true);
    let original = current["result"]["currentValue"].as_bool().unwrap();
    let preview = client.invoke("bgi.preview_settings", &json!({"changes":[{
        "path":"detailedErrorLogs","value":!original,"expectedVersion":current["result"]["valueVersion"]
    }]})).unwrap();
    let plan_id = preview["result"]["planId"].as_str().unwrap();
    assert_eq!(preview["result"]["changesConfig"], false);
    let submitted = client
        .invoke("bgi.commit_settings", &json!({"planId":plan_id}))
        .unwrap();
    let commit = wait_job(&client, &submitted);
    // Always attempt rollback before asserting the commit outcome.
    let rollback = client
        .invoke("bgi.rollback_settings", &json!({"changeId":plan_id}))
        .unwrap();
    let rolled_back = wait_job(&client, &rollback);
    assert_eq!(
        commit["verification"]["status"], "succeeded",
        "commit did not verify: {commit}"
    );
    assert_eq!(
        rolled_back["verification"]["status"], "succeeded",
        "rollback did not verify: {rolled_back}"
    );
    let after = client
        .invoke("bgi.get_setting", &json!({"path":"detailedErrorLogs"}))
        .unwrap();
    assert_eq!(after["result"]["currentValue"], original);
    let after_config: Value = serde_json::from_slice(&std::fs::read(config_file).unwrap()).unwrap();
    assert!(
        before_config == after_config,
        "configuration values changed after rollback"
    );
    eprintln!(
        "LIVE TRANSACTION: harmless logging flag preview/commit/readback/rollback; original configuration values preserved"
    );
    let _ = call("bridge.state", json!({}));
    let mut wrong = connection;
    wrong.token = Some("invalid-token".into());
    assert!(BgiClient::new(wrong).info().is_err());
    for _ in 0..2 {
        let off = call("bridge.setEnabled", json!({"enabled":false}));
        assert!(off["warning"].is_null());
        assert_eq!(client.info().unwrap()["enabled"], false);
        assert!(
            client.state().is_err(),
            "disabled bridge must reject old clients too"
        );
        assert!(
            app.handle("bridge.state", json!({}), Arc::new(|_, _| {}))
                .is_err()
        );
        let status = call("bootstrap", json!({}));
        assert_eq!(status["bridge"]["enabled"], false);
        assert!(
            !status["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "bgi.state.get")
        );
        call("bridge.setEnabled", json!({"enabled":true}));
        assert_eq!(
            client.info().unwrap()["instanceId"],
            instance,
            "toggle must reuse the loaded bridge"
        );
    }
    call("bridge.setEnabled", json!({"enabled":false}));
    app.shutdown();
}

fn wait_job(client: &BgiClient, submitted: &Value) -> Value {
    let id = submitted["jobId"].as_str().unwrap();
    for _ in 0..200 {
        let job = client.job(id).unwrap();
        if matches!(
            job["state"].as_str(),
            Some("completed" | "failed" | "cancelled")
        ) {
            return job;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("bridge test job did not finish");
}
