//! A shortcut stores one concrete callable action, never an entire conversation plan.
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
    pub action: ShortcutCall,
    #[serde(default)]
    pub prepare: Vec<ShortcutCall>,
    #[serde(default)]
    pub target_name: String,
    #[serde(default)]
    pub application_name: String,
}

pub fn schema() -> Value {
    let call = json!({"type":"object","properties":{"tool":{"type":"string","minLength":1},"arguments":{"type":"object"}},"required":["tool","arguments"],"additionalProperties":false});
    json!({"type":"object","properties":{
        "id":{"type":"string"},"name":{"type":"string","minLength":1},"description":{"type":"string"},
        "binding":{"type":"object","properties":{"action":call,"prepare":{"type":"array","items":call,"maxItems":4},"targetName":{"type":"string"},"applicationName":{"type":"string"}},"required":["action","targetName","applicationName"],"additionalProperties":false}
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
    if binding.prepare.len() > 4
        || !binding.action.arguments.is_object()
        || transient(&binding.action.arguments)
    {
        return Err(Error::Config(
            "快捷任务需要一个稳定的具体目标，不能保存旧运行的临时标识".into(),
        ));
    }
    let mut nodes = vec![];
    for (at, call) in binding.prepare.iter().enumerate() {
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
        nodes.push(tool_node(format!("prepare-{at}"), "检查运行条件", call));
    }
    if catalog.get(&binding.action.tool).is_none() {
        return Err(Error::Config("运行入口当前不可用".into()));
    }
    nodes.push(tool_node(
        "launch".into(),
        &binding.target_name,
        &binding.action,
    ));
    Ok(nodes)
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
            action: ShortcutCall {
                tool: "plugin.run".into(),
                arguments: json!({"name":"named target"}),
            },
            prepare: vec![],
            target_name: "named target".into(),
            application_name: "Other tool".into(),
        };
        assert_eq!(nodes(&binding, &catalog).unwrap().len(), 1);
        binding.prepare.push(binding.action.clone());
        assert!(nodes(&binding, &catalog).is_err());
        binding.prepare.clear();
        binding.action.arguments = json!({"jobId":"old-job"});
        assert!(nodes(&binding, &catalog).is_err());
    }
}
