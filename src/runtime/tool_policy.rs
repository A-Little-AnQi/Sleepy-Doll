use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::{
    extension::{ToolDefinition, ToolEffect, ToolExposure},
    model::{Message, Role},
};

use super::operation::permissions::PermissionMode;

pub struct TurnTools {
    read: HashSet<String>,
    written: HashSet<String>,
    discovered: HashSet<String>,
    used: HashSet<String>,
    mode: PermissionMode,
    shortcut: bool,
}

impl TurnTools {
    pub fn new(
        definitions: &[ToolDefinition],
        history: &[Message],
        discovered: &HashSet<String>,
        mode: PermissionMode,
        shortcut: bool,
    ) -> Self {
        let by_name = definitions
            .iter()
            .map(|tool| (tool.name.as_str(), tool))
            .collect::<HashMap<_, _>>();
        let mut calls = HashMap::new();
        let mut policy = Self {
            read: HashSet::new(),
            written: HashSet::new(),
            discovered: discovered.clone(),
            used: HashSet::new(),
            mode,
            shortcut,
        };
        for message in history {
            for call in &message.tool_calls {
                calls.insert(call.id.as_str(), call.name.as_str());
                policy.used.insert(call.name.clone());
            }
            if message.role != Role::Tool {
                continue;
            }
            let Some(tool) = message
                .tool_call_id
                .as_deref()
                .and_then(|id| calls.get(id))
                .and_then(|name| by_name.get(name))
            else {
                continue;
            };
            let Ok(result) = serde_json::from_str::<Value>(&message.content) else {
                continue;
            };
            if result["ok"] != true || result["value"].is_null() {
                continue;
            }
            if result["value"]["untrusted"] == true || tool.source == "core:web" {
                continue;
            }
            if tool.name == "search.batch" {
                for name in result["value"]["searchEvidence"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if let Some(provider) = by_name.get(name)
                        && provider.execution.search_provider
                        && provider.execution.effect == ToolEffect::ReadOnly
                        && name != "tools.search"
                        && provider.source != "core:web"
                    {
                        policy.read.insert(provider.source.clone());
                    }
                }
                continue;
            }
            if tool.execution.effect == ToolEffect::ReadOnly {
                if !matches!(tool.name.as_str(), "workspace.environment" | "tools.search")
                    && result["value"]["total"] != 0
                    && result["value"]["found"] != false
                    && result["value"]["resolved"] != false
                    && result["value"]["verdict"] != "notFound"
                    && !result["value"]
                        .as_array()
                        .is_some_and(|items| items.is_empty())
                {
                    policy.read.insert(tool.source.clone());
                }
            } else if tool.execution.effect != ToolEffect::InternalState {
                policy.written.insert(tool.source.clone());
            }
        }
        policy
    }

    pub fn allows(&self, tool: &ToolDefinition) -> bool {
        let read_only = tool.execution.effect == ToolEffect::ReadOnly;
        let internal = tool.execution.effect == ToolEffect::InternalState;
        let shortcut_action = matches!(
            tool.name.as_str(),
            "shortcut.save" | "task.save" | "user.ask"
        );
        if self.shortcut && !read_only && !shortcut_action {
            return false;
        }
        if self.shortcut && shortcut_action {
            return true;
        }
        if self.shortcut && matches!(tool.name.as_str(), "shortcut.list" | "shortcut.get") {
            return true;
        }
        if self.mode == PermissionMode::PlanOnly && !read_only && !internal {
            return false;
        }
        if self.discovered.contains(&tool.name) {
            return true;
        }
        match tool.execution.exposure {
            ToolExposure::Always => true,
            ToolExposure::AfterRead => self.read.contains(&tool.source),
            ToolExposure::AfterWrite => self.written.contains(&tool.source),
            ToolExposure::OnDemand => false,
        }
    }

    pub fn select(&self, definitions: &[ToolDefinition]) -> Vec<ToolDefinition> {
        definitions
            .iter()
            .filter(|tool| self.allows(tool))
            .filter(|tool| {
                matches!(
                    tool.name.as_str(),
                    "tools.search" | "skills.search" | "skills.read" | "skills.reference"
                ) || self.discovered.contains(&tool.name)
                    || self.used.contains(&tool.name)
                    || matches!(
                        tool.execution.exposure,
                        ToolExposure::AfterRead | ToolExposure::AfterWrite
                    )
                    || (self.shortcut
                        && matches!(
                            tool.name.as_str(),
                            "shortcut.save" | "task.save" | "user.ask"
                        ))
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        extension::{FunctionTool, Tool, ToolExecution},
        model::ToolCall,
        runtime::context::message,
    };
    use serde_json::json;

    fn tool(
        name: &str,
        source: &str,
        exposure: ToolExposure,
        effect: ToolEffect,
    ) -> ToolDefinition {
        FunctionTool::new(name, "工具", json!({"type":"object"}), source, |_| {
            Ok(json!({}))
        })
        .with_execution(ToolExecution {
            effect,
            exposure,
            always_load: true,
            deferred: false,
            ..ToolExecution::default()
        })
        .definition()
    }

    fn observation(name: &str, value: Value) -> Vec<Message> {
        let mut call = message(Role::Assistant, "");
        call.tool_calls.push(ToolCall {
            id: "call".into(),
            name: name.into(),
            arguments: json!({}),
        });
        let mut result = message(Role::Tool, json!({"ok":true,"value":value}).to_string());
        result.tool_call_id = Some("call".into());
        vec![call, result]
    }

    #[test]
    fn evidence_unlocks_only_its_provider() {
        let tools = vec![
            tool(
                "a.read",
                "plugin:a",
                ToolExposure::Always,
                ToolEffect::ReadOnly,
            ),
            tool(
                "a.write",
                "plugin:a",
                ToolExposure::AfterRead,
                ToolEffect::ExternalWrite,
            ),
            tool(
                "b.write",
                "plugin:b",
                ToolExposure::AfterRead,
                ToolEffect::ExternalWrite,
            ),
        ];
        let policy = TurnTools::new(
            &tools,
            &observation("a.read", json!({"path":"item"})),
            &HashSet::new(),
            PermissionMode::AskEach,
            false,
        );
        assert!(policy.allows(&tools[1]));
        assert!(!policy.allows(&tools[2]));
    }

    #[test]
    fn empty_search_is_not_target_evidence() {
        let tools = vec![
            tool("a.read", "a", ToolExposure::Always, ToolEffect::ReadOnly),
            tool("a.write", "a", ToolExposure::AfterRead, ToolEffect::Write),
        ];
        let policy = TurnTools::new(
            &tools,
            &observation("a.read", json!({"total":0})),
            &HashSet::new(),
            PermissionMode::AskEach,
            false,
        );
        assert!(!policy.allows(&tools[1]));
    }

    #[test]
    fn discovery_cannot_override_read_only_permissions() {
        let write = tool("write", "a", ToolExposure::OnDemand, ToolEffect::Write);
        let policy = TurnTools::new(
            std::slice::from_ref(&write),
            &[],
            &HashSet::from(["write".into()]),
            PermissionMode::PlanOnly,
            false,
        );
        assert!(!policy.allows(&write));
    }

    #[test]
    fn successful_submission_unlocks_verification() {
        let tools = vec![
            tool("submit", "a", ToolExposure::AfterRead, ToolEffect::Write),
            tool(
                "status",
                "a",
                ToolExposure::AfterWrite,
                ToolEffect::ReadOnly,
            ),
        ];
        let policy = TurnTools::new(
            &tools,
            &observation("submit", json!({"jobId":"job"})),
            &HashSet::new(),
            PermissionMode::AskEach,
            false,
        );
        assert!(policy.allows(&tools[1]));
    }
}
