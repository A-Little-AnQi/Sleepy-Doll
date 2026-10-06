//! A shortcut stores concrete calls in execution order, without a model at run time.
use super::task::{FailurePolicy, TaskNode, ToolCatalog, ToolNode};
use crate::{
    error::{Error, Result},
    extension::ToolEffect,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShortcutCall {
    pub tool: String,
    pub arguments: Value,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShortcutBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ShortcutCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ShortcutStep>,
    #[serde(default)]
    pub prepare: Vec<ShortcutCall>,
    #[serde(default)]
    pub target_name: String,
    #[serde(default)]
    pub application_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShortcutStep {
    pub title: String,
    #[serde(default)]
    pub prepare: Vec<ShortcutCall>,
    pub action: ShortcutCall,
}

pub fn schema() -> Value {
    let call = json!({"type":"object","properties":{"tool":{"type":"string","minLength":1},"arguments":{"type":"object"}},"required":["tool","arguments"],"additionalProperties":false});
    json!({"type":"object","properties":{
        "id":{"type":"string"},"name":{"type":"string","minLength":1},"description":{"type":"string"},
        "binding":{"type":"object","properties":{"action":call,"steps":{"type":"array","minItems":1,"items":{"type":"object","properties":{"title":{"type":"string","minLength":1},"prepare":{"type":"array","items":call,"maxItems":4},"action":call},"required":["title","action"],"additionalProperties":false}},"prepare":{"type":"array","items":call,"maxItems":4},"targetName":{"type":"string"},"applicationName":{"type":"string"}},"required":["targetName","applicationName"],"oneOf":[{"required":["action"],"not":{"required":["steps"]}},{"required":["steps"],"not":{"required":["action"]}}],"additionalProperties":false}
    },"required":["name","binding"],"additionalProperties":false})
}
fn transient(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            matches!(
                key.as_str(),
                "jobId" | "requestId" | "contextId" | "objectId" | "attemptId"
            ) || transient(value)
        }),
        Value::Array(items) => items.iter().any(transient),
        _ => false,
    }
}
pub fn nodes(binding: &ShortcutBinding, catalog: &ToolCatalog) -> Result<Vec<TaskNode>> {
    if binding.action.is_some() == !binding.steps.is_empty() {
        return Err(Error::Config(
            "快捷任务需要一个运行动作或一组按顺序执行的步骤".into(),
        ));
    }
    let mut nodes = vec![];
    preparation(&mut nodes, "prepare", &binding.prepare, catalog)?;
    if let Some(action) = &binding.action {
        action_node(
            &mut nodes,
            "launch".into(),
            &binding.target_name,
            action,
            catalog,
        )?;
    }
    for (at, step) in binding.steps.iter().enumerate() {
        if step.title.trim().is_empty() {
            return Err(Error::Config("请指定每个步骤的名称".into()));
        }
        preparation(
            &mut nodes,
            &format!("step-{at}-prepare"),
            &step.prepare,
            catalog,
        )?;
        action_node(
            &mut nodes,
            format!("step-{at}"),
            &step.title,
            &step.action,
            catalog,
        )?;
    }
    Ok(nodes)
}

fn preparation(
    nodes: &mut Vec<TaskNode>,
    prefix: &str,
    calls: &[ShortcutCall],
    catalog: &ToolCatalog,
) -> Result<()> {
    if calls.len() > 4 {
        return Err(Error::Config("每个步骤最多附加四个只读准备调用".into()));
    }
    for (at, call) in calls.iter().enumerate() {
        let contract = catalog
            .get(&call.tool)
            .ok_or_else(|| Error::Config("准备工具当前不可用".into()))?;
        if contract.execution.effect != ToolEffect::ReadOnly
            || !call.arguments.is_object()
            || transient(&call.arguments)
        {
            return Err(Error::Config(
                "快捷任务的准备阶段只能读取和核对当前目标".into(),
            ));
        }
        validate_against_schema(&call.tool, &call.arguments, &contract.input_schema)?;
        nodes.push(tool_node(format!("{prefix}-{at}"), "检查运行条件", call));
    }
    Ok(())
}

fn action_node(
    nodes: &mut Vec<TaskNode>,
    id: String,
    title: &str,
    call: &ShortcutCall,
    catalog: &ToolCatalog,
) -> Result<()> {
    if !call.arguments.is_object() || transient(&call.arguments) {
        return Err(Error::Config(
            "快捷任务需要稳定目标参数，不能保存旧运行的临时标识".into(),
        ));
    }
    if matches!(
        call.tool.as_str(),
        "shortcut.save" | "task.save" | "task.schema" | "user.ask"
    ) {
        return Err(Error::Config("不能把保存入口或交互问答作为运行动作".into()));
    }
    let contract = catalog
        .get(&call.tool)
        .ok_or_else(|| Error::Config("运行入口当前不可用".into()))?;
    // 保存时按目录真实 input_schema 校验参数形状；扁平化的宿主参数
    // （如丢失 methodId/内层 arguments 的 bgi.api.invoke）在此拒绝。
    validate_against_schema(&call.tool, &call.arguments, &contract.input_schema)?;
    nodes.push(tool_node(id, title, call));
    Ok(())
}
/// 与运行时工具调用同一实现的外层参数校验；错误信息带工具名便于定位。
fn validate_against_schema(tool: &str, arguments: &Value, schema: &Value) -> Result<()> {
    let errors = crate::extension::validate(arguments, schema, "$");
    if errors.is_empty() {
        return Ok(());
    }
    let first = errors[0]["message"].as_str().unwrap_or("参数不符合契约");
    let path = errors[0]["path"].as_str().unwrap_or("$");
    Err(Error::Config(format!(
        "入口绑定 {tool} 的参数不符合契约（{path}）：{first}；请按契约层次填写，编辑已有入口只改内层参数"
    )))
}

fn tool_node(id: String, title: &str, call: &ShortcutCall) -> TaskNode {
    TaskNode::Tool(ToolNode {
        id,
        title: title.into(),
        tool: Some(call.tool.clone()),
        capability_id: None,
        arguments: call.arguments.clone(),
        execution: None,
        provider_version: None,
        resource_versions: vec![],
        on_failure: FailurePolicy::default(),
        on_unverified: FailurePolicy::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ToolExecution;
    use crate::runtime::operation::task::ToolContract;
    #[test]
    fn binding_rejects_write_preparation_and_old_job_identity() {
        let mut catalog = ToolCatalog::new();
        catalog.insert(
            "plugin.run",
            ToolContract {
                execution: ToolExecution::default(),
                provider_version: None,
                input_schema: json!({"type":"object"}),
            },
        );
        let mut binding = ShortcutBinding {
            action: Some(ShortcutCall {
                tool: "plugin.run".into(),
                arguments: json!({"name":"named target"}),
            }),
            steps: vec![],
            prepare: vec![],
            target_name: "named target".into(),
            application_name: "Other tool".into(),
        };
        assert_eq!(nodes(&binding, &catalog).unwrap().len(), 1);
        binding.prepare.push(binding.action.clone().unwrap());
        assert!(nodes(&binding, &catalog).is_err());
        binding.prepare.clear();
        binding.action.as_mut().unwrap().arguments = json!({"jobId":"old-job"});
        assert!(nodes(&binding, &catalog).is_err());
    }

    fn read_only_catalog(catalog: &mut ToolCatalog, names: &[&str]) {
        for name in names {
            catalog.insert(
                name,
                ToolContract {
                    execution: ToolExecution {
                        effect: ToolEffect::ReadOnly,
                        ..ToolExecution::default()
                    },
                    provider_version: None,
                    input_schema: json!({"type":"object"}),
                },
            );
        }
    }

    fn call(tool: &str) -> ShortcutCall {
        ShortcutCall {
            tool: tool.into(),
            arguments: json!({"name":"named target"}),
        }
    }

    /// 用生产 BgiClient 登记的真实目录校验：扁平化或缺失分层的宿主参数
    /// 在保存时必须被拒，合法的单组／多组绑定保持通过。
    #[test]
    fn bindings_validate_against_production_catalog_schemas() {
        use crate::bridge::{BgiClient, register_tools};
        let mut registry = crate::extension::ToolRegistry::default();
        let client = BgiClient::new(crate::config::BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:0".into(),
            token: None,
            instance_id: None,
            timeout_ms: 1000,
            host_install_path: None,
            auto_start: true,
            launch_silently: false,
        });
        register_tools(&mut registry, std::sync::Arc::new(client)).unwrap();
        let mut catalog = ToolCatalog::new();
        for definition in registry.definitions() {
            if definition.name == "bgi.api.invoke" || definition.name == "bgi.api.describe" {
                catalog.insert(
                    &definition.name,
                    ToolContract {
                        execution: definition.execution.clone(),
                        provider_version: definition.provider_version.clone(),
                        input_schema: definition.input_schema.clone(),
                    },
                );
            }
        }
        let base = |action_arguments: Value| ShortcutBinding {
            action: Some(ShortcutCall {
                tool: "bgi.api.invoke".into(),
                arguments: action_arguments,
            }),
            steps: vec![],
            prepare: vec![ShortcutCall {
                tool: "bgi.api.describe".into(),
                arguments: json!({"methodId":"bgi.run_script_groups"}),
            }],
            target_name: "挖矿讨伐".into(),
            application_name: "BetterGI".into(),
        };
        // 正例：单组与多组都按契约分层。
        let single = base(json!({
            "methodId":"bgi.run_script_group",
            "arguments":{"groupName":"挖矿讨伐","waitForCompletion":false}
        }));
        assert!(nodes(&single, &catalog).is_ok(), "单组分层绑定应通过");
        let multi = base(json!({
            "methodId":"bgi.run_script_groups",
            "arguments":{"groupNames":["挖矿讨伐","兽怪暴徒"],"closeGameAfter":true,"waitForCompletion":false}
        }));
        assert!(nodes(&multi, &catalog).is_ok(), "多组分层绑定应通过");
        // 负例：扁平化宿主参数（丢失 methodId 与内层 arguments）。
        let flat = base(json!({"groupName":"挖矿讨伐","waitForCompletion":false}));
        assert!(nodes(&flat, &catalog).is_err(), "扁平参数必须被拒");
        // 负例：缺 methodId。
        let missing_method = base(json!({"arguments":{"groupName":"挖矿讨伐"}}));
        assert!(
            nodes(&missing_method, &catalog).is_err(),
            "缺 methodId 必须被拒"
        );
        // 负例：缺内层 arguments。
        let missing_inner = base(json!({"methodId":"bgi.run_script_group"}));
        assert!(
            nodes(&missing_inner, &catalog).is_err(),
            "缺内层 arguments 必须被拒"
        );
    }

    #[test]
    fn binding_requires_exactly_one_of_action_or_steps() {
        let mut catalog = ToolCatalog::new();
        read_only_catalog(&mut catalog, &["plugin.read", "plugin.run"]);
        let base = |action: Option<ShortcutCall>, steps: Vec<ShortcutStep>| ShortcutBinding {
            action,
            steps,
            prepare: vec![],
            target_name: "named target".into(),
            application_name: "Other tool".into(),
        };
        let mut binding = base(
            Some(call("plugin.run")),
            vec![ShortcutStep {
                title: "第二段".into(),
                prepare: vec![],
                action: call("plugin.run"),
            }],
        );
        // 动作与步骤混用拒绝。
        assert!(nodes(&binding, &catalog).is_err());
        binding.action = None;
        // 多步保序：每步 prepare 在该步 action 之前，步骤按数组顺序。
        binding.steps = vec![
            ShortcutStep {
                title: "第一段".into(),
                prepare: vec![call("plugin.read")],
                action: call("plugin.run"),
            },
            ShortcutStep {
                title: "第二段".into(),
                prepare: vec![call("plugin.read")],
                action: call("plugin.run"),
            },
            ShortcutStep {
                title: "第三段".into(),
                prepare: vec![],
                action: call("plugin.run"),
            },
        ];
        let compiled = nodes(&binding, &catalog).unwrap();
        let ids: Vec<&str> = compiled
            .iter()
            .map(|node| match node {
                TaskNode::Tool(tool) => tool.id.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(
            ids,
            vec![
                "step-0-prepare-0",
                "step-0",
                "step-1-prepare-0",
                "step-1",
                "step-2"
            ]
        );
        // 两者都缺拒绝。
        let binding = base(None, vec![]);
        assert!(nodes(&binding, &catalog).is_err());
    }
}
