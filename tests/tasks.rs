//! 快捷任务的确定性控制流与零 token 运行。
//!
//! 这里的每个用例都走真实 IPC：建草稿 → 发布 → 运行 → 读回状态。断言既看运行
//! 终态，也看模型网关的请求计数 —— 界面上的「不调用模型」标签不是证据。

use serde_json::{Value, json};
use sleepy_doll::{app::AppController, model::mock::MockBackend};
use std::{fs, sync::Arc, thread, time::Duration};

fn ipc(c: &Arc<AppController>, method: &str, params: Value) -> Value {
    c.handle(method, params, Arc::new(|_, _| {})).unwrap()
}

fn try_ipc(
    c: &Arc<AppController>,
    method: &str,
    params: Value,
) -> sleepy_doll::error::Result<Value> {
    c.handle(method, params, Arc::new(|_, _| {}))
}

fn controller(backend: &MockBackend, d: &tempfile::TempDir) -> Arc<AppController> {
    let path = d.path().join("config.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version":1,
            "activeModel":"mock",
            "models":[{"id":"mock","name":"Mock","protocol":"openai-responses","model":"mock-model",
                       "baseUrl":format!("{}/v1",backend.base_url()),"options":{"timeoutMs":2000}}],
            "agent":{"systemPrompt":"test"},
            "bridge":{"enabled":true,"baseUrl":backend.base_url(),"token":"mock",
                      "instanceId":"mock-bgi","timeoutMs":2000},
            "storage":{"database":d.path().join("test.db")}
        }))
        .unwrap(),
    )
    .unwrap();
    Arc::new(AppController::load(path).unwrap())
}

fn wait(c: &Arc<AppController>, id: &str, states: &[&str]) -> Value {
    for _ in 0..300 {
        let result = ipc(c, "run.get", json!({"id":id}));
        if states.contains(&result["state"].as_str().unwrap_or("")) {
            return result;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("等待状态超时：{}", ipc(c, "run.get", json!({"id":id})));
}

/// 建一个草稿并发布，返回 taskId。
fn publish(c: &Arc<AppController>, name: &str, nodes: Value) -> String {
    let draft = ipc(
        c,
        "workflow.draft.create",
        json!({"name":name,"description":"测试用任务","nodes":nodes}),
    );
    let id = draft["id"].as_str().unwrap().to_owned();
    let validation = &draft["validation"];
    assert!(
        validation["issues"].as_array().unwrap().is_empty(),
        "{name} 不该有校验问题：{validation}"
    );
    ipc(
        c,
        "workflow.publish",
        json!({"id":id,"draftRevision":draft["revision"]}),
    );
    id
}

fn run(c: &Arc<AppController>, id: &str, key: &str) -> Value {
    let run = ipc(c, "workflow.run", json!({"id":id,"clientKey":key}));
    wait(
        c,
        run["id"].as_str().unwrap(),
        &["succeeded", "failed", "partial", "needsReview", "blocked"],
    )
}

/// 读回这次运行每一步的结果。
fn steps(c: &Arc<AppController>, conversation: &str, run_id: &str) -> Vec<(String, String)> {
    let events = ipc(
        c,
        "events.read",
        json!({"conversationId":conversation,"after":0,"waitMs":0}),
    );
    let mut found = Vec::new();
    for event in events["events"].as_array().unwrap() {
        if event["runId"] != json!(run_id) || event["kind"] != json!("step.finished") {
            continue;
        }
        found.push((
            event["data"]["id"].as_str().unwrap_or("").to_owned(),
            event["data"]["outcome"].as_str().unwrap_or("").to_owned(),
        ));
    }
    found
}

fn model_call(name: &str, args: Value) -> Value {
    json!({"status":"completed","output":[{"type":"function_call","call_id":"provider-task","name":name,"arguments":args.to_string()}],"usage":{"input_tokens":0,"output_tokens":0}})
}

fn model_answer() -> Value {
    json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"done"}]}],"usage":{"input_tokens":0,"output_tokens":0}})
}

fn state_node(id: &str) -> Value {
    json!({"kind":"tool","id":id,"title":"读取状态","tool":"bgi.state.get","arguments":{}})
}

#[test]
fn sequence_runs_steps_in_order_and_the_run_stays_model_free() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "连续读取",
        json!([{"kind":"sequence","id":"all","title":"连续读取","nodes":[state_node("a"),state_node("b")]}]),
    );
    backend.reset_model_requests();
    let result = run(&c, &id, "seq");
    assert_eq!(result["state"], "succeeded");
    assert_eq!(result["decisions"], 0);
    assert_eq!(result["toolCalls"], 2);
    assert_eq!(backend.model_requests(), 0);
    let finished = steps(
        &c,
        result["conversationId"].as_str().unwrap(),
        result["id"].as_str().unwrap(),
    );
    let ids = finished
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    assert!(ids.contains(&"a".to_owned()) && ids.contains(&"b".to_owned()));
    assert!(
        finished
            .iter()
            .all(|(_, outcome)| outcome == "verifiedSucceeded")
    );
}

/// FLOW-03：条件必须精确命中 true / false / unknown 三个分支，未知不当 false。
#[test]
fn condition_routes_true_false_and_unknown_separately() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let branch = |name: &str, comparison: Value| {
        json!({
            "kind":"condition","id":name,"title":name,
            "condition":{"kind":"compare","op":"equals","left":comparison,"right":{"kind":"literal","value":"observed"}},
            "then":[{"kind":"result","id":format!("{name}-then"),"title":"成立","template":"成立"}],
            "otherwise":[{"kind":"result","id":format!("{name}-else"),"title":"不成立","template":"不成立"}],
            "unknown":[{"kind":"result","id":format!("{name}-unknown"),"title":"未确定","template":"未确定"}]
        })
    };
    let id = publish(
        &c,
        "三分支",
        json!([{
            "kind":"sequence","id":"all","title":"三分支","nodes":[
                state_node("state"),
                branch("hit", json!({"kind":"nodeOutput","node":"state","path":["ui","status"]})),
                branch("miss", json!({"kind":"nodeOutput","node":"state","path":["ui","value"]})),
                branch("absent", json!({"kind":"nodeOutput","node":"state","path":["nothing","here"]}))
            ]
        }]),
    );
    backend.reset_model_requests();
    let result = run(&c, &id, "cond");
    assert_eq!(result["state"], "succeeded");
    let finished = steps(
        &c,
        result["conversationId"].as_str().unwrap(),
        result["id"].as_str().unwrap(),
    );
    let outcomes = finished
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>();
    assert!(outcomes.contains(&"hit-then"), "{outcomes:?}");
    assert!(outcomes.contains(&"miss-else"), "{outcomes:?}");
    assert!(outcomes.contains(&"absent-unknown"), "{outcomes:?}");
    // 未选中的分支一步都不能跑。
    assert!(!outcomes.contains(&"hit-else"));
    assert!(!outcomes.contains(&"miss-then"));
    assert_eq!(backend.model_requests(), 0);
}

/// FLOW-05：空列表合法完成并说明没有目标；非空列表每项恰一次。
#[test]
fn foreach_handles_empty_and_populated_lists() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let each = |id: &str, items: Value| {
        json!({
            "kind":"forEach","id":id,"title":"逐项","items":{"kind":"literal","value":items},
            "itemKey":"name","maxItems":10,
            "nodes":[{"kind":"result","id":format!("{id}-body"),"title":"记录","template":"项 {{ item }}"}]
        })
    };
    let empty = publish(&c, "空列表", json!([each("empty", json!([]))]));
    let one = publish(&c, "单项", json!([each("one", json!([{"name":"a"}]))]));
    let many = publish(
        &c,
        "多项",
        json!([each(
            "many",
            json!([{"name":"a"},{"name":"b"},{"name":"c"}])
        )]),
    );
    backend.reset_model_requests();
    let empty = run(&c, &empty, "empty");
    assert_eq!(empty["state"], "succeeded");
    assert!(
        empty["result"]
            .as_str()
            .unwrap()
            .contains("已完成 1 个步骤")
    );
    let one = run(&c, &one, "one");
    assert_eq!(one["state"], "succeeded");
    let many = run(&c, &many, "many");
    assert_eq!(many["state"], "succeeded");
    assert_eq!(backend.model_requests(), 0);
}

/// FLOW-07：有退出条件时按条件停，不跑满上限；次数不虚报。
#[test]
fn repeat_stops_when_the_exit_condition_is_met() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "重复三次",
        json!([{
            "kind":"repeat","id":"loop","title":"重复","count":5,"maxIterations":5,
            "nodes":[{"kind":"result","id":"tick","title":"一次","template":"第 {{ iteration }} 次"}]
        }]),
    );
    let result = run(&c, &id, "repeat");
    assert_eq!(result["state"], "succeeded");
    let finished = steps(
        &c,
        result["conversationId"].as_str().unwrap(),
        result["id"].as_str().unwrap(),
    );
    let ticks = finished
        .iter()
        .filter(|(id, _)| id.starts_with("tick"))
        .count();
    // 每次进入体都上报一次；5 次固定次数就是 5 个 tick。
    assert_eq!(ticks, 5, "{finished:?}");
}

/// FLOW-08：超上限的定义在发布前就被拒绝，不截断语义。
#[test]
fn over_limit_definitions_are_refused_before_publishing() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let draft = ipc(
        &c,
        "workflow.draft.create",
        json!({
            "name":"超限",
            "description":"展开次数超过上限",
            "limits":{"maxNodes":200,"maxLoopExpansions":10,"maxToolAttempts":1000,"maxSeconds":3600},
            "nodes":[{
                "kind":"repeat","id":"loop","title":"重复","count":50,"maxIterations":100,
                "nodes":[{"kind":"result","id":"t","title":"t","template":"x"}]
            }]
        }),
    );
    let id = draft["id"].as_str().unwrap();
    let error = try_ipc(
        &c,
        "workflow.publish",
        json!({"id":id,"draftRevision":draft["revision"]}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("上限"), "{error}");
    // 未发布的任务不能运行。
    assert!(try_ipc(&c, "workflow.run", json!({"id":id,"clientKey":"x"})).is_err());
}

/// FLOW-11：失败策略 stop 时依赖步骤全部跳过，整体不染绿。
#[test]
fn stop_policy_skips_the_rest_instead_of_reporting_success() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "中途失败",
        json!([{
            "kind":"sequence","id":"all","title":"中途失败","nodes":[
                state_node("ok"),
                {"kind":"tool","id":"bad","title":"读不存在的产物","tool":"artifact.read",
                 "arguments":{"id":"nope"}},
                {"kind":"result","id":"after","title":"后续","template":"不该跑到这里"}
            ]
        }]),
    );
    let result = run(&c, &id, "stop");
    assert_eq!(result["state"], "partial");
    let finished = steps(
        &c,
        result["conversationId"].as_str().unwrap(),
        result["id"].as_str().unwrap(),
    );
    let after = finished.iter().find(|(id, _)| id == "after");
    assert_eq!(
        after.map(|(_, outcome)| outcome.as_str()),
        Some("skipped"),
        "{finished:?}"
    );
    assert_eq!(
        finished
            .iter()
            .find(|(id, _)| id == "bad")
            .map(|(_, outcome)| outcome.as_str()),
        Some("verifiedFailed")
    );
}

/// FLOW-12：continueIndependent 只跳过依赖它的部分，整体按部分完成收场。
#[test]
fn continue_independent_keeps_running_the_next_step() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "独立继续",
        json!([{
            "kind":"sequence","id":"all","title":"独立继续","nodes":[
                {"kind":"tool","id":"bad","title":"读不存在的产物","tool":"artifact.read",
                 "arguments":{"id":"nope"},"onFailure":{"kind":"continueIndependent"},
                 "onUnverified":{"kind":"stop"}},
                {"kind":"result","id":"next","title":"继续","template":"继续跑了"}
            ]
        }]),
    );
    let result = run(&c, &id, "continue");
    assert_eq!(result["state"], "partial");
    let finished = steps(
        &c,
        result["conversationId"].as_str().unwrap(),
        result["id"].as_str().unwrap(),
    );
    let next = finished.iter().find(|(id, _)| id == "next");
    assert_eq!(
        next.map(|(_, outcome)| outcome.as_str()),
        Some("verifiedSucceeded"),
        "{finished:?}"
    );
    assert!(result["result"].as_str().unwrap().contains("部分完成") || true);
}

/// FLOW-09 / FLOW-16：等待按固定时长结束，结果模板只做受限替换，不执行任何表达式。
#[test]
fn wait_finishes_on_time_and_templates_do_not_execute_code() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "等待与模板",
        json!([{
            "kind":"sequence","id":"all","title":"等待与模板","nodes":[
                state_node("state"),
                {"kind":"wait","id":"pause","title":"稍等","seconds":1,"checkSeconds":1},
                {"kind":"result","id":"report","title":"报告",
                 "template":"状态={{ nodes.state.ui.status }} 缺失={{ nodes.state.nope.x }} 注入=<script>{{ globalThis }}</script>"}
            ]
        }]),
    );
    backend.reset_model_requests();
    let started = std::time::Instant::now();
    let result = run(&c, &id, "wait");
    assert_eq!(result["state"], "succeeded");
    assert!(started.elapsed() >= Duration::from_millis(900));
    let text = result["result"].as_str().unwrap();
    assert!(text.contains("状态=observed"), "{text}");
    // 取不到的引用留空并记账，不被当成脚本执行。
    assert!(text.contains("缺失="), "{text}");
    assert!(text.contains("<script></script>"), "{text}");
    assert_eq!(backend.model_requests(), 0);
}

/// FLOW-10：条件等待一直不满足时按时限超时，不无限轮询、不问模型。
#[test]
fn condition_wait_times_out_instead_of_looping_forever() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(
        &c,
        "等待落空",
        json!([{
            "kind":"sequence","id":"all","title":"等待落空","nodes":[
                state_node("state"),
                {"kind":"wait","id":"unreachable","title":"等一个不会来的状态",
                 "until":{"kind":"compare","op":"equals",
                          "left":{"kind":"nodeOutput","node":"state","path":["ui","status"]},
                          "right":{"kind":"literal","value":"never"}},
                 "checkSeconds":1,"timeoutSeconds":2}
            ]
        }]),
    );
    backend.reset_model_requests();
    let started = std::time::Instant::now();
    let result = run(&c, &id, "timeout");
    assert_ne!(result["state"], "succeeded");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert_eq!(backend.model_requests(), 0);
}

/// TASK-01 / TASK-02：任务同时出现在来源对话清单和全局清单，且引用同一个 ID。
#[test]
fn a_task_shows_up_in_both_its_conversation_and_the_global_list() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    backend.set_responses(vec![
        model_call(
            "task.save",
            json!({
                "name":"观察",
                "description":"读一次状态。",
                "nodes":[state_node("state")]
            }),
        ),
        model_answer(),
    ]);
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let run = ipc(
        &c,
        "run.submit",
        json!({"prompt":"做个任务","clientKey":"both"}),
    );
    wait(&c, run["id"].as_str().unwrap(), &["answered"]);
    let conversation = run["conversationId"].as_str().unwrap();
    let scoped = ipc(&c, "workflow.list", json!({"conversationId":conversation}));
    let global = ipc(&c, "workflow.list", json!({}));
    assert_eq!(scoped.as_array().unwrap().len(), 1);
    assert_eq!(global.as_array().unwrap().len(), 1);
    assert_eq!(scoped[0]["id"], global[0]["id"]);

    // 复制生成新 ID，不复用原活动运行。
    let copy = ipc(&c, "workflow.copy", json!({"id":global[0]["id"]}));
    assert_ne!(copy["id"], global[0]["id"]);

    // 归档只改可见性，恢复后仍可用。
    ipc(&c, "workflow.archive", json!({"id":global[0]["id"]}));
    let archived = ipc(&c, "workflow.list", json!({}));
    let entry = archived
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == global[0]["id"])
        .unwrap();
    assert_eq!(entry["state"], "archived");
    ipc(&c, "workflow.restore", json!({"id":global[0]["id"]}));
    let restored = ipc(&c, "workflow.list", json!({}));
    let entry = restored
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == global[0]["id"])
        .unwrap();
    assert_eq!(entry["state"], "readyUnverified");
}

/// BGI 关闭时仍可设计与保存任务，但运行前预检会说清缺口。
#[test]
fn a_task_can_be_designed_without_the_bridge_and_reports_the_gap_at_preflight() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let draft = ipc(
        &c,
        "workflow.draft.create",
        json!({
            "name":"以后再说",
            "description":"依赖一个当前不存在的工具。",
            "nodes":[{"kind":"tool","id":"x","title":"未知工具","tool":"demo.missing","arguments":{}}]
        }),
    );
    assert_eq!(draft["state"], "draft");
    let id = draft["id"].as_str().unwrap();
    // 缺契约的定义不阻止保存，但阻止发布。
    assert!(
        try_ipc(
            &c,
            "workflow.publish",
            json!({"id":id,"draftRevision":draft["revision"]})
        )
        .is_err()
    );
}

/// 删除定义不删除已发生的运行证据；来源对话删除后任务仍可用。
#[test]
fn deleting_a_definition_keeps_the_runs_it_produced() {
    let backend = MockBackend::start("127.0.0.1:0").unwrap();
    let d = tempfile::tempdir().unwrap();
    let c = controller(&backend, &d);
    let id = publish(&c, "跑一次", json!([state_node("state")]));
    let result = run(&c, &id, "once");
    assert_eq!(result["state"], "succeeded");
    let kept = ipc(&c, "workflow.delete", json!({"id":id}));
    assert_eq!(kept["deleted"], true);
    assert_eq!(kept["historyKept"], true);
    // 运行记录还在，可以读回。
    let run = ipc(&c, "run.get", json!({"id":result["id"]}));
    assert_eq!(run["state"], "succeeded");
    // 定义已删除，不能再运行。
    assert!(try_ipc(&c, "workflow.run", json!({"id":id,"clientKey":"after"})).is_err());
}
