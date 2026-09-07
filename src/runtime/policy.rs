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
    pub context_chars: usize,
    pub max_tokens: u64,
    pub grants: Vec<Grant>,
    pub permission_mode: super::permissions::PermissionMode,
    pub trust_grants: Vec<super::permissions::TrustGrant>,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            catalog_directory: ".".into(),
            max_decisions: 32,
            max_tools: 128,
            max_replans: 2,
            duration_sec: 1800,
            context_chars: 48000,
            max_tokens: 100000,
            grants: vec![],
            permission_mode: super::permissions::PermissionMode::AskEach,
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
impl RuntimeConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_decisions == 0
            || self.max_decisions > 128
            || self.max_tools == 0
            || self.max_tools > 1024
            || self.duration_sec < 1
            || self.duration_sec > 86400
            || self.context_chars < 4096
            || self.max_tokens < 1024
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
            super::permissions::PermissionEngine::validate_grant(grant)?;
        }
        Ok(())
    }
    pub fn allows(&self, request: &Value) -> bool {
        self.grants.iter().any(|g| {
            g.expires_at > super::types::unix_now()
                && request.get("capabilityId").unwrap_or(&request["methodId"]) == &g.capability_id
                && request["instanceId"] == g.instance_id
                && request["catalogVersion"] == g.catalog_version
                && request["arguments"] == g.arguments
                && super::types::hash(&request["resources"]) == g.resource_binding_hash
        })
    }
}
