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
    /// 是否仍在推进。
    pub fn active(self) -> bool {
        !self.terminal()
    }
    /// 是否仍占用互斥资源。待核对可以停止推进，但在人工处置前不释放锁。
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
            Self::Blocked => matches!(next, Self::Queued | Self::Preflighting | Self::Deciding),
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
    /// 本轮是否已经丢掉或清空过较早上下文。
    #[serde(default)]
    pub context_compacted: bool,
    /// 模型服务报告的缓存读取量，仅用于核对底层协议行为。它不代表 Agent 已经
    /// 正确维持了可复用前缀。
    #[serde(default)]
    pub cache_read_tokens: u64,
    /// Agent 在发起本轮模型请求前确认可复用的稳定前缀 token 数。命中判断来自
    /// 本地保存的模型、系统提示、工具契约与消息前缀快照，不依赖厂商用量字段。
    #[serde(default)]
    pub prompt_cache_hit_tokens: u64,
    /// 最近一次模型请求是否复用了上一轮的完整稳定前缀。
    #[serde(default)]
    pub prompt_cache_hit: bool,
    /// 冷启动或失效原因，供事件、诊断和测试使用。
    #[serde(default)]
    pub prompt_cache_reason: Option<String>,
    /// 最近一次已经派发给模型的缓存关键快照。先持久化再发送，崩溃恢复后仍能
    /// 判断后续请求有没有改写既有前缀。
    #[serde(default)]
    pub prompt_cache_snapshot: Option<PromptCacheSnapshot>,
    #[serde(default)]
    pub message_boundary: i64,
    pub result: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub source: RunSource,
    /// 接纳时固定的模型配置。
    #[serde(default)]
    pub model_id: Option<String>,
    /// 本轮已经发现、允许直接调用的工具与桥接口，崩溃恢复后从这里还原。
    #[serde(default)]
    pub discovered: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptCacheSnapshot {
    pub model_key: String,
    pub system_key: String,
    pub tools_key: String,
    pub prefix_key: String,
    pub message_count: usize,
    pub tokens: u64,
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
    /// 与 `Run.discovered` 同步。
    #[serde(default)]
    pub discovered: Vec<String>,
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
    let mut value = serde_json::to_value(run).expect("serializable run");
    // 前缀哈希只用于恢复与审计，界面既不需要它，也不应把内部缓存键当产品状态。
    if let Some(object) = value.as_object_mut() {
        object.remove("promptCacheSnapshot");
    }
    value
}
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
pub fn hash(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    // serde_json 的 map 按 key 排序（preserve_order 未启用）。
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_runs_can_be_requeued_or_cancelled() {
        assert!(RunState::Blocked.permits(RunState::Queued));
        assert!(RunState::Blocked.permits(RunState::Cancelled));
    }

    #[test]
    fn public_run_does_not_expose_the_internal_prompt_cache_key() {
        let run = Run {
            id: "run".into(),
            conversation_id: "conversation".into(),
            prompt: "test".into(),
            state: RunState::Deciding,
            revision: 1,
            created_at: "now".into(),
            updated_at: "now".into(),
            deadline: 1,
            decisions: 1,
            tool_calls: 0,
            input_tokens: 10,
            output_tokens: 0,
            usage_estimated: true,
            context_tokens: 10,
            context_window: 100,
            context_compacted: false,
            cache_read_tokens: 0,
            prompt_cache_hit_tokens: 10,
            prompt_cache_hit: true,
            prompt_cache_reason: Some("hit".into()),
            prompt_cache_snapshot: Some(PromptCacheSnapshot {
                model_key: "secret-model-key".into(),
                ..PromptCacheSnapshot::default()
            }),
            message_boundary: 1,
            result: None,
            error: None,
            source: RunSource::Agent,
            model_id: Some("model".into()),
            discovered: vec![],
        };
        let public = public_run(&run);
        assert!(public.get("promptCacheSnapshot").is_none());
        assert_eq!(public["promptCacheHit"], true);
        assert_eq!(public["promptCacheHitTokens"], 10);
    }
}
