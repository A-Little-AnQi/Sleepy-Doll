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
    pub max_stagnant_calls: usize,
    pub memory_enabled: bool,
    pub duration_sec: i64,
    /// 上下文预算（字符）。留空则按当前模型的窗口推导。
    #[serde(default)]
    pub context_chars: Option<usize>,
    /// 单次请求的总窗口上限（输入与输出）。留空则用模型窗口。
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
            // 人为轮次/工具上限默认不限制（0）：防失控靠时限、停止按钮与急停热键。
            max_decisions: 0,
            max_tools: 0,
            max_replans: 2,
            max_stagnant_calls: 6,
            memory_enabled: true,
            duration_sec: 1800,
            // 留空：按当前模型的窗口推导，见 `budget`。
            context_chars: None,
            max_tokens: None,
            grants: vec![],
            // 默认每次写入都问一次。
            permission_mode: crate::runtime::operation::permissions::PermissionMode::AskEach,
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
/// 触发修剪前留的缓冲。
const THRESHOLD_BUFFER: u64 = 13_000;

pub fn output_reserve(model: &crate::config::ModelConfig) -> u64 {
    let window = model.options.context_window.max(1);
    model
        .options
        .max_output_tokens
        .unwrap_or_else(|| 8_192.min((window / 4).max(1)))
        .min(window.saturating_sub(512).max(1))
}

/// Auxiliary reasoning shares the user's output cap, not the whole main window.
pub fn auxiliary_output_reserve(model: &crate::config::ModelConfig) -> u64 {
    output_reserve(model).min(8192)
}

pub fn auxiliary_timeout(purpose: &str, remaining: i64) -> u64 {
    let maximum = match purpose {
        "compaction" => 180,
        _ => 120,
    };
    if purpose == "memory" {
        maximum as u64
    } else {
        remaining.clamp(1, maximum) as u64
    }
}

/// Observations correct an estimator bias once; repeated observations must not
/// compound the same ratio and shrink a usable window on every turn.
pub fn calibrated_scale(previous: usize, estimated: u64, observed: u64) -> usize {
    if estimated == 0 || observed <= estimated {
        return previous;
    }
    let correction = estimated
        .saturating_mul(90)
        .checked_div(observed)
        .unwrap_or(1)
        .clamp(1, 100) as usize;
    previous.min(correction)
}

fn window(policy: &RuntimeConfig, model: &crate::config::ModelConfig) -> u64 {
    let model_window = model.options.context_window.max(1);
    policy.max_tokens.unwrap_or(model_window).min(model_window)
}

/// 上下文预算：字符预算与 token 上限。
///
/// 按当前模型自己的窗口推导：窗口减去给输出留的余量，再留一段缓冲。显式配置了
/// `contextChars` / `maxTokens` 时以配置为准。
pub fn budget(policy: &RuntimeConfig, model: &crate::config::ModelConfig) -> (usize, u64) {
    let window = window(policy, model);
    let derived = input_token_budget(policy, model);
    (policy.context_chars.unwrap_or(derived as usize), window)
}

pub fn input_token_budget(policy: &RuntimeConfig, model: &crate::config::ModelConfig) -> u64 {
    let window = window(policy, model);
    let reserve = output_reserve(model);
    window
        .saturating_sub(reserve)
        .saturating_sub(THRESHOLD_BUFFER.min(window / 8))
}

impl RuntimeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.duration_sec < 1
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

#[cfg(test)]
mod tests {
    use super::*;

    fn model(window: u64) -> crate::config::ModelConfig {
        serde_json::from_value(
            serde_json::json!({"id":"m","name":"m","protocol":"openai-chat",
            "model":"m","baseUrl":"http://localhost","options":{"contextWindow":window}}),
        )
        .unwrap()
    }

    #[test]
    fn small_windows_have_room_for_input_and_output() {
        let model = model(8192);
        assert_eq!(output_reserve(&model), 2048);
        assert_eq!(input_token_budget(&RuntimeConfig::default(), &model), 5120);
    }

    #[test]
    fn configured_budget_cannot_expand_the_model_window() {
        let policy = RuntimeConfig {
            max_tokens: Some(100_000),
            ..RuntimeConfig::default()
        };
        assert_eq!(budget(&policy, &model(8192)).1, 8192);
    }

    #[test]
    fn explicit_small_windows_and_output_caps_are_not_expanded() {
        let policy = RuntimeConfig::default();
        let mut model = model(2048);
        assert_eq!(budget(&policy, &model).1, 2048);
        assert_eq!(output_reserve(&model), 512);
        assert_eq!(input_token_budget(&policy, &model), 1280);
        model.options.max_output_tokens = Some(100_000);
        assert_eq!(output_reserve(&model), 1536);
    }

    #[test]
    fn calibration_does_not_repeatedly_shrink_for_the_same_estimator_error() {
        let mut scale = 100;
        for _ in 0..12 {
            scale = calibrated_scale(scale, 1000, 2000);
            assert_eq!(scale, 45);
        }
        assert_eq!(calibrated_scale(20, 1000, 2000), 20);
        assert_eq!(calibrated_scale(45, 1000, 900), 45);
        let mut model = model(256000);
        model.options.max_output_tokens = Some(15000);
        assert_eq!(auxiliary_output_reserve(&model), 8192);
        model.options.max_output_tokens = Some(512);
        assert_eq!(auxiliary_output_reserve(&model), 512);
        assert_eq!(auxiliary_timeout("memory", 600), 120);
        assert_eq!(auxiliary_timeout("compaction", 600), 180);
        assert_eq!(auxiliary_timeout("compaction", -1), 1);
        assert_eq!(
            auxiliary_timeout("memory", -600),
            120,
            "Background learning must not inherit an ended task deadline"
        );
    }
}
