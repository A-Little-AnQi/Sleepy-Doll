//! Runtime-owned provenance; provider roles and tool data cannot revise goals.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InputOrigin {
    HumanMessage,
    HumanSupplement,
    HumanAnswer { request_id: String },
    ShortcutClick { task_id: String, revision: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalInput {
    pub revision: u64,
    pub origin: InputOrigin,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub revision: u64,
    pub inputs: Vec<GoalInput>,
}

impl Goal {
    pub fn new(text: &str) -> Self {
        Self {
            revision: 1,
            inputs: vec![GoalInput {
                revision: 1,
                origin: InputOrigin::HumanMessage,
                text: text.into(),
            }],
        }
    }
    pub fn revise(&mut self, origin: InputOrigin, text: &str) {
        self.revision += 1;
        self.inputs.push(GoalInput {
            revision: self.revision,
            origin,
            text: text.into(),
        });
    }
    pub fn text(&self) -> String {
        self.inputs
            .iter()
            .map(|input| input.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Transport success is an observation, not business verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptState {
    Observed,
    Accepted,
    Verified,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReceipt {
    pub id: String,
    pub run_id: String,
    pub goal_revision: u64,
    pub call_id: String,
    pub tool: String,
    pub arguments_hash: String,
    pub effect: crate::extension::ToolEffect,
    pub state: ReceiptState,
    pub observed_at: String,
    pub evidence: Value,
}

pub fn receipt_state(value: &Value, effect: crate::extension::ToolEffect) -> ReceiptState {
    if value["ok"] != true {
        return ReceiptState::Failed;
    }
    let body = &value["value"];
    if matches!(
        body["outcome"].as_str(),
        Some("verifiedFailed" | "failed" | "cancelled")
    ) || body
        .pointer("/verification/status")
        .is_some_and(|s| s == "failed")
    {
        return ReceiptState::Failed;
    }
    if matches!(
        body["outcome"].as_str(),
        Some("unknown" | "submitting" | "running")
    ) {
        return ReceiptState::Unknown;
    }
    if crate::runtime::operation::executor::launch_submitted(body) {
        return ReceiptState::Accepted;
    }
    if body["outcome"] == "verifiedSucceeded"
        || body
            .pointer("/verification/status")
            .is_some_and(|s| s == "succeeded")
    {
        return ReceiptState::Verified;
    }
    if effect != crate::extension::ToolEffect::ReadOnly {
        return ReceiptState::Unknown;
    }
    ReceiptState::Observed
}

pub fn progress_value(value: &Value) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        "observedAt" | "elapsedMs" | "snapshotId" | "callId" | "requestId"
                    )
                })
                .map(|(key, value)| (key.clone(), progress_value(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(progress_value).collect()),
        other => other.clone(),
    }
}
