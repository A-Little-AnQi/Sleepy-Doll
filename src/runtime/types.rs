use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Answered,
    Queued,
    Deciding,
    AwaitingUser,
    AwaitingApproval,
    Executing,
    WaitingJob,
    Verifying,
    Recovering,
    Cancelling,
    Succeeded,
    Partial,
    Failed,
    Cancelled,
    NeedsReview,
}
impl RunState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Answered
                | Self::Succeeded
                | Self::Partial
                | Self::Failed
                | Self::Cancelled
                | Self::NeedsReview
        )
    }
    pub fn permits(self, next: Self) -> bool {
        if self.terminal() {
            return false;
        }
        if next == Self::Cancelling || next == Self::Recovering || next.terminal() {
            return true;
        }
        match self {
            Self::Queued => matches!(next, Self::Deciding | Self::Recovering),
            Self::Deciding => {
                matches!(next, Self::Executing | Self::AwaitingUser | Self::Verifying)
            }
            Self::Executing => matches!(
                next,
                Self::AwaitingApproval
                    | Self::AwaitingUser
                    | Self::WaitingJob
                    | Self::Deciding
                    | Self::Verifying
            ),
            Self::AwaitingApproval => matches!(next, Self::Executing),
            Self::AwaitingUser => matches!(next, Self::Deciding | Self::Executing),
            Self::WaitingJob => matches!(next, Self::Verifying | Self::Recovering),
            Self::Verifying => matches!(next, Self::Deciding | Self::Executing),
            Self::Recovering => matches!(next, Self::WaitingJob | Self::Verifying | Self::Deciding),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub conversation_id: String,
    pub prompt: String,
    pub state: RunState,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
    pub deadline: i64,
    pub decisions: usize,
    pub tool_calls: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub usage_estimated: bool,
    #[serde(default)]
    pub message_boundary: i64,
    pub result: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub source: RunSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RunSource {
    #[default]
    Agent,
    SavedStrategy {
        #[serde(alias = "strategy_id")]
        strategy_id: String,
    },
    SavedWorkflow {
        #[serde(alias = "workflow_id")]
        workflow_id: String,
        #[serde(alias = "workflow_revision")]
        workflow_revision: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub sequence: u64,
    pub conversation_id: String,
    pub run_id: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCheckpoint {
    pub run_id: String,
    pub goal: String,
    pub run_state: RunState,
    pub run_revision: u64,
    pub plan_revision: Option<u64>,
    #[serde(default)]
    pub completed_steps: Vec<String>,
    pub pending_step: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub unresolved_attempts: Vec<String>,
    pub pending_approval: Option<String>,
    #[serde(default)]
    pub selected_resources: Vec<Value>,
    #[serde(default)]
    pub bridge_instances: Vec<String>,
    #[serde(default)]
    pub catalog_versions: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attempt {
    pub id: String,
    pub run_id: String,
    pub call_id: String,
    pub request: Value,
    pub request_hash: String,
    pub instance_id: String,
    pub job_id: Option<String>,
    pub outcome: String,
    pub evidence: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub capability_id: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub execution: Option<crate::extension::ToolExecution>,
    #[serde(default)]
    pub provider_version: Option<String>,
    #[serde(default)]
    pub resource_versions: Vec<String>,
    pub arguments: Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRevision {
    pub revision: u64,
    pub goal: String,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedStrategy {
    pub id: String,
    pub name: String,
    pub source_run_id: String,
    pub plan: PlanRevision,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: String,
    pub run_id: String,
    pub request_hash: String,
    pub request: Value,
    pub expires_at: i64,
    pub decision: Option<bool>,
}

pub fn unix_now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
pub fn public_run(run: &Run) -> Value {
    serde_json::to_value(run).expect("serializable run")
}
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
pub fn hash(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    // serde_json maps are sorted (preserve_order is deliberately disabled).
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
