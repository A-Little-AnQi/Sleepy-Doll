//! 旧版「已验证运行提取」的存储结构。新写入一律走 [`super::task`] 的修订模型；
//! 这里只保留读取旧行并把它们转换成修订的能力，避免两套定义长期并存。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::{Error, Result},
    extension::{ToolExecution, UnattendedPolicy},
};

use super::task::{FailurePolicy, ModelUsage, SequenceNode, TaskNode, ToolNode, WorkflowRevision};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: String,
    pub title: String,
    pub tool: String,
    pub arguments: Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub execution: ToolExecution,
    #[serde(default)]
    pub provider_version: Option<String>,
    #[serde(default)]
    pub resource_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Workflow {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
    pub steps: Vec<WorkflowStep>,
    pub verified_from_run: String,
    pub verified_at: String,
    pub unattended: UnattendedPolicy,
    pub created_at: String,
}

impl Workflow {
    /// 旧结构的依赖约束强于新结构，转换后仍是一条顺序链，语义不变。
    pub fn to_revision(&self) -> Option<WorkflowRevision> {
        if self.steps.is_empty() {
            return None;
        }
        let mut ordered = Vec::new();
        let mut placed = std::collections::HashSet::new();
        let mut remaining = self.steps.clone();
        while !remaining.is_empty() {
            let mut progressed = false;
            let mut next = Vec::new();
            for step in remaining {
                if step.depends_on.iter().all(|id| placed.contains(id)) {
                    placed.insert(step.id.clone());
                    ordered.push(step);
                    progressed = true;
                } else {
                    next.push(step);
                }
            }
            if !progressed {
                return None;
            }
            remaining = next;
        }
        let nodes = ordered
            .into_iter()
            .map(|step| {
                TaskNode::Tool(ToolNode {
                    id: step.id,
                    title: step.title,
                    tool: Some(step.tool),
                    capability_id: None,
                    arguments: step.arguments,
                    execution: Some(step.execution),
                    provider_version: step.provider_version,
                    resource_versions: step.resource_versions,
                    on_failure: FailurePolicy::Stop,
                    on_unverified: FailurePolicy::Stop,
                })
            })
            .collect::<Vec<_>>();
        Some(WorkflowRevision {
            task_id: self.id.clone(),
            revision: self.revision,
            schema_version: super::task::SCHEMA_VERSION,
            name: self.name.clone(),
            description: self.description.clone(),
            nodes: vec![TaskNode::Sequence(SequenceNode {
                id: "steps".into(),
                title: self.name.clone(),
                nodes,
            })],
            limits: Default::default(),
            model_usage: ModelUsage::None,
            validation: Default::default(),
            created_at: self.created_at.clone(),
        })
    }
}

impl Workflow {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty()
            || self.name.trim().is_empty()
            || self.revision == 0
            || self.steps.is_empty()
        {
            return Err(Error::Config(
                "workflow id, name, revision and steps are required".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(id: &str, depends_on: &[&str]) -> WorkflowStep {
        WorkflowStep {
            id: id.into(),
            title: id.into(),
            tool: "tool".into(),
            arguments: json!({}),
            depends_on: depends_on.iter().map(|id| (*id).to_owned()).collect(),
            execution: ToolExecution::read_only(),
            provider_version: None,
            resource_versions: vec![],
        }
    }

    #[test]
    fn legacy_workflow_converts_into_an_ordered_sequence() {
        let workflow = Workflow {
            id: "w".into(),
            revision: 3,
            name: "w".into(),
            description: "".into(),
            input_schema: Value::Null,
            steps: vec![
                step("a", &[]),
                step("c", &["a"]),
                step("b", &[]),
                step("d", &["c"]),
            ],
            verified_from_run: "r".into(),
            verified_at: "now".into(),
            unattended: UnattendedPolicy::Forbidden,
            created_at: "now".into(),
        };
        let revision = workflow.to_revision().unwrap();
        assert_eq!(revision.revision, 3);
        let TaskNode::Sequence(sequence) = &revision.nodes[0] else {
            panic!("转换结果应是顺序节点");
        };
        let order = sequence
            .nodes
            .iter()
            .map(|node| node.id().to_owned())
            .collect::<Vec<_>>();
        let position = |id: &str| order.iter().position(|entry| entry == id).unwrap();
        assert!(position("a") < position("c"));
        assert!(position("c") < position("d"));
    }

    #[test]
    fn cyclic_legacy_workflow_refuses_to_convert() {
        let workflow = Workflow {
            id: "w".into(),
            revision: 1,
            name: "w".into(),
            description: "".into(),
            input_schema: Value::Null,
            steps: vec![step("a", &["b"]), step("b", &["a"])],
            verified_from_run: "r".into(),
            verified_at: "now".into(),
            unattended: UnattendedPolicy::Forbidden,
            created_at: "now".into(),
        };
        assert!(workflow.to_revision().is_none());
    }
}
