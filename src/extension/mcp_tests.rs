use super::*;
use crate::extension::{FunctionTool, ToolEffect, ToolRegistry};
use std::{
    io::{BufRead, BufReader},
    process::Command as StdCommand,
};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/runtime/mcp-fixture.mjs");

struct HttpFixture {
    child: std::process::Child,
    url: String,
}
impl HttpFixture {
    fn start(stateless: bool) -> Self {
        let mut child = StdCommand::new("node")
            .args([
                FIXTURE,
                "http",
                if stateless { "stateless" } else { "session" },
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut url = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut url)
            .unwrap();
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
        Self {
            child,
            url: url.trim().into(),
        }
    }
}
impl Drop for HttpFixture {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn manifest(value: Value) -> McpServerManifest {
    serde_json::from_value(value).unwrap()
}
fn stdio_manifest() -> McpServerManifest {
    manifest(json!({"id":"local","command":"node","args":[FIXTURE,"stdio"],"timeoutMs":2000}))
}
fn find(tools: &[Arc<dyn Tool>], suffix: &str) -> Arc<dyn Tool> {
    tools
        .iter()
        .find(|tool| tool.definition().name.ends_with(suffix))
        .unwrap()
        .clone()
}

#[test]
fn stdio_discovers_pages_preserves_content_and_does_not_trust_remote_policy() {
    let client = McpClient::start(&stdio_manifest()).unwrap();
    assert!(client.instructions().unwrap().contains("echo"));
    let tools = client
        .list_tools("fixture", "local", "1", &HashMap::new())
        .unwrap();
    assert_eq!(tools.len(), 6);
    let echo = find(&tools, ".echo");
    assert_eq!(echo.definition().label, "标题 echo");
    assert_eq!(echo.definition().execution.effect, ToolEffect::Unknown);
    assert_eq!(echo.call(&json!({"x":7})).unwrap(), json!({"value":7}));
    assert!(
        find(&tools, ".fail")
            .call(&json!({}))
            .unwrap_err()
            .to_string()
            .contains("任务参数与当前状态不匹配")
    );
    assert!(find(&tools, ".invalid").call(&json!({})).is_err());
    assert_eq!(
        find(&tools, ".content").call(&json!({})).unwrap()["content"][1]["type"],
        "image"
    );
}

#[test]
fn http_supports_json_sse_stateful_and_stateless_servers() {
    for stateless in [false, true] {
        let fixture = HttpFixture::start(stateless);
        let config = manifest(
            json!({"id":"http","transport":"streamableHttp","url":fixture.url,"headers":{"x-fixture":"header-value"},"timeoutMs":2000}),
        );
        let client = McpClient::start(&config).unwrap();
        let tools = client
            .list_tools("fixture", "http", "1", &HashMap::new())
            .unwrap();
        assert_eq!(
            find(&tools, ".echo").call(&json!({"x":9})).unwrap()["value"],
            9
        );
        let inspection = find(&tools, ".inspect").call(&json!({})).unwrap();
        for request in inspection["observed"].as_array().unwrap() {
            assert_eq!(request["custom"], "header-value");
            if request["method"] != "initialize" {
                assert_eq!(request["protocol"], "2025-06-18");
                if !stateless {
                    assert_eq!(request["session"], "fixture-session");
                }
            }
        }
    }
}

#[test]
fn cancellation_and_timeout_send_standard_notifications() {
    let client = McpClient::start(&stdio_manifest()).unwrap();
    let tools = client
        .list_tools("fixture", "local", "1", &HashMap::new())
        .unwrap();
    let slow = find(&tools, ".slow");
    let cancellation = CancellationToken::new();
    let trigger = cancellation.clone();
    let result = crate::runtime::executor().block_on(async {
        tokio::join!(slow.call_async(json!({}), cancellation), async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            trigger.cancel();
        })
        .0
    });
    assert!(matches!(result, Err(Error::Conflict(_))));
    let inspection = find(&tools, ".inspect").call(&json!({})).unwrap();
    assert!(
        inspection["observed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["method"] == "notifications/cancelled")
    );

    let mut config = stdio_manifest();
    config.timeout_ms = 2000;
    let mut timed = McpClient::start(&config).unwrap();
    Arc::get_mut(&mut timed).unwrap().timeout = Duration::from_millis(250);
    let result = timed.request("tools/call", json!({"name":"slow","arguments":{}}));
    assert!(matches!(result, Err(Error::Timeout(_))));
    let inspected = timed
        .request("tools/call", json!({"name":"inspect","arguments":{}}))
        .unwrap();
    assert!(
        inspected["structuredContent"]["observed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["method"] == "notifications/cancelled")
    );
}

#[test]
fn bgi_roundtrip_keeps_names_contracts_execution_and_structured_results() {
    let config = manifest(json!({"id":"bridge","transport":"inProcess","server":"bgi"}));
    let native = Arc::new(crate::bridge::BgiClient::new(crate::config::BridgeConfig {
        enabled: true,
        base_url: "http://127.0.0.1:1".into(),
        token: None,
        instance_id: None,
        timeout_ms: 500,
        host_install_path: None,
        auto_start: false,
        launch_silently: false,
    }));
    let mut original = ToolRegistry::default();
    crate::bridge::register_tools(&mut original, native.clone()).unwrap();
    let server = crate::bridge::mcp::server(native, true).unwrap();
    let client = McpClient::in_process(&config, server).unwrap();
    let tools = client
        .list_tools("bgi", "bridge", "1", &HashMap::new())
        .unwrap();
    assert_eq!(tools.len(), original.len());
    for before in original.definitions() {
        let after = tools
            .iter()
            .find(|t| t.definition().name == before.name)
            .unwrap()
            .definition();
        assert_eq!(before.input_schema, after.input_schema);
        assert_eq!(before.label, after.label);
        assert_eq!(
            serde_json::to_value(before.execution).unwrap(),
            serde_json::to_value(after.execution).unwrap()
        );
        assert_eq!(after.source, "plugin:bgi:mcp:bridge");
    }
    assert!(
        !tools
            .iter()
            .any(|tool| tool.definition().name.starts_with("bgi.host."))
    );
    let args = json!({"query":"配置组","limit":2});
    let expected = original.call("bgi.feature.search", &args);
    let actual = find(&tools, "bgi.feature.search").call(&args).unwrap();
    assert_eq!(actual, expected["value"]);
}

#[test]
fn local_mcp_preserves_recoverable_stale_error() {
    let mut registry = ToolRegistry::default();
    registry
        .register(FunctionTool::new(
            "local.write",
            "write",
            json!({"type":"object"}),
            "test",
            |_| Err(Error::Stale("文件已被其他操作修改".into())),
        ))
        .unwrap();
    let config = manifest(json!({"id":"local","transport":"inProcess","server":"local"}));
    let client =
        McpClient::in_process(&config, RegistryServer::new(registry, "local", "")).unwrap();
    let tools = client
        .list_tools("local", "local", "1", &HashMap::new())
        .unwrap();
    assert!(matches!(tools[0].call(&json!({})), Err(Error::Stale(_))));
}

#[test]
fn structured_results_keep_accompanying_media() {
    let result = json!({"structuredContent":{"value":7},"content":[{"type":"image","data":"eA==","mimeType":"image/png"}]});
    let schema =
        json!({"type":"object","required":["value"],"properties":{"value":{"type":"integer"}}});
    assert_eq!(
        normalize_output(result.clone(), Some(&schema), false).unwrap(),
        result
    );
}

#[test]
fn disconnect_after_submission_stays_uncertain_instead_of_claiming_rejection() {
    let client = McpClient::start(&stdio_manifest()).unwrap();
    let result = client.request("tools/call", json!({"name":"disconnect","arguments":{}}));
    assert!(matches!(result, Err(Error::Http(_))));
}

#[test]
fn manifest_rejects_mixed_transports_and_keeps_legacy_stdio() {
    assert!(stdio_manifest().validate().is_ok());
    for value in [
        json!({"id":"x","command":"node","url":"http://localhost/mcp"}),
        json!({"id":"x","transport":"streamableHttp","url":"file:///mcp"}),
        json!({"id":"x","transport":"inProcess","server":"bgi","command":"node"}),
        json!({"id":"x","transport":"streamableHttp","url":"http://u:p@localhost/mcp"}),
    ] {
        assert!(manifest(value).validate().is_err());
    }
}

#[test]
fn bgi_runtime_requests_and_events_cross_mcp_without_exposing_internal_tools() {
    let fixture = HttpFixture::start(false);
    let config = crate::config::BridgeConfig {
        enabled: true,
        base_url: fixture.url.trim_end_matches("/mcp").into(),
        token: None,
        instance_id: None,
        timeout_ms: 2000,
        host_install_path: None,
        auto_start: false,
        launch_silently: false,
    };
    let native = Arc::new(
        crate::runtime::host::bridge::Bridge::for_transport_tests(config.clone()).unwrap(),
    );
    let internal = crate::bridge::mcp::transport_tools(native).unwrap();
    let server = RegistryServer::new(ToolRegistry::default(), "bgi", "").with_internal(internal);
    let endpoint = manifest(json!({"id":"bridge","transport":"inProcess","server":"bgi"}));
    let client = McpClient::in_process(&endpoint, server).unwrap();
    assert!(
        client
            .list_tools("bgi", "bridge", "1", &HashMap::new())
            .unwrap()
            .is_empty()
    );
    // 外层配置指向不可达端口，证明请求经 MCP 服务中的 native 连接执行。
    let mut outer_config = config;
    outer_config.base_url = "http://127.0.0.1:1".into();
    let bridge = crate::runtime::host::bridge::Bridge::new(outer_config)
        .unwrap()
        .with_mcp(client.clone());
    crate::runtime::executor().block_on(async {
        let token = CancellationToken::new();
        let reply = bridge
            .request(
                "POST",
                "/bridge/v1/read",
                Some(&json!({"methodId":"fixture.read"})),
                &token,
            )
            .await
            .unwrap();
        assert_eq!(reply["body"]["methodId"], "fixture.read");
        let event = client
            .call_tool("bgi.host.events", json!({"cursor":null}), token)
            .await
            .unwrap();
        assert_eq!(event["cursor"], "event-7");
    });
}

#[test]
fn shipped_bgi_manifest_loads_and_disables_tools_with_its_plugin() {
    let native = Arc::new(crate::bridge::BgiClient::new(crate::config::BridgeConfig {
        enabled: true,
        base_url: "http://127.0.0.1:1".into(),
        token: None,
        instance_id: None,
        timeout_ms: 500,
        host_install_path: None,
        auto_start: false,
        launch_silently: false,
    }));
    for disabled in [vec![], vec!["bgi".to_string()]] {
        let mut plugins = crate::extension::plugins::PluginManager::default();
        plugins.provide_mcp(
            "bgi",
            crate::bridge::mcp::server(native.clone(), true).unwrap(),
        );
        let mut tools = ToolRegistry::default();
        let mut skills = crate::extension::skills::SkillRegistry::default();
        plugins
            .load(
                &[std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins")],
                &[],
                &disabled,
                &mut tools,
                &mut skills,
            )
            .unwrap();
        if disabled.is_empty() {
            assert!(
                tools
                    .definitions()
                    .iter()
                    .any(|d| d.name == "bgi.api.invoke" && d.source == "plugin:bgi:mcp:bridge")
            );
            assert!(tools.mcp_connection("bgi", "bridge").is_some());
            assert!(
                skills
                    .list()
                    .iter()
                    .any(|s| s.name == "mcp-bgi-bridge" && s.always_load)
            );
        } else {
            assert!(tools.is_empty());
            assert!(tools.mcp_connection("bgi", "bridge").is_none());
            assert!(skills.is_empty());
        }
    }
}
