use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    extension::{RiskLevel, ToolEffect, UnattendedPolicy},
};

/// 审批级别：运行时据此决定写入是否需要询问。默认 `Standard`。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// 只读：任何写入都拒绝。
    PlanOnly,
    /// 每次写入都问。
    AskEach,
    /// 按实际影响判定；删除与大范围变更需要确认。
    #[default]
    Standard,
    /// 完全控制：一律直接执行，不再询问。
    FullAccess,
    /// 旧值：按资源授权范围放行。
    TrustedScopes,
}

impl PermissionMode {
    /// 面向用户的级别清单。
    pub fn levels() -> &'static [(PermissionMode, &'static str, &'static str)] {
        &[
            (
                PermissionMode::PlanOnly,
                "只读",
                "只查询和阅读，任何修改都不执行",
            ),
            (PermissionMode::AskEach, "请求审批", "每次修改前都问你一次"),
            (
                PermissionMode::Standard,
                "替我审批",
                "普通修改直接执行；删除文件和大幅改配置才问你一次",
            ),
            (
                PermissionMode::FullAccess,
                "完全控制",
                "一律直接执行，不再询问。删除和大范围覆盖也直接做",
            ),
        ]
    }
    pub fn label(self) -> &'static str {
        Self::levels()
            .iter()
            .find(|(mode, _, _)| *mode == self)
            .map(|(_, label, _)| *label)
            .unwrap_or("替我审批")
    }
    pub fn description(self) -> &'static str {
        Self::levels()
            .iter()
            .find(|(mode, _, _)| *mode == self)
            .map(|(_, _, description)| *description)
            .unwrap_or("")
    }
}

/// 大范围判定的阈值。
pub const LARGE_SCOPE_FIELDS: usize = 10;
pub const LARGE_SCOPE_OBJECTS: usize = 3;

/// 一次意图内的改动规模。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChangeScope {
    /// 受影响的配置叶字段数量。
    pub fields: usize,
    /// 受影响的对象数量（配置组、脚本等）。
    pub objects: usize,
    /// 删除用户文件或目录。
    pub deletes: bool,
    /// 整份替换或重置已有配置。
    pub replaces_whole: bool,
    /// 创建全新对象。
    pub creates: bool,
}

impl ChangeScope {
    pub fn fields(fields: usize) -> Self {
        Self {
            fields,
            ..Self::default()
        }
    }
    /// 新建一个全新对象。
    pub fn create(objects: usize) -> Self {
        Self {
            creates: true,
            objects,
            ..Self::default()
        }
    }
    pub fn delete() -> Self {
        Self {
            deletes: true,
            objects: 1,
            ..Self::default()
        }
    }
    pub fn whole() -> Self {
        Self {
            replaces_whole: true,
            objects: 1,
            ..Self::default()
        }
    }
    /// 按目标资源的当前内容与写入内容算出实际影响。
    ///
    /// 两边不是同一种可解析结构时返回 `None`。
    pub fn from_contents(previous: Option<&str>, next: &str) -> Option<Self> {
        let Some(previous) = previous else {
            return Some(Self::create(1));
        };
        let (Ok(before), Ok(after)) = (
            serde_json::from_str::<serde_json::Value>(previous),
            serde_json::from_str::<serde_json::Value>(next),
        ) else {
            return None;
        };
        Some(Self {
            fields: diff_leaf_fields(&before, &after),
            objects: 1,
            ..Self::default()
        })
    }
    pub fn accumulate(&mut self, other: Self) {
        // 累计字段、对象与各项标记。
        self.fields += other.fields;
        self.objects += other.objects;
        self.deletes |= other.deletes;
        self.replaces_whole |= other.replaces_whole;
        self.creates &= other.creates;
    }
    /// 是否达到需要确认的门限。
    pub fn is_large(&self) -> bool {
        if self.deletes || self.replaces_whole {
            return true;
        }
        if self.creates && self.objects <= 1 {
            return false;
        }
        self.fields >= LARGE_SCOPE_FIELDS || self.objects >= LARGE_SCOPE_OBJECTS
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrustGrant {
    pub id: String,
    pub provider_id: String,
    #[serde(default)]
    pub resource_ids: Vec<String>,
    #[serde(default)]
    pub resource_kinds: Vec<String>,
    #[serde(default)]
    pub effects: Vec<ToolEffect>,
    pub maximum_risk: RiskLevel,
    pub expires_at: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct PermissionRequest<'a> {
    pub provider_id: &'a str,
    pub resource_ids: &'a [String],
    pub resource_kinds: &'a [String],
    pub effect: ToolEffect,
    pub risk: RiskLevel,
    pub unattended: UnattendedPolicy,
    /// 本次请求的实际影响。缺省表示「无法界定」。
    pub scope: Option<ChangeScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    Ask,
    Deny,
}

pub struct PermissionEngine;

impl PermissionEngine {
    pub fn decide(
        mode: PermissionMode,
        request: &PermissionRequest<'_>,
        grants: &[TrustGrant],
    ) -> PermissionDecision {
        // 完全控制：一律放行。
        if mode == PermissionMode::FullAccess {
            return PermissionDecision::Allow;
        }
        if request.effect == ToolEffect::ReadOnly {
            return PermissionDecision::Allow;
        }
        if mode == PermissionMode::PlanOnly {
            return PermissionDecision::Deny;
        }
        // 无人值守不能覆盖需要用户确认的动作。
        if request.unattended == UnattendedPolicy::Forbidden
            && request.scope.is_some_and(|scope| scope.is_large())
        {
            return PermissionDecision::Ask;
        }
        if request.risk == RiskLevel::Irreversible {
            return PermissionDecision::Ask;
        }
        if mode == PermissionMode::AskEach {
            return PermissionDecision::Ask;
        }
        // 标准模式：普通写入直接执行。
        if mode == PermissionMode::Standard {
            // 未声明效果的工具需要一次确认。
            if request.effect == ToolEffect::Unknown {
                return PermissionDecision::Ask;
            }
            return match request.scope {
                Some(scope) if scope.is_large() => PermissionDecision::Ask,
                _ => PermissionDecision::Allow,
            };
        }
        if request.resource_ids.is_empty() && request.resource_kinds.is_empty() {
            return PermissionDecision::Ask;
        }
        let now = crate::runtime::types::unix_now();
        if grants.iter().any(|grant| {
            grant.provider_id == request.provider_id
                && grant.expires_at.is_none_or(|expires| expires > now)
                && risk_rank(request.risk) <= risk_rank(grant.maximum_risk)
                && (grant.effects.is_empty() || grant.effects.contains(&request.effect))
                && request
                    .resource_ids
                    .iter()
                    .all(|id| grant.resource_ids.contains(id))
                && request
                    .resource_kinds
                    .iter()
                    .all(|kind| grant.resource_kinds.contains(kind))
        }) {
            PermissionDecision::Allow
        } else {
            PermissionDecision::Ask
        }
    }

    pub fn validate_grant(grant: &TrustGrant) -> Result<()> {
        if grant.id.trim().is_empty() || grant.provider_id.trim().is_empty() {
            return Err(Error::Config("授权必须绑定 Provider 和稳定 ID".into()));
        }
        if grant.resource_ids.is_empty() && grant.resource_kinds.is_empty() {
            return Err(Error::Config("授权必须限定资源或资源类别".into()));
        }
        if grant.maximum_risk == RiskLevel::Irreversible {
            return Err(Error::Config("持续授权不能覆盖不可逆操作".into()));
        }
        Ok(())
    }
}

/// 值不同的叶字段数；新增与删除的字段都算一处改动。
pub fn diff_leaf_fields(before: &serde_json::Value, after: &serde_json::Value) -> usize {
    match (before, after) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let keys = left
                .keys()
                .chain(right.keys())
                .collect::<std::collections::HashSet<_>>();
            keys.into_iter()
                .map(|key| match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => diff_leaf_fields(left, right),
                    (Some(value), None) | (None, Some(value)) => count_leaves(value),
                    (None, None) => 0,
                })
                .sum()
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            let shared = left.len().min(right.len());
            let paired = (0..shared)
                .map(|index| diff_leaf_fields(&left[index], &right[index]))
                .sum::<usize>();
            let extra = left[shared..]
                .iter()
                .chain(right[shared..].iter())
                .map(count_leaves)
                .sum::<usize>();
            paired + extra
        }
        (left, right) if left == right => 0,
        (left, right) => count_leaves(left).max(count_leaves(right)),
    }
}

fn count_leaves(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map.values().map(count_leaves).sum::<usize>().max(1),
        serde_json::Value::Array(items) => items.iter().map(count_leaves).sum::<usize>().max(1),
        _ => 1,
    }
}

fn risk_rank(risk: RiskLevel) -> u8 {
    match risk {
        RiskLevel::Observe => 0,
        RiskLevel::Low => 1,
        RiskLevel::Standard => 2,
        RiskLevel::High => 3,
        RiskLevel::Irreversible => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(scope: ChangeScope) -> PermissionRequest<'a> {
        PermissionRequest {
            provider_id: "p",
            resource_ids: &[],
            resource_kinds: &[],
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::Standard,
            unattended: UnattendedPolicy::Allowed,
            scope: Some(scope),
        }
    }

    #[test]
    fn grants_are_resource_scoped_and_never_cover_irreversible_work() {
        let grant = TrustGrant {
            id: "g".into(),
            provider_id: "p".into(),
            resource_ids: vec!["r".into()],
            resource_kinds: vec![],
            effects: vec![ToolEffect::LocalWrite],
            maximum_risk: RiskLevel::Standard,
            expires_at: None,
            created_at: "now".into(),
        };
        let scoped = PermissionRequest {
            provider_id: "p",
            resource_ids: &["r".into()],
            resource_kinds: &[],
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::Standard,
            unattended: UnattendedPolicy::Forbidden,
            scope: Some(ChangeScope::fields(1)),
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::TrustedScopes, &scoped, &[grant]),
            PermissionDecision::Allow
        );
        let irreversible = PermissionRequest {
            risk: RiskLevel::Irreversible,
            ..scoped
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::TrustedScopes, &irreversible, &[]),
            PermissionDecision::Ask
        );
    }

    /// 普通写入直接执行。
    #[test]
    fn ordinary_writes_run_without_a_prompt() {
        assert_eq!(
            PermissionEngine::decide(
                PermissionMode::Standard,
                &request(ChangeScope::fields(1)),
                &[]
            ),
            PermissionDecision::Allow
        );
        assert_eq!(
            PermissionEngine::decide(
                PermissionMode::Standard,
                &request(ChangeScope::create(1)),
                &[]
            ),
            PermissionDecision::Allow
        );
    }

    /// 门限：9 个叶字段直接执行，10 个开始需要一次确认。
    #[test]
    fn large_scope_thresholds_match_the_product_rule() {
        assert!(!ChangeScope::fields(9).is_large());
        assert!(ChangeScope::fields(10).is_large());
        let two = ChangeScope {
            fields: 2,
            objects: 2,
            ..ChangeScope::default()
        };
        assert!(!two.is_large());
        let three = ChangeScope {
            fields: 3,
            objects: 3,
            ..ChangeScope::default()
        };
        assert!(three.is_large());
    }

    /// 新建一个带很多默认字段的对象不算批量改配置。
    #[test]
    fn creating_one_object_is_never_a_bulk_edit() {
        let created = ChangeScope {
            creates: true,
            fields: 20,
            objects: 1,
            ..ChangeScope::default()
        };
        assert!(!created.is_large());
    }

    /// 整份替换只看动作，不看字段数。
    #[test]
    fn whole_replacement_always_asks() {
        let replace = ChangeScope {
            fields: 2,
            objects: 1,
            replaces_whole: true,
            ..ChangeScope::default()
        };
        assert!(replace.is_large());
        assert_eq!(
            PermissionEngine::decide(PermissionMode::Standard, &request(replace), &[]),
            PermissionDecision::Ask
        );
    }

    /// 删除用户内容永远需要确认，且不能被无人值守覆盖。
    #[test]
    fn deletions_always_ask() {
        let delete = ChangeScope::delete();
        let mut unattended = request(delete);
        unattended.unattended = UnattendedPolicy::Forbidden;
        assert_eq!(
            PermissionEngine::decide(PermissionMode::Standard, &unattended, &[]),
            PermissionDecision::Ask
        );
    }

    /// 拆成多次调用不能绕过累计门限。
    #[test]
    fn splitting_an_intent_does_not_evade_the_threshold() {
        let mut total = ChangeScope::default();
        for _ in 0..10 {
            total.accumulate(ChangeScope::fields(1));
        }
        assert!(total.is_large());
        assert_eq!(
            PermissionEngine::decide(PermissionMode::Standard, &request(total), &[]),
            PermissionDecision::Ask
        );
    }

    /// 影响无法界定时仍按标准模式放行。
    #[test]
    fn undetermined_scope_does_not_blanket_ask() {
        let mut unknown = request(ChangeScope::default());
        unknown.scope = None;
        assert_eq!(
            PermissionEngine::decide(PermissionMode::Standard, &unknown, &[]),
            PermissionDecision::Allow
        );
        // 只读规划模式仍然拒绝一切写入。
        assert_eq!(
            PermissionEngine::decide(PermissionMode::PlanOnly, &unknown, &[]),
            PermissionDecision::Deny
        );
    }

    /// 未声明效果时询问一次。
    #[test]
    fn undeclared_effect_still_asks_once() {
        let undeclared = PermissionRequest {
            effect: ToolEffect::Unknown,
            ..request(ChangeScope::fields(1))
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::Standard, &undeclared, &[]),
            PermissionDecision::Ask
        );
        // 声明了效果的写入直接执行。
        assert_eq!(
            PermissionEngine::decide(
                PermissionMode::Standard,
                &request(ChangeScope::fields(1)),
                &[]
            ),
            PermissionDecision::Allow
        );
    }

    /// 写整份文件不等于改了整份配置。
    #[test]
    fn scope_counts_the_real_diff_not_the_write_size() {
        let before = r#"{"name":"日常","index":0,"projects":[{"a":1},{"b":2}]}"#;
        let after = r#"{"name":"日常","index":1,"projects":[{"a":1},{"b":2}]}"#;
        let scope = ChangeScope::from_contents(Some(before), after).unwrap();
        assert_eq!(scope.fields, 1);
        assert!(!scope.is_large());

        let removed = r#"{"name":"日常","projects":[{"a":1},{"b":2}]}"#;
        assert_eq!(
            ChangeScope::from_contents(Some(before), removed)
                .unwrap()
                .fields,
            1
        );
        let created = ChangeScope::from_contents(None, after).unwrap();
        assert!(created.creates);
        assert!(!created.is_large());
        // 两边不是同一类结构时返回 None。
        assert!(ChangeScope::from_contents(Some("plain text"), after).is_none());
    }

    /// 完全控制：删除与大范围覆盖也直接执行。
    #[test]
    fn full_access_never_asks() {
        let delete = request(ChangeScope::delete());
        assert_eq!(
            PermissionEngine::decide(PermissionMode::FullAccess, &delete, &[]),
            PermissionDecision::Allow
        );
        let irreversible = PermissionRequest {
            risk: RiskLevel::Irreversible,
            ..request(ChangeScope::whole())
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::FullAccess, &irreversible, &[]),
            PermissionDecision::Allow
        );
        // 只读级别仍然拒绝一切写入。
        assert_eq!(
            PermissionEngine::decide(PermissionMode::PlanOnly, &delete, &[]),
            PermissionDecision::Deny
        );
    }

    /// 每一档都有级别名与说明。
    #[test]
    fn every_level_is_described() {
        for (mode, label, description) in PermissionMode::levels() {
            assert!(!label.is_empty() && !description.is_empty(), "{mode:?}");
            assert_eq!(mode.label(), *label);
            assert_eq!(mode.description(), *description);
        }
    }

    /// 请求审批级别对每次写入都询问。
    #[test]
    fn ask_each_still_prompts_for_every_write() {
        assert_eq!(
            PermissionEngine::decide(
                PermissionMode::AskEach,
                &request(ChangeScope::fields(1)),
                &[]
            ),
            PermissionDecision::Ask
        );
    }
}
