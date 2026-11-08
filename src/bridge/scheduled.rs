//! 定时执行接纳固定配置组和经当前宿主只读校验的命令计划；不改写原快捷任务。
use crate::{
    Error, Result,
    runtime::operation::task::{FailurePolicy, TaskNode, ToolNode, WorkflowRevision},
};
use serde_json::{Value, json};

fn raw_command(node: &TaskNode) -> Option<&ToolNode> {
    match node {
        TaskNode::Tool(call)
            if call.tool.as_deref() == Some("bgi.api.invoke")
                && call.arguments["methodId"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("cmd.")) =>
        {
            Some(call)
        }
        _ => None,
    }
}

/// 旧入口的固定命令与等待共用一个运行期上下文；不保存原生对象，也不执行命令。
pub(crate) fn normalize_legacy_commands(
    workflow: &mut WorkflowRevision,
    config: crate::config::BridgeConfig,
) -> Result<()> {
    if !workflow
        .nodes
        .iter()
        .any(|node| raw_command(node).is_some())
    {
        return Ok(());
    }
    let client = super::BgiClient::new(config);
    workflow.nodes = legacy_command_plans(&workflow.nodes, |method| client.describe(method))?;
    workflow.validation.node_count = workflow.nodes.len();
    Ok(())
}

fn legacy_command_plans(
    nodes: &[TaskNode],
    mut describe: impl FnMut(&str) -> Result<Value>,
) -> Result<Vec<TaskNode>> {
    let mut result = Vec::new();
    let mut at = 0;
    while at < nodes.len() {
        let Some(first) = raw_command(&nodes[at]) else {
            result.push(nodes[at].clone());
            at += 1;
            continue;
        };
        let mut plan = first.clone();
        let mut steps = Vec::new();
        let mut game = false;
        while at < nodes.len() {
            if let Some(call) = raw_command(&nodes[at]) {
                if call.on_failure != FailurePolicy::Stop
                    || call.on_unverified != FailurePolicy::Stop
                    || call.capability_id.is_some()
                    || call.provider_version != first.provider_version
                    || call.arguments.as_object().is_none_or(|args| {
                        args.keys()
                            .any(|key| key != "methodId" && key != "arguments")
                    })
                {
                    return Err(Error::Config(
                        "此旧入口需重新封装，不能改变原有执行策略或上下文参数".into(),
                    ));
                }
                let args = call.arguments["arguments"]
                    .as_object()
                    .ok_or_else(|| Error::Config("旧命令入口缺少明确参数".into()))?;
                if args
                    .iter()
                    .any(|(key, value)| key != "argument" || value.is_object() || value.is_array())
                {
                    return Err(Error::Config(
                        "此旧入口包含界面上下文或对象参数，需先封装稳定的运行期选择规则".into(),
                    ));
                }
                let method = call.arguments["methodId"].as_str().expect("固定命令");
                let contract = describe(method)?;
                if contract["methodId"] != method || contract["callable"] != true {
                    return Err(Error::Config("旧入口的命令当前不可用".into()));
                }
                match contract["effect"].as_str() {
                    Some("hostCommand") => {}
                    Some("gameWrite") => game = true,
                    _ => return Err(Error::Config("旧入口的命令作用范围无法确认".into())),
                }
                let mut step = json!({"command":method,"title":call.title});
                if let Some(argument) = args.get("argument") {
                    step["argument"] = argument.clone();
                }
                steps.push(step);
                plan.resource_versions
                    .extend(call.resource_versions.iter().cloned());
            } else if let TaskNode::Wait(wait) = &nodes[at]
                && wait
                    .seconds
                    .is_some_and(|seconds| (1..=3600).contains(&seconds))
                && wait.until.is_none()
                && wait.probe.is_none()
                && wait.timeout_seconds.is_none()
            {
                steps.push(json!({"waitSeconds":wait.seconds,"title":wait.title}));
            } else {
                break;
            }
            if steps.len() > 16 {
                return Err(Error::Config(
                    "旧入口的连续命令超过计划上限，需拆分为独立入口".into(),
                ));
            }
            at += 1;
        }
        plan.resource_versions.sort();
        plan.resource_versions.dedup();
        plan.arguments = json!({"methodId":if game {"bgi.run_game_command_plan"} else {"bgi.run_host_command_plan"},"arguments":{"steps":steps}});
        result.push(TaskNode::Tool(plan));
    }
    Ok(result)
}

pub(crate) fn command_plan_mode(node: &ToolNode) -> Option<&'static str> {
    if node.tool.as_deref() != Some("bgi.api.invoke") {
        return None;
    }
    match node.arguments["methodId"].as_str() {
        Some("bgi.run_host_command_plan") => Some("host"),
        Some("bgi.run_game_command_plan") => Some("game"),
        _ => None,
    }
}

/// 只读校验由宿主真实命令目录决定 effect、参数类型和无交互边界，不在创建阶段执行计划。
pub(crate) fn bind_command_plan(
    node: &mut ToolNode,
    config: crate::config::BridgeConfig,
) -> Result<()> {
    let mode =
        command_plan_mode(node).ok_or_else(|| Error::Config("不是受控原生命令计划".into()))?;
    let client = super::BgiClient::new(config);
    let contract = client.describe("bgi.validate_command_plan").map_err(|error|Error::Config(format!("当前连接未取得命令计划校验能力：{}。先核对桥的连接与能力；更新后需停用旧桥再连接，目录仍无新能力时再重启 BetterGI。当前任务尚未注册，不能据此断言目标不可实现。",error.user_message())))?;
    if contract["effect"] != "readOnly" || contract["callable"] != true {
        return Err(Error::Config(
            "当前桥未提供只读命令计划校验；请更新并重新连接桥，不能跳过校验".into(),
        ));
    }
    let arguments = json!({"mode":mode,"plan":node.arguments["arguments"]});
    let issues = crate::extension::validate(&arguments, &contract["inputSchema"], "$");
    if !issues.is_empty() {
        return Err(Error::Config(
            "命令计划参数不符合当前桥契约，请读取 bgi.validate_command_plan 的说明".into(),
        ));
    }
    let response = client.invoke("bgi.validate_command_plan", &arguments)?;
    let result = &response["result"];
    pin_command_plan(node, result)?;
    Ok(())
}

fn pin_command_plan(node: &mut ToolNode, result: &Value) -> Result<()> {
    if result["valid"] != true || result["executed"] != false {
        return Err(Error::Config("宿主没有返回有效的只读计划校验结果".into()));
    }
    if result["requiresGameReady"] != json!(command_plan_mode(node) == Some("game")) {
        return Err(Error::Config(
            "命令计划入口与真实目录的游戏作用范围不一致，未登记".into(),
        ));
    }
    let hash = result["contractHash"]
        .as_str()
        .filter(|h| {
            h.len() == 64
                && h.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
        .ok_or_else(|| Error::Config("命令计划缺少可核验契约指纹".into()))?;
    let arguments = node
        .arguments
        .get_mut("arguments")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| Error::Config("命令计划缺少参数对象".into()))?;
    arguments.insert("expectedContractHash".into(), json!(hash));
    Ok(())
}

pub(crate) fn prepare(node: &mut ToolNode) -> Result<bool> {
    if node.tool.as_deref() != Some("bgi.api.invoke") {
        return Ok(false);
    }
    if !matches!(
        node.arguments["methodId"].as_str(),
        Some("bgi.run_script_groups" | "bgi.run_script_group")
    ) {
        return Err(Error::Config("此入口使用原始界面调用，需要先封装为运行时解析目标的命令计划；当前绑定不适合定时，不代表目标不可实现。".into()));
    }
    let arguments = node.arguments["arguments"]
        .as_object_mut()
        .ok_or_else(|| Error::Config("配置组任务缺少稳定参数".into()))?;
    arguments.insert("waitForCompletion".into(), true.into());
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(method: &str, arguments: Value) -> TaskNode {
        let mut call = node(method);
        call.arguments["arguments"] = arguments;
        TaskNode::Tool(call)
    }
    fn contract(method: &str) -> Result<Value> {
        Ok(json!({"methodId":method,"callable":true,"effect":"hostCommand"}))
    }
    #[test]
    fn legacy_commands_share_one_runtime_context_and_preserve_waits() {
        let nodes = vec![
            command("cmd.main.open", json!({})),
            serde_json::from_value(
                json!({"kind":"wait","id":"pause","title":"等待","seconds":5,"checkSeconds":5}),
            )
            .unwrap(),
            command("cmd.feed.refresh", json!({"argument":"current"})),
        ];
        let snapshot = nodes.clone();
        let converted = legacy_command_plans(&nodes, contract).unwrap();
        assert_eq!(nodes, snapshot);
        assert_eq!(converted.len(), 1);
        let TaskNode::Tool(call) = &converted[0] else {
            panic!("命令计划")
        };
        assert_eq!(command_plan_mode(call), Some("host"));
        let steps = &call.arguments["arguments"]["steps"];
        assert_eq!(steps[0]["command"], "cmd.main.open");
        assert_eq!(steps[1]["waitSeconds"], 5);
        assert_eq!(steps[2]["command"], "cmd.feed.refresh");
        assert_eq!(steps[2]["argument"], "current");
        let game = legacy_command_plans(&nodes, |method| {
            Ok(json!({"methodId":method,"callable":true,"effect":"gameWrite"}))
        })
        .unwrap();
        let TaskNode::Tool(game) = &game[0] else {
            panic!("游戏命令计划")
        };
        assert_eq!(command_plan_mode(game), Some("game"));
    }
    #[test]
    fn legacy_conversion_rejects_transient_targets_and_changed_policies() {
        for arguments in [
            json!({"contextId":"old"}),
            json!({"argument":{"objectId":"old"}}),
            json!({"selection":{}}),
            json!({"dialogInput":{"text":"name"}}),
        ] {
            assert!(
                legacy_command_plans(&[command("cmd.main.open", arguments)], contract).is_err()
            );
        }
        let mut original = node("cmd.main.open");
        original.arguments["arguments"] = json!({});
        original.on_failure = FailurePolicy::ContinueIndependent;
        assert!(legacy_command_plans(&[TaskNode::Tool(original)], contract).is_err());
        let raw = [command("cmd.main.open", json!({}))];
        for bad in [
            json!({"methodId":"cmd.other.open","callable":true,"effect":"hostCommand"}),
            json!({"methodId":"cmd.main.open","callable":false,"effect":"hostCommand"}),
            json!({"methodId":"cmd.main.open","callable":true,"effect":"unknown"}),
        ] {
            assert!(legacy_command_plans(&raw, |_| Ok(bad.clone())).is_err());
        }
    }
    #[test]
    #[ignore = "显式提供隔离的 SLEEPY_DOLL_SCHEDULE_BINDING_FIXTURE，仅调用真实桥的只读校验"]
    fn live_legacy_binding_validates_without_executing_commands() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            bridge: crate::config::BridgeConfig,
            workflow: WorkflowRevision,
        }
        let path =
            std::env::var_os("SLEEPY_DOLL_SCHEDULE_BINDING_FIXTURE").expect("缺少隔离验证输入");
        let mut fixture: Fixture = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let original = fixture.workflow.nodes.clone();
        assert!(original.iter().any(|node| raw_command(node).is_some()));
        normalize_legacy_commands(&mut fixture.workflow, fixture.bridge.clone()).unwrap();
        let mut plans = 0;
        let mut steps = 0;
        for node in &mut fixture.workflow.nodes {
            if let TaskNode::Tool(call) = node
                && command_plan_mode(call).is_some()
            {
                bind_command_plan(call, fixture.bridge.clone()).unwrap();
                assert_eq!(
                    call.arguments["arguments"]["expectedContractHash"]
                        .as_str()
                        .unwrap()
                        .len(),
                    64
                );
                plans += 1;
                steps += call.arguments["arguments"]["steps"]
                    .as_array()
                    .unwrap()
                    .len();
            }
        }
        assert!(plans > 0);
        assert_eq!(steps, original.len());
        println!(
            "Read-only legacy binding validated: plans={plans}, orderedSteps={steps}; no execution requested"
        );
    }
    fn node(method: &str) -> ToolNode {
        serde_json::from_value(json!({"id":"plan","title":"命令计划","tool":"bgi.api.invoke","arguments":{"methodId":method,"arguments":{"steps":[{"command":"cmd.sample.refresh","title":"刷新"}]}}})).unwrap()
    }
    #[test]
    fn native_command_plan_pins_only_readonly_validated_scopes() {
        let mut call = node("bgi.run_host_command_plan");
        let valid = json!({"valid":true,"executed":false,"requiresGameReady":false,"contractHash":"a".repeat(64)});
        pin_command_plan(&mut call, &valid).unwrap();
        assert_eq!(
            call.arguments["arguments"]["expectedContractHash"],
            "a".repeat(64)
        );
        for patch in [
            json!({"valid":false}),
            json!({"executed":true}),
            json!({"requiresGameReady":true}),
            json!({"contractHash":"bad"}),
        ] {
            let mut reply = valid.clone();
            reply
                .as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            assert!(pin_command_plan(&mut node("bgi.run_host_command_plan"), &reply).is_err());
        }
        assert_eq!(command_plan_mode(&node("cmd.sample.refresh")), None);
    }
    #[test]
    fn raw_command_rejection_points_to_binding_repair_not_goal_impossibility() {
        let error = prepare(&mut node("cmd.feed_window.get_live_redeem_codes"))
            .unwrap_err()
            .user_message();
        assert!(error.contains("不代表目标不可实现"));
        assert!(error.contains("运行时解析目标"));
    }
}
