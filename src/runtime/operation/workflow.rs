use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::{Error, Result},
    extension::{ToolExecution, UnattendedPolicy},
};

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
        if !self.input_schema.is_null() {
            jsonschema::validator_for(&self.input_schema)
                .map_err(|_| Error::Config("workflow input schema is invalid".into()))?;
        }
        let mut seen = HashSet::new();
        for step in &self.steps {
            step.execution.validate()?;
            if step.id.trim().is_empty()
                || step.tool.trim().is_empty()
                || !step.arguments.is_object()
            {
                return Err(Error::Config("workflow step is incomplete".into()));
            }
            if !step
                .depends_on
                .iter()
                .all(|dependency| seen.contains(dependency))
                || !seen.insert(step.id.clone())
            {
                return Err(Error::Config(
                    "workflow dependencies must reference earlier unique steps".into(),
                ));
            }
            if self.unattended == UnattendedPolicy::Allowed
                && step.execution.unattended != UnattendedPolicy::Allowed
            {
                return Err(Error::Config(
                    "workflow contains a step that forbids unattended execution".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unattended_workflow_requires_every_step_to_opt_in() {
        let workflow = Workflow {
            id: "w".into(),
            revision: 1,
            name: "w".into(),
            description: "".into(),
            input_schema: Value::Null,
            steps: vec![WorkflowStep {
                id: "s".into(),
                title: "s".into(),
                tool: "tool".into(),
                arguments: serde_json::json!({}),
                depends_on: vec![],
                execution: ToolExecution::default(),
                provider_version: None,
                resource_versions: vec![],
            }],
            verified_from_run: "r".into(),
            verified_at: "now".into(),
            unattended: UnattendedPolicy::Allowed,
            created_at: "now".into(),
        };
        assert!(workflow.validate().is_err());
    }
}
