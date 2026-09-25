use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    extension::{RiskLevel, ToolEffect, UnattendedPolicy},
};

/// 审批级别：运行时据此决定写入是否需要询问。默认 `AskEach`。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// 只读：任何写入都拒绝。
    PlanOnly,
    /// 每次写入都问。
    #[default]
    #[serde(alias = "standard")]
    AskEach,
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
            .unwrap_or("请求审批")
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
