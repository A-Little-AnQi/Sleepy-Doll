use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunState {
    Answered,
    Queued,
    Preflighting,
    Deciding,
    AwaitingUser,
    AwaitingApproval,
    Executing,
    WaitingJob,
    Verifying,
    Recovering,
    Cancelling,
    Blocked,
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
    /// 是否仍在推进。`needsReview` 不主动执行，但也不是终态的对账结果。
    pub fn active(self) -> bool {
        !self.terminal()
    }
    /// 是否仍占用互斥资源。待核对可以停止推进，但在人工处置前不释放锁，
    /// 否则通用「非运行即放行」逻辑会让另一个运行重复写入未知的外部效果。
    pub fn holds_lease(self) -> bool {
        !matches!(
            self,
            Self::Answered
                | Self::Succeeded
                | Self::Partial
                | Self::Failed
                | Self::Cancelled
                | Self::Blocked
        )
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "排队中",
            Self::Preflighting => "检查运行条件",
            Self::Deciding => "正在处理请求",
            Self::AwaitingUser => "等待你补充信息",
            Self::AwaitingApproval => "等待你确认更改",
            Self::Executing => "正在执行",
            Self::WaitingJob => "等待工具完成",
            Self::Verifying => "正在核对结果",
            Self::Recovering => "正在恢复运行状态",
            Self::Cancelling => "正在请求停止",
            Self::Blocked => "暂时无法运行",
            Self::Answered => "已回答",
            Self::Succeeded => "已完成并核对",
            Self::Partial => "部分完成",
            Self::Failed => "执行失败",
            Self::NeedsReview => "结果待核对",
            Self::Cancelled => "已停止",
        }
    }
    pub fn permits(self, next: Self) -> bool {
        if self.terminal() {
            return false;
        }
        if next == Self::Cancelling || next == Self::Recovering || next.terminal() {
            return true;
        }
        match self {
            Self::Queued => matches!(next, Self::Preflighting | Self::Deciding | Self::Blocked),
            // 预检失败且未提交任何外部写入时才进 blocked。
            Self::Preflighting => matches!(
                next,
                Self::Deciding | Self::Executing | Self::AwaitingApproval | Self::Blocked
            ),
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
            Self::AwaitingApproval => matches!(next, Self::Executing | Self::Blocked),
            Self::AwaitingUser => matches!(next, Self::Deciding | Self::Executing),
            Self::WaitingJob => matches!(next, Self::Verifying | Self::Recovering),
            Self::Verifying => matches!(next, Self::Deciding | Self::Executing),
            Self::Recovering => matches!(
                next,
                Self::WaitingJob | Self::Verifying | Self::Deciding | Self::Blocked
            ),
            // blocked 修好后由用户重新点击运行，不自动推进。
            Self::Blocked => matches!(next, Self::Preflighting | Self::Deciding),
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
    /// 最近一次实际发给模型的上下文占用（token，含系统提示与已压缩后的历史）。
    #[serde(default)]
    pub context_tokens: u64,
    /// 该次请求所用的窗口上限。
    #[serde(default)]
    pub context_window: u64,
    /// 本轮是否已经丢掉或清空过较早上下文。界面只展示这一事实，不再展开细节。
    #[serde(default)]
    pub context_compacted: bool,
    #[serde(default)]
    pub message_boundary: i64,
    pub result: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub source: RunSource,
    /// 接纳时固定的模型配置。运行途中改选择只影响下一次决策请求，已经发出的
    /// 请求保持原协议与配置。
    #[serde(default)]
    pub model_id: Option<String>,
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
