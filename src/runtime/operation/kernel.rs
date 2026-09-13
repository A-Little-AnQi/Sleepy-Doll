use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extension::{RiskLevel, ToolExecution};

pub type ResourceId = String;
pub type PluginId = String;
pub type ArtifactId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceDescriptor {
    pub id: ResourceId,
    pub provider_id: PluginId,
    pub kind: String,
    pub display_name: String,
    pub version: String,
    pub location: Value,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub presentation: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceSnapshot {
    pub id: String,
    pub resource_id: ResourceId,
    pub resource_version: String,
    pub content_artifact: ArtifactId,
    pub content_hash: String,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationState {
    Draft,
    Validating,
    AwaitingAuthorization,
    Preparing,
    Prepared,
    Committing,
    Verifying,
    Succeeded,
    Cancelling,
    Recovering,
    RollingBack,
    RolledBack,
    Failed,
    Cancelled,
    NeedsReview,
}

impl OperationState {
    pub fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::RolledBack | Self::Failed | Self::Cancelled
        )
    }

    pub fn permits(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        if self.terminal() {
            return false;
        }
        if next == Self::Cancelling {
            return true;
        }
        match self {
            Self::Draft => matches!(next, Self::Validating | Self::Cancelled),
            Self::Validating => matches!(
                next,
                Self::AwaitingAuthorization | Self::Preparing | Self::Failed
            ),
            Self::AwaitingAuthorization => {
                matches!(next, Self::Preparing | Self::Cancelled | Self::Failed)
            }
            Self::Preparing => matches!(next, Self::Prepared | Self::Failed | Self::NeedsReview),
            Self::Prepared => matches!(next, Self::Committing | Self::Cancelled | Self::Recovering),
            Self::Committing => matches!(
                next,
                Self::Verifying | Self::RollingBack | Self::Recovering | Self::NeedsReview
            ),
            Self::Verifying => matches!(
                next,
                Self::Succeeded | Self::RollingBack | Self::NeedsReview
            ),
            Self::RollingBack => matches!(next, Self::RolledBack | Self::NeedsReview),
            Self::Cancelling => {
                matches!(next, Self::Cancelled | Self::Recovering | Self::NeedsReview)
            }
            Self::Recovering => matches!(
                next,
                Self::Prepared | Self::Verifying | Self::RollingBack | Self::NeedsReview
            ),
            Self::NeedsReview => {
                matches!(next, Self::Recovering | Self::RollingBack | Self::Cancelled)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrecondition {
    pub resource_id: ResourceId,
    pub expected_version: String,
    pub expected_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum StagedOutput {
    ReplaceResource {
        resource_id: ResourceId,
        content_artifact: ArtifactId,
        expected_hash: String,
    },
    BrokeredAction {
        executor: String,
        request: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRequest {
    pub provider_id: PluginId,
    pub method: String,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompensationPlan {
    #[serde(default)]
    pub restore_snapshots: Vec<String>,
    #[serde(default)]
    pub provider_request: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationPlan {
    pub id: String,
    pub provider_id: PluginId,
    #[serde(default)]
    pub resources: Vec<ResourcePrecondition>,
    #[serde(default)]
    pub staged_outputs: Vec<StagedOutput>,
    pub execution: ToolExecution,
    pub verification: VerificationRequest,
    pub compensation: CompensationPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StepEvidence {
    pub step_id: String,
    pub outcome: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationCheckpoint {
    pub operation_id: String,
    pub state: OperationState,
    #[serde(default)]
    pub completed_steps: Vec<StepEvidence>,
    #[serde(default)]
    pub pending_step: Option<String>,
    #[serde(default)]
    pub held_leases: Vec<String>,
    #[serde(default)]
    pub unresolved_attempts: Vec<String>,
    #[serde(default)]
    pub pending_approval: Option<String>,
    pub revision: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub run_id: Option<String>,
    pub provider_id: PluginId,
    pub title: String,
    pub state: OperationState,
    pub revision: u64,
    pub risk: RiskLevel,
    pub plan: MutationPlan,
    pub checkpoint: OperationCheckpoint,
    pub created_at: String,
    pub updated_at: String,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum IssueKind {
    UserConfiguration,
    ThirdPartyScript,
    HostApplication,
    Environment,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticFinding {
    pub id: String,
    pub provider_id: PluginId,
    pub kind: IssueKind,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub repair_action: Option<Value>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferenceRecord {
    pub key: String,
    pub value: Value,
    pub source: String,
    pub explicitly_provided: bool,
    pub scope: String,
    pub created_at: String,
    pub confirmed_at: String,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricEvent {
    pub name: String,
    pub value: f64,
    pub unit: String,
    #[serde(default)]
    pub labels: Value,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRecord {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub message: String,
    #[serde(default)]
    pub target: Option<Value>,
    pub created_at: String,
    pub read_at: Option<String>,
}
