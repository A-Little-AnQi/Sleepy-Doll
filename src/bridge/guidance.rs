//! Model-facing call metadata; transport readiness and game readiness are distinct.
use serde_json::{Value, json};

pub(crate) fn contract(mut value: Value) -> Value {
    let Some(id) = value["methodId"].as_str().map(str::to_owned) else {
        return value;
    };
    let tool = if value["effect"] == "readOnly" {
        "bgi.api.read"
    } else {
        "bgi.api.invoke"
    };
    value["agentCall"] = json!({
        "tool":tool,
        "methodId":id,
        "argumentsShape":"{methodId, arguments: <符合 inputSchema 的对象>}",
        "shortcutPrepare":[{"tool":"bgi.api.describe","arguments":{"methodId":id}}],
        "savingRequiresGameReady":false,
        "note":"methodId 是 BetterGI 接口标识，不是 Agent 工具名。快捷入口使用上述 tool 和参数层级；保存不执行目标，游戏就绪条件只在运行时检查。callable=false 时按 unavailableReason 处理，不推断为连接失败。示例只展示结构，名称、路径、对象引用与版本哈希必须取自本次真实证据。"
    });
    value
}

pub(crate) fn state(mut value: Value) -> Value {
    if !value["runtime"].is_object() {
        return value;
    }
    value["bridgeConnected"] = json!(true);
    if !value["gameReady"].is_boolean() {
        value["gameReady"] = value["bridgeReady"].clone();
    }
    value["stateMeaning"] = json!(
        "本次状态接口已响应，BetterGI 连接正常。gameReady（旧字段 bridgeReady）只表示截图器和游戏主界面就绪；false 不代表断连，不阻止读取或保存配置、查询接口、创建快捷入口。故障原因以失败操作的具体错误为准。"
    );
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_not_running_is_not_a_disconnected_bridge() {
        let result = state(
            json!({"bridgeReady":false,"runtime":{"hostLoaded":true,"captureReady":false,"gameProcessRunning":false}}),
        );
        assert_eq!(result["bridgeConnected"], true);
        assert_eq!(result["gameReady"], false);
        assert_eq!(result["bridgeReady"], false);
        assert_eq!(
            state(json!({"error":"unavailable"})),
            json!({"error":"unavailable"})
        );
    }

    #[test]
    fn call_mapping_preserves_contract_and_distinguishes_read_from_write() {
        for (effect, tool) in [
            ("readOnly", "bgi.api.read"),
            ("gameWrite", "bgi.api.invoke"),
        ] {
            let schema = json!({"type":"object","properties":{"groupName":{"type":"string"}},"required":["groupName"]});
            let result = contract(
                json!({"methodId":"bgi.run_script_group","callable":true,"effect":effect,"inputSchema":schema}),
            );
            assert_eq!(result["inputSchema"], schema);
            assert_eq!(result["agentCall"]["tool"], tool);
            assert_eq!(result["agentCall"]["methodId"], "bgi.run_script_group");
            assert_eq!(
                result["agentCall"]["shortcutPrepare"][0]["arguments"]["methodId"],
                result["methodId"]
            );
            assert_eq!(result["agentCall"]["savingRequiresGameReady"], false);
        }
        let disabled = contract(
            json!({"methodId":"bgi.disabled","effect":"gameWrite","callable":false,"unavailableReason":"removed"}),
        );
        assert_eq!(disabled["callable"], false);
        assert_eq!(disabled["unavailableReason"], "removed");
    }
}
