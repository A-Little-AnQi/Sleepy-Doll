use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct RuntimeConfig {
    pub catalog_directory: std::path::PathBuf,
    pub max_decisions: usize,
    pub max_tools: usize,
    pub max_replans: u64,
    pub duration_sec: i64,
    /// 上下文预算（字符）。留空则按当前模型的窗口推导。
    #[serde(default)]
    pub context_chars: Option<usize>,
    /// 单次请求的输入 token 上限。留空则用模型窗口。
    #[serde(default)]
    pub max_tokens: Option<u64>,
    pub grants: Vec<Grant>,
    pub permission_mode: crate::runtime::operation::permissions::PermissionMode,
    pub trust_grants: Vec<crate::runtime::operation::permissions::TrustGrant>,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            catalog_directory: ".".into(),
            // 长任务（诊断→修正→执行→收尾）实测 32 轮不够：修分辨率再跑采集就撞线。
            max_decisions: 64,
            max_tools: 128,
            max_replans: 2,
            duration_sec: 1800,
            // 留空：按当前模型的窗口推导，见 `budget`。
            context_chars: None,
            max_tokens: None,
            grants: vec![],
            // 默认按实际影响判定：普通写入直接执行，只有删除与大范围变更确认一次。
            permission_mode: crate::runtime::operation::permissions::PermissionMode::Standard,
            trust_grants: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub capability_id: String,
    pub instance_id: String,
    pub catalog_version: String,
    pub arguments: Value,
    #[serde(default)]
    pub resource_binding_hash: String,
    pub expires_at: i64,
}
/// 给输出留的余量上限。
const MAX_OUTPUT_RESERVE: u64 = 20_000;

/// 触发修剪前留的缓冲。
const THRESHOLD_BUFFER: u64 = 13_000;

/// 上下文预算：字符预算与 token 上限。
///
/// 按当前模型自己的窗口推导：窗口减去给输出留的余量，再留一段缓冲。显式配置了
/// `contextChars` / `maxTokens` 时以配置为准。
pub fn budget(policy: &RuntimeConfig, model: &crate::config::ModelConfig) -> (usize, u64) {
    let window = model.options.context_window.max(8_192);
    let reserve = model
        .options
        .max_output_tokens
        .unwrap_or(8_192)
        .min(MAX_OUTPUT_RESERVE);
    let derived = window
        .saturating_sub(reserve)
        .saturating_sub(THRESHOLD_BUFFER);
    (
        policy.context_chars.unwrap_or(derived as usize),
        policy.max_tokens.unwrap_or(window),
    )
}

impl RuntimeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_decisions == 0
            || self.max_decisions > 128
            || self.max_tools == 0
            || self.max_tools > 1024
            || self.duration_sec < 1
            || self.duration_sec > 86400
            || self.context_chars.is_some_and(|chars| chars < 4096)
            || self.max_tokens.is_some_and(|tokens| tokens < 1024)
        {
            return Err(Error::Config("invalid runtime budget".into()));
        }
        for g in &self.grants {
            if !g.arguments.is_object()
                || g.capability_id.is_empty()
                || g.instance_id.is_empty()
                || g.catalog_version.is_empty()
            {
                return Err(Error::Config(
                    "grant must bind an instance, capability, version and exact arguments".into(),
                ));
            }
        }
        for grant in &self.trust_grants {
            crate::runtime::operation::permissions::PermissionEngine::validate_grant(grant)?;
        }
        Ok(())
    }
    pub fn allows(&self, request: &Value) -> bool {
        self.grants.iter().any(|g| {
            g.expires_at > crate::runtime::types::unix_now()
                && request.get("capabilityId").unwrap_or(&request["methodId"]) == &g.capability_id
                && request["instanceId"] == g.instance_id
                && request["catalogVersion"] == g.catalog_version
                && request["arguments"] == g.arguments
                && crate::runtime::types::hash(&request["resources"]) == g.resource_binding_hash
        })
    }
}
