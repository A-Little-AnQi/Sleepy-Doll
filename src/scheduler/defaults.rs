use super::{Scheduler, model::*};
use crate::{
    Error, Result,
    extension::{ToolDefinition, ToolEffect},
    runtime::operation::{
        shortcuts::ShortcutCall,
        task::{TaskNode, ToolNode, WorkflowRevision},
    },
};
use serde_json::{Value, json};

impl Scheduler {
    /// 仅供可信应用代码登记，不接受插件清单或模型自行声明关闭能力。
    pub fn register_default_action(
        &self,
        provider: &str,
        action: DefaultAction,
        tool: &str,
        arguments: Value,
    ) -> Result<()> {
        if provider.is_empty()
            || tool.is_empty()
            || tool.starts_with("scheduler.")
            || !arguments.is_object()
        {
            return Err(Error::Config("默认动作登记无效".into()));
        }
        self.default_actions.lock().unwrap().insert(
            (provider.into(), action),
            ShortcutCall {
                tool: tool.into(),
                arguments,
            },
        );
        Ok(())
    }

    pub(super) fn default_workflow(
        &self,
        source: &WorkflowRevision,
        action: DefaultAction,
        definitions: &[ToolDefinition],
    ) -> Result<WorkflowRevision> {
        let mut providers = std::collections::HashSet::new();
        for node in &source.nodes {
            let TaskNode::Tool(node) = node else {
                return Err(Error::Config("无法识别前一个任务所属的软件".into()));
            };
            let tool = definitions
                .iter()
                .find(|tool| Some(tool.name.as_str()) == node.tool.as_deref())
                .ok_or_else(|| Error::Conflict("前一个任务的工具已不可用".into()))?;
            if node.execution.as_deref().unwrap_or(&tool.execution).effect != ToolEffect::ReadOnly {
                providers.insert(
                    crate::extension::providers::provider_of_tool(&tool.name, &tool.source)
                        .ok_or_else(|| Error::Config("无法识别前一个任务所属的软件".into()))?,
                );
            }
        }
        if providers.len() != 1 {
            return Err(Error::Config(
                "前一个任务涉及多个软件或没有可识别的软件，无法执行关闭动作".into(),
            ));
        }
        let provider = providers.into_iter().next().unwrap();
        let call = if provider == "bgi" {
            match action {
                DefaultAction::CloseGame => ShortcutCall {
                    tool: "bgi.api.invoke".into(),
                    arguments: json!({"methodId":"bgi.exit_game","arguments":{}}),
                },
                DefaultAction::CloseToolSoftware => ShortcutCall {
                    tool: "bgi.host.close".into(),
                    arguments: json!({}),
                },
            }
        } else {
            self.default_actions
                .lock()
                .unwrap()
                .get(&(provider.clone(), action))
                .cloned()
                .ok_or_else(|| {
                    Error::Config(format!("前一个任务所属软件尚未支持{}", action.title()))
                })?
        };
        let tool = definitions
            .iter()
            .find(|tool| tool.name == call.tool)
            .ok_or_else(|| Error::Conflict(format!("{}接口尚未启用", action.title())))?;
        if crate::extension::providers::provider_of_tool(&tool.name, &tool.source).as_deref()
            != Some(&provider)
            || (provider == "bgi"
                && !matches!(tool.source.as_str(), "core:bgi" | "plugin:bgi:mcp:bridge"))
        {
            return Err(Error::Config("关闭接口不属于前一个任务的软件".into()));
        }
        if provider != "bgi"
            && (tool.execution.model_usage != crate::extension::ModelUsage::None
                || tool.execution.unattended != crate::extension::UnattendedPolicy::Allowed
                || tool.execution.effect_is_dynamic
                || tool.execution.effect == ToolEffect::Unknown
                || tool.execution.risk == crate::extension::RiskLevel::Irreversible)
        {
            return Err(Error::Config("此关闭接口不允许定时执行".into()));
        }
        if !crate::extension::validate(&call.arguments, &tool.input_schema, "$").is_empty() {
            return Err(Error::Config("关闭接口的参数契约已变化".into()));
        }
        let mut workflow = source.clone();
        workflow.name = action.title().into();
        workflow.shortcut = None;
        workflow.limits.max_seconds = 60;
        let mut execution = tool.execution.clone();
        execution.model_usage = crate::extension::ModelUsage::None;
        workflow.nodes = vec![TaskNode::Tool(ToolNode {
            id: "close".into(),
            title: action.title().into(),
            tool: Some(call.tool),
            arguments: call.arguments,
            execution: Some(Box::new(execution)),
            capability_id: None,
            provider_version: tool.provider_version.clone(),
            resource_versions: vec![],
            on_failure: Default::default(),
            on_unverified: Default::default(),
        })];
        Ok(workflow)
    }
}
