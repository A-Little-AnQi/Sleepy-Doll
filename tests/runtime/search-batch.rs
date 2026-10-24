use super::*;
use crate::extension::{FunctionTool, Tool, ToolExposure};
use futures_util::future::BoxFuture;
use std::sync::atomic::AtomicUsize;
use tokio::sync::Notify;

type SearchFn =
    Arc<dyn Fn(Value, CancellationToken) -> BoxFuture<'static, Result<Value>> + Send + Sync>;
struct SearchTool {
    run: SearchFn,
}
impl Tool for SearchTool {
    fn definition(&self) -> ToolDefinition {
        FunctionTool::new("fixture.search", "检索演示接口", json!({"type":"object","properties":{"index":{"type":"integer"}},"required":["index"],"additionalProperties":false}), "plugin:fixture", |_| unreachable!())
            .with_execution(ToolExecution { search_provider:true,always_load:true,deferred:false,..ToolExecution::read_only() }).definition()
    }
    fn call(&self, _: &Value) -> Result<Value> {
        unreachable!()
    }
    fn call_async(
        self: Arc<Self>,
        a: Value,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Result<Value>> {
        (self.run)(a, cancel)
    }
}
fn register(supervisor: &Supervisor, callback: SearchFn) {
    supervisor
        .config
        .write()
        .unwrap()
        .plugins
        .enabled
        .push("fixture".into());
    let mut registry = (*supervisor.tools()).clone();
    registry.register(SearchTool { run: callback }).unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(registry);
}
fn request(id: &str, index: usize) -> Value {
    json!({"id":id,"tool":"fixture.search","arguments":{"index":index}})
}
fn batch(requests: Vec<Value>, concurrency: usize, completion: &str) -> ToolCall {
    ToolCall {
        id: "batch".into(),
        name: "search.batch".into(),
        arguments: json!({"requests":requests,"concurrency":concurrency,
        "order":if concurrency==1 {"sequential"} else {"parallel"},"completion":completion}),
    }
}
fn bridge(supervisor: &Supervisor) -> host::bridge::Bridge {
    host::bridge::Bridge::new(supervisor.config.read().unwrap().bridge.clone()).unwrap()
}
struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn unordered_search_refills_without_waiting_for_the_slowest_and_respects_limit() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    let release = Arc::new(Notify::new());
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (release2, active2, peak2) = (release.clone(), active.clone(), peak.clone());
    register(
        &supervisor,
        Arc::new(move |a, _| {
            let (release, active, peak) = (release2.clone(), active2.clone(), peak2.clone());
            Box::pin(async move {
                peak.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                let _active = Active(active);
                let index = a["index"].as_u64().unwrap();
                if index == 0 {
                    release.notified().await;
                }
                if index == 4 {
                    release.notify_one();
                }
                Ok(json!({"items":[{"index":index}]}))
            })
        }),
    );
    let run = supervisor
        .journal
        .create("检索接口", "search-refill", "search-refill", 60, None)
        .unwrap();
    let call = batch(
        (0..6).map(|i| request(&i.to_string(), i)).collect(),
        2,
        "all",
    );
    let value = tokio::time::timeout(
        Duration::from_secs(3),
        supervisor.search_batch(
            &run,
            &call,
            &bridge(&supervisor),
            &CancellationToken::new(),
            &mut HashSet::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(peak.load(Ordering::SeqCst), 2, "{value}");
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert_eq!(value["allSettled"], true);
    assert_eq!(value["results"].as_array().unwrap().len(), 6);
    let arrivals = value["completionOrder"].as_array().unwrap();
    assert!(
        arrivals.iter().position(|v| v == "4").unwrap()
            < arrivals.iter().position(|v| v == "0").unwrap()
    );
}

#[tokio::test]
async fn dependencies_wait_for_all_results_and_distinguish_empty_from_failure() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    register(
        &supervisor,
        Arc::new(move |a, _| {
            let seen = seen.clone();
            Box::pin(async move {
                let index = a["index"].as_u64().unwrap();
                seen.lock().unwrap().push(index);
                if index == 1 {
                    return Err(Error::Tool("搜索源暂不可用".into()));
                }
                Ok(json!({"items":[]}))
            })
        }),
    );
    let run = supervisor
        .journal
        .create(
            "检索接口",
            "search-conditions",
            "search-conditions",
            60,
            None,
        )
        .unwrap();
    let mut empty = request("wrong-empty", 2);
    empty["after"] = json!(["a", "b"]);
    empty["when"] = json!("allEmpty");
    let mut fallback = request("error-fallback", 3);
    fallback["after"] = json!(["a", "b"]);
    fallback["when"] = json!("anyError");
    let value = supervisor
        .search_batch(
            &run,
            &batch(
                vec![request("a", 0), request("b", 1), empty, fallback],
                2,
                "all",
            ),
            &bridge(&supervisor),
            &CancellationToken::new(),
            &mut HashSet::new(),
        )
        .await
        .unwrap();
    assert_eq!(value["results"][2]["state"], "skipped");
    assert_eq!(*calls.lock().unwrap(), vec![0, 1, 3]);
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    let start = events
        .iter()
        .position(|e| e.kind == "search.started" && e.data["id"] == "error-fallback")
        .unwrap();
    for id in ["a", "b"] {
        assert!(
            events
                .iter()
                .position(|e| e.kind == "search.completed" && e.data["id"] == id)
                .unwrap()
                < start
        );
    }
}

#[tokio::test]
async fn first_hit_returns_without_slowest_and_cancels_unfinished_work() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    let active = Arc::new(AtomicUsize::new(0));
    let state = active.clone();
    register(
        &supervisor,
        Arc::new(move |a, cancel| {
            let active = state.clone();
            Box::pin(async move {
                active.fetch_add(1, Ordering::SeqCst);
                let _guard = Active(active);
                if a["index"] == 0 {
                    cancel.cancelled().await;
                    return Err(Error::Cancelled);
                }
                Ok(json!({"items":["found"]}))
            })
        }),
    );
    let run = supervisor
        .journal
        .create("检索接口", "search-first", "search-first", 60, None)
        .unwrap();
    let value = supervisor
        .search_batch(
            &run,
            &batch(
                vec![request("slow", 0), request("hit", 1), request("queued", 2)],
                2,
                "firstHit",
            ),
            &bridge(&supervisor),
            &CancellationToken::new(),
            &mut HashSet::new(),
        )
        .await
        .unwrap();
    assert_eq!(value["stopReason"], "firstHit");
    assert_eq!(value["allSettled"], false);
    assert_eq!(value["results"][0]["state"], "abandoned");
    assert_eq!(value["results"][2]["state"], "notStarted");
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_batch_is_rejected_before_any_request_and_sequential_order_is_preserved() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    register(
        &supervisor,
        Arc::new(move |a, _| {
            let seen = seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(a["index"].clone());
                Ok(json!({"items":[]}))
            })
        }),
    );
    let run = supervisor
        .journal
        .create(
            "检索接口",
            "search-validation",
            "search-validation",
            60,
            None,
        )
        .unwrap();
    let good = batch(vec![request("a", 0), request("b", 1)], 1, "all");
    let mut bad = good.clone();
    bad.arguments["concurrency"] = json!(5);
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    bad = good.clone();
    bad.arguments["requests"][1]["tool"] = json!("workspace.write");
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    bad = good.clone();
    bad.arguments["requests"][0]["after"] = json!(["b"]);
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    bad = good.clone();
    bad.arguments["requests"][1]["arguments"] = json!({"unknown":1});
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    assert!(calls.lock().unwrap().is_empty());
    bad = good.clone();
    bad.arguments["requests"] = json!(
        (0..9)
            .map(|i| request(&i.to_string(), i))
            .collect::<Vec<_>>()
    );
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    bad = good.clone();
    bad.arguments["requests"][1]["tool"] = json!("search.batch");
    assert!(
        supervisor
            .search_batch(
                &run,
                &bad,
                &bridge(&supervisor),
                &CancellationToken::new(),
                &mut HashSet::new()
            )
            .await
            .is_err()
    );
    supervisor
        .search_batch(
            &run,
            &good,
            &bridge(&supervisor),
            &CancellationToken::new(),
            &mut HashSet::new(),
        )
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), vec![json!(0), json!(1)]);
}

#[tokio::test]
async fn cancelling_a_search_batch_drops_inflight_waits_without_launching_the_queue() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    let entered = Arc::new(Notify::new());
    let signal = entered.clone();
    let count = Arc::new(AtomicUsize::new(0));
    let active = count.clone();
    register(
        &supervisor,
        Arc::new(move |_, cancel| {
            let (signal, active) = (signal.clone(), active.clone());
            Box::pin(async move {
                active.fetch_add(1, Ordering::SeqCst);
                let _guard = Active(active);
                signal.notify_one();
                cancel.cancelled().await;
                Err(Error::Cancelled)
            })
        }),
    );
    let run = supervisor
        .journal
        .create("检索接口", "search-cancel", "search-cancel", 60, None)
        .unwrap();
    let token = CancellationToken::new();
    let abort = token.clone();
    let finish = async {
        entered.notified().await;
        abort.cancel();
    };
    let call = batch(vec![request("first", 0), request("queued", 1)], 1, "all");
    let native = bridge(&supervisor);
    let mut exposed = HashSet::new();
    let (value, _) = tokio::join!(
        supervisor.search_batch(&run, &call, &native, &token, &mut exposed),
        finish
    );
    assert!(matches!(value, Err(Error::Cancelled)));
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let events = supervisor.journal.events(&run.conversation_id, 0).unwrap();
    assert!(
        !events
            .iter()
            .any(|e| e.kind == "search.started" && e.data["id"] == "queued")
    );
}

#[tokio::test]
async fn timeout_is_an_unfinished_barrier_not_an_empty_result() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    register(
        &supervisor,
        Arc::new(|_, cancel| {
            Box::pin(async move {
                cancel.cancelled().await;
                Err(Error::Cancelled)
            })
        }),
    );
    let run = supervisor
        .journal
        .create("检索接口", "search-timeout", "search-timeout", 60, None)
        .unwrap();
    let mut call = batch(vec![request("slow", 0), request("queued", 1)], 1, "all");
    call.arguments["timeoutMs"] = json!(1000);
    let value = supervisor
        .search_batch(
            &run,
            &call,
            &bridge(&supervisor),
            &CancellationToken::new(),
            &mut HashSet::new(),
        )
        .await
        .unwrap();
    assert_eq!(value["allSettled"], false);
    assert_eq!(value["stopReason"], "timeout");
    assert!(value["found"].is_null());
    assert_eq!(value["results"][0]["outcome"], "unknown");
    assert_eq!(value["results"][1]["state"], "notStarted");
}

#[test]
fn discovery_filters_sources_effects_and_batch_eligibility_before_ranking() {
    let fixture = ModelFixture::new(|_| panic!("no model calls"));
    let supervisor = supervisor(&fixture);
    register(
        &supervisor,
        Arc::new(|_, _| Box::pin(async { unreachable!() })),
    );
    let mut exposed = HashSet::new();
    let value=supervisor.discover_tools(&json!({"query":"search","sourcePrefix":"plugin:fixture","effect":"readOnly","searchOnly":true}),&mut exposed).unwrap();
    assert_eq!(value.as_array().unwrap().len(), 1);
    assert_eq!(value[0]["name"], "fixture.search");
    assert!(exposed.contains("fixture.search"));
    let error = supervisor
        .discover_tools(
            &json!({"query":"fixture.search","sourcePrefix":"fixture"}),
            &mut HashSet::new(),
        )
        .unwrap_err();
    assert!(error.user_message().contains("plugin:fixture"));
    assert!(error.user_message().contains("不是业务能力未找到"));
    assert!(
        supervisor
            .discover_tools(&json!({"query":"search","effect":"invalid"}), &mut exposed)
            .is_err()
    );
}

#[tokio::test]
async fn model_discovers_batch_then_receives_merged_results_and_real_read_unlocks() {
    let mut turn = 0;
    let fixture = ModelFixture::new(move |body| {
        turn += 1;
        (
            200,
            match turn {
                1 => tool_calls(vec![("tools.search", json!({"query":"search.batch"}))]),
                2 => tool_calls(vec![(
                    "search.batch",
                    batch(vec![request("a", 0), request("b", 1)], 2, "all").arguments,
                )]),
                3 => {
                    assert!(body["messages"].to_string().contains("allSettled"));
                    assert!(
                        body["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|t| t["function"]["name"] == gateway::wire_name("fixture.next"))
                    );
                    assert!(
                        !body["tools"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|t| t["function"]["name"] == gateway::wire_name("fixture.write"))
                    );
                    answer("已找到接口")
                }
                _ => panic!("unexpected model call"),
            },
        )
    });
    let supervisor = supervisor(&fixture);
    register(
        &supervisor,
        Arc::new(|a, _| {
            Box::pin(async move { Ok(json!({"items":[a],"details":"x".repeat(40000)})) })
        }),
    );
    let mut tools = (*supervisor.tools()).clone();
    tools
        .register(
            FunctionTool::new(
                "fixture.write",
                "不应开放的写入",
                json!({"type":"object"}),
                "plugin:fixture",
                |_| panic!("never execute"),
            )
            .with_execution(ToolExecution {
                effect: ToolEffect::LocalWrite,
                always_load: true,
                deferred: false,
                exposure: ToolExposure::AfterRead,
                ..ToolExecution::default()
            }),
        )
        .unwrap();
    tools
        .register(
            FunctionTool::new(
                "fixture.next",
                "读取完整接口",
                json!({"type":"object"}),
                "plugin:fixture",
                |_| panic!("schema only"),
            )
            .with_execution(ToolExecution {
                always_load: true,
                deferred: false,
                exposure: ToolExposure::AfterRead,
                ..ToolExecution::read_only()
            }),
        )
        .unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    supervisor.config.write().unwrap().runtime.permission_mode =
        operation::permissions::PermissionMode::PlanOnly;
    let mut run = supervisor
        .journal
        .create("并行查找接口", "search-model", "search-model", 60, None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Answered);
    assert_eq!(fixture.requests.lock().unwrap().len(), 3);
}
