//! 定时执行接纳固定配置组和经当前宿主只读校验的命令计划；不改写原快捷任务。
use crate::{Error, Result, runtime::operation::task::ToolNode};
use serde_json::{Value, json};

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
