use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    tools::{RiskLevel, ToolEffect, UnattendedPolicy},
};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    PlanOnly,
    #[default]
    AskEach,
    TrustedScopes,
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
        if request.effect == ToolEffect::ReadOnly {
            return PermissionDecision::Allow;
        }
        if mode == PermissionMode::PlanOnly {
            return PermissionDecision::Deny;
        }
        if request.risk == RiskLevel::Irreversible
            || request.unattended == UnattendedPolicy::Allowed && request.risk == RiskLevel::High
        {
            return PermissionDecision::Ask;
        }
        if mode == PermissionMode::AskEach {
            return PermissionDecision::Ask;
        }
        if request.resource_ids.is_empty() && request.resource_kinds.is_empty() {
            return PermissionDecision::Ask;
        }
        let now = super::types::unix_now();
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
        let request = PermissionRequest {
            provider_id: "p",
            resource_ids: &["r".into()],
            resource_kinds: &[],
            effect: ToolEffect::LocalWrite,
            risk: RiskLevel::Standard,
            unattended: UnattendedPolicy::Forbidden,
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::TrustedScopes, &request, &[grant]),
            PermissionDecision::Allow
        );
        let irreversible = PermissionRequest {
            risk: RiskLevel::Irreversible,
            ..request
        };
        assert_eq!(
            PermissionEngine::decide(PermissionMode::TrustedScopes, &irreversible, &[]),
            PermissionDecision::Ask
        );
    }
}
