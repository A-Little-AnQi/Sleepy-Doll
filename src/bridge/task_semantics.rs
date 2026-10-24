//! BetterGI task grouping and launch semantics belong to the adapter.
use crate::error::Result;
use crate::{
    model::ToolCall,
    runtime::{
        operation,
        types::{Run, RunSource},
    },
};
use serde_json::Value;
use serde_json::json;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;
/// 是否为配置组启动调用（含旧的每组一次接口）。只认方法 ID，不认业务结果
/// 里恰好叫 executionMode 之类的字段。
pub(crate) fn is_group_invoke(call: &ToolCall) -> bool {
    call.name == "bgi.api.invoke"
        && matches!(
            call.arguments["methodId"].as_str().unwrap_or(""),
            "bgi.run_script_group" | "bgi.run_script_groups"
        )
}

/// 是否为无参数的收尾关游戏调用。只认这个可靠形状；带参数的退出语义不明，
/// 不并入合并计划。
pub(crate) fn is_exit_game_invoke(call: &ToolCall) -> bool {
    call.name == "bgi.api.invoke"
        && call.arguments["methodId"].as_str() == Some("bgi.exit_game")
        && call.arguments["arguments"]
            .as_object()
            .is_some_and(|arguments| arguments.is_empty())
}

/// 从一次配置组启动调用里取出可合并的组名与收尾、等待选项。参数形状不可
/// 靠或带有语义不明的额外字段时返回 None，交回正常执行路径给出精确错误，
/// 不猜合并方式。
pub(crate) fn mergeable_group_call(call: &ToolCall) -> Option<(Vec<String>, bool, bool)> {
    let arguments = &call.arguments["arguments"];
    if !arguments.is_object() {
        return None;
    }
    for key in arguments.as_object()?.keys() {
        if !matches!(
            key.as_str(),
            "groupNames"
                | "closeGameAfter"
                | "waitForCompletion"
                | "name"
                | "groupName"
                | "scriptGroupName"
        ) {
            return None;
        }
    }
    let names = match call.arguments["methodId"].as_str()? {
        "bgi.run_script_groups" => arguments["groupNames"]
            .as_array()?
            .iter()
            .map(|value| value.as_str().map(str::to_owned))
            .collect::<Option<Vec<String>>>()?,
        "bgi.run_script_group" => vec![
            arguments["name"]
                .as_str()
                .or_else(|| arguments["groupName"].as_str())
                .or_else(|| arguments["scriptGroupName"].as_str())?
                .to_owned(),
        ],
        _ => return None,
    };
    if names.is_empty() {
        return None;
    }
    let close = arguments["closeGameAfter"].as_bool().unwrap_or(false);
    let wait = arguments["waitForCompletion"].as_bool().unwrap_or(false);
    Some((names, close, wait))
}

/// 纯合并成员分析：只取第一段连续的配置组启动调用；紧随其后、且是整批最后
/// 一个的无参 exit_game 收尾并入 closeGameAfter。中间夹着其它调用、退出出
/// 现在组之前或批中，都不合并。返回成员下标、保序组名与收尾、等待选项。
pub(crate) fn group_launch_members(
    calls: &[ToolCall],
) -> Option<(Vec<usize>, Vec<String>, bool, bool)> {
    let first = calls.iter().position(is_group_invoke)?;
    let mut member_indexes = Vec::new();
    let mut names = Vec::new();
    let mut close = false;
    let mut wait = false;
    let mut index = first;
    while index < calls.len() && is_group_invoke(&calls[index]) {
        let (group, call_close, call_wait) = mergeable_group_call(&calls[index])?;
        member_indexes.push(index);
        names.extend(group);
        close = close || call_close;
        wait = wait || call_wait;
        index += 1;
    }
    if index == calls.len().saturating_sub(1) && is_exit_game_invoke(&calls[index]) {
        // 收尾退出必须在所有组运行动作之后，且是本批最后一项。
        member_indexes.push(index);
        close = true;
    }
    if member_indexes.len() < 2 || names.len() > 32 {
        return None;
    }
    Some((member_indexes, names, close, wait))
}

/// 普通对话的后台交接判定：Agent 来源、非入口封装、配置组启动调用，且桥
/// 结果是明确成功的 launch 交接（分类委托给 executor 的聚合核验，
/// completion／失败／unknown 都不终结，走正常流程）。
pub(crate) fn launch_handoff(
    call: &ToolCall,
    run: &Run,
    output: &Value,
) -> Option<(Vec<String>, bool)> {
    if !matches!(run.source, RunSource::Agent)
        || run.shortcut_configuration
        || call.name != "bgi.api.invoke"
    {
        return None;
    }
    if !operation::executor::launch_submitted(output) {
        return None;
    }
    if is_group_invoke(call) {
        let (names, close, wait) = mergeable_group_call(call)?;
        return (!wait).then_some((names, close));
    }
    if call.arguments["methodId"] != "bgi.run_one_dragon"
        || call.arguments["arguments"]["waitForCompletion"] == true
    {
        return None;
    }
    let name = output
        .pointer("/evidence/result/name")
        .and_then(Value::as_str)
        .or_else(|| call.arguments["arguments"]["name"].as_str());
    Some((name.into_iter().map(str::to_owned).collect(), false))
}

/// 后台交接的确定性最终消息：短、自然，只陈述已提交范围与收尾方式，不暗示
/// 业务已完成，也不向用户复述「同批调用／工具结果」等内部字段；确有无法执
/// 行的后续任务时点名数量。
pub(crate) fn handoff_message(names: &[String], close: bool, pending_tasks: usize) -> String {
    let mut text = if names.is_empty() {
        "BetterGI 已接收任务，将在后台按顺序运行。".to_owned()
    } else {
        let listed = names
            .iter()
            .map(|name| format!("「{name}」"))
            .collect::<Vec<_>>()
            .join("");
        if close {
            format!("已交给 BetterGI 运行{listed}，全部完成后会关闭原神。")
        } else {
            format!("已交给 BetterGI 运行{listed}。")
        }
    };
    if pending_tasks > 0 {
        text.push_str(&format!(
            "后续 {pending_tasks} 项任务尚未执行，需要时再告诉我。"
        ));
    }
    text
}

/// 同批配置组启动调用归一成一次 run_script_groups 的执行计划。
pub(crate) struct GroupLaunchPlan {
    /// 运行时内部使用的合并调用；不进入 assistant 轮，不单独记工具历史。
    pub(crate) merged: ToolCall,
    /// 原调用在批次里的下标，各自拿到配对结果。
    pub(crate) member_indexes: Vec<usize>,
    /// 合并后的保序组名，用于最终消息。
    pub(crate) names: Vec<String>,
    pub(crate) close: bool,
    pub(crate) wait: bool,
}

/// 同批出现多个配置组启动调用时，先把它们归一成一次 run_script_groups
/// 后台交接：保序合并 groupNames，任一要求关闭游戏就 closeGameAfter，
/// 任一显式 waitForCompletion=true 则整体保持显式等待。合并目标契约由
/// 这里读取并登记 exposed，因此不产生模型可见的额外工具轮次；契约不支
/// 持批量或参数不可合并时返回 None，走原路径让原调用给出精确错误。
pub(crate) async fn group_launch_plan(
    calls: &[ToolCall],
    bridge: &crate::runtime::host::bridge::Bridge,
    cancel: &CancellationToken,
    exposed: &mut HashSet<String>,
) -> Result<Option<GroupLaunchPlan>> {
    // 成员分析是纯函数：连续组启动调用 + 可选尾部无参 exit_game。
    let Some((member_indexes, names, close, wait)) = group_launch_members(calls) else {
        return Ok(None);
    };
    let contract = bridge.describe("bgi.run_script_groups", cancel).await?;
    if contract["inputSchema"]["properties"]["groupNames"].is_null() {
        return Ok(None);
    }
    let Some(version) = contract["catalogVersion"]
        .as_str()
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };
    exposed.insert(format!("bridge-api:bgi.run_script_groups:{version}"));
    let mut arguments = json!({"groupNames":names});
    if close {
        arguments["closeGameAfter"] = json!(true);
    }
    if wait {
        arguments["waitForCompletion"] = json!(true);
    }
    Ok(Some(GroupLaunchPlan {
        merged: ToolCall {
            id: format!("call_{}", uuid::Uuid::new_v4()),
            name: "bgi.api.invoke".into(),
            arguments: json!({"methodId":"bgi.run_script_groups","arguments":arguments}),
        },
        member_indexes,
        names,
        close,
        wait,
    }))
}

impl crate::runtime::host::bridge::Bridge {
    pub(crate) fn requires_order(&self, calls: &[ToolCall]) -> bool {
        calls.iter().any(is_group_invoke)
    }

    pub(crate) async fn prepare_batch(
        &self,
        calls: &[ToolCall],
        cancel: &CancellationToken,
        exposed: &mut HashSet<String>,
    ) -> Result<Option<crate::runtime::host::dispatch::BatchPlan>> {
        Ok(group_launch_plan(calls, self, cancel, exposed)
            .await?
            .map(|plan| crate::runtime::host::dispatch::BatchPlan {
                merged: plan.merged,
                member_indexes: plan.member_indexes,
                result_kind: "bgi.run_script_groups".into(),
                handoff: (!plan.wait).then(|| crate::runtime::host::dispatch::Handoff {
                    summary: handoff_message(&plan.names, plan.close, 0),
                }),
            }))
    }

    pub(crate) fn task_handoff(
        &self,
        call: &ToolCall,
        run: &Run,
        output: &Value,
    ) -> Option<crate::runtime::host::dispatch::Handoff> {
        launch_handoff(call, run, output).map(|(names, close)| {
            crate::runtime::host::dispatch::Handoff {
                summary: handoff_message(&names, close, 0),
            }
        })
    }
}
