use std::{
    sync::{Arc, RwLock},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum HookEventKind {
    RunStarted,
    BeforePlanCommit,
    BeforeToolUse,
    AfterToolUse,
    ToolUseFailed,
    ApprovalRequested,
    VerificationFailed,
    RunNeedsReview,
    RunCompleted,
    AdapterStarted,
    AdapterFailed,
    BridgeDisconnected,
    BridgeReconnected,
    OperationPrepared,
    OperationCommitted,
    OperationRolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookEvent {
    pub kind: HookEventKind,
    pub run_id: Option<String>,
    pub operation_id: Option<String>,
    pub occurred_at: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpHookConfig {
    pub id: String,
    pub event: HookEventKind,
    pub url: String,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub can_block: bool,
    #[serde(default)]
    pub can_append_context: bool,
    #[serde(default)]
    pub failure_fatal: bool,
}

const fn default_timeout() -> u64 {
    3000
}

#[derive(Debug, Clone, Default)]
pub struct HookOutcome {
    pub blocked: bool,
    pub reason: Option<String>,
    pub context: Vec<String>,
}

type InternalHook = Arc<dyn Fn(&HookEvent) -> Result<HookOutcome> + Send + Sync>;

#[derive(Default)]
pub struct HookBus {
    internal: RwLock<Vec<(HookEventKind, InternalHook)>>,
    http: Vec<HttpHookConfig>,
}

impl HookBus {
    pub fn new(http: Vec<HttpHookConfig>) -> Result<Self> {
        for hook in &http {
            if hook.id.trim().is_empty()
                || !local_http_url(&hook.url)
                || hook.timeout_ms == 0
                || hook.timeout_ms > 30_000
            {
                return Err(Error::Config(
                    "Hook 必须使用有效 ID、本地 HTTP 地址和 30 秒内超时".into(),
                ));
            }
        }
        Ok(Self {
            internal: RwLock::new(vec![]),
            http,
        })
    }

    pub fn register_internal(&self, event: HookEventKind, hook: InternalHook) {
        self.internal.write().unwrap().push((event, hook));
    }

    pub async fn emit(&self, event: &HookEvent) -> Result<HookOutcome> {
        let mut outcome = HookOutcome::default();
        for (_, hook) in self
            .internal
            .read()
            .unwrap()
            .iter()
            .filter(|(kind, _)| kind == &event.kind)
        {
            merge(&mut outcome, hook(event)?);
        }
        for hook in self.http.iter().filter(|hook| hook.event == event.kind) {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_millis(hook.timeout_ms))
                .build()?;
            let response = client.post(&hook.url).json(event).send().await;
            let value = match response {
                Ok(response) if response.status().is_success() => {
                    response.json::<Value>().await.map_err(Error::from)
                }
                Ok(_) => Err(Error::Tool("Hook 返回失败状态".into())),
                Err(error) => Err(Error::from(error)),
            };
            match value {
                Ok(value) => {
                    if hook.can_block && value["blocked"] == true {
                        outcome.blocked = true;
                        outcome.reason = value["reason"].as_str().map(str::to_owned);
                    }
                    if hook.can_append_context {
                        outcome.context.extend(
                            value["context"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .filter(|text| text.len() <= 4096),
                        );
                    }
                }
                Err(error) if hook.failure_fatal => return Err(error),
                Err(_) => {}
            }
        }
        if outcome.blocked {
            return Err(Error::Conflict(
                outcome
                    .reason
                    .clone()
                    .unwrap_or_else(|| "操作被生命周期 Hook 阻止".into()),
            ));
        }
        Ok(outcome)
    }

    pub fn event(
        kind: HookEventKind,
        run_id: Option<&str>,
        operation_id: Option<&str>,
        data: Value,
    ) -> HookEvent {
        HookEvent {
            kind,
            run_id: run_id.map(str::to_owned),
            operation_id: operation_id.map(str::to_owned),
            occurred_at: super::types::now(),
            data,
        }
    }
}

fn merge(target: &mut HookOutcome, source: HookOutcome) {
    target.blocked |= source.blocked;
    if target.reason.is_none() {
        target.reason = source.reason;
    }
    target
        .context
        .extend(source.context.into_iter().filter(|text| text.len() <= 4096));
}

/// Accepts loopback HTTP targets only. The URL is parsed rather than
/// prefix-matched: `http://127.0.0.1:9@evil.com` starts with a loopback prefix
/// but resolves to `evil.com`, so a prefix test would post hook payloads — tool
/// arguments and results — to an external host.
fn local_http_url(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "http" {
        return false;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return false;
    }
    match parsed.host() {
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_reject_remote_and_shell_targets() {
        assert!(
            HookBus::new(vec![HttpHookConfig {
                id: "x".into(),
                event: HookEventKind::RunStarted,
                url: "https://example.com".into(),
                timeout_ms: 1000,
                can_block: false,
                can_append_context: false,
                failure_fatal: false
            }])
            .is_err()
        );
        assert!(
            HookBus::new(vec![HttpHookConfig {
                id: "x".into(),
                event: HookEventKind::RunStarted,
                url: "cmd.exe /c echo bad".into(),
                timeout_ms: 1000,
                can_block: false,
                can_append_context: false,
                failure_fatal: false
            }])
            .is_err()
        );
    }

    #[test]
    fn hooks_reject_loopback_prefixes_that_actually_resolve_elsewhere() {
        // `127.0.0.1:9` is userinfo here, not the host, so a string prefix test
        // would let the payload leave the machine.
        for target in [
            "http://127.0.0.1:9@evil.com/collect",
            "http://localhost:1@evil.com/collect",
            "http://localhost.attacker.com/collect",
            "http://127.0.0.1.attacker.com/collect",
            "http://[::1]:1@evil.com/collect",
            "https://127.0.0.1:9000/hook",
            "http://169.254.169.254/latest/meta-data/",
        ] {
            assert!(
                HookBus::new(vec![HttpHookConfig {
                    id: "x".into(),
                    event: HookEventKind::RunStarted,
                    url: target.into(),
                    timeout_ms: 1000,
                    can_block: false,
                    can_append_context: false,
                    failure_fatal: false
                }])
                .is_err(),
                "{target} must not be accepted as a local hook"
            );
        }
        for target in [
            "http://127.0.0.1:9000/hook",
            "http://localhost:9000/hook",
            "http://[::1]:9000/hook",
        ] {
            assert!(
                HookBus::new(vec![HttpHookConfig {
                    id: "x".into(),
                    event: HookEventKind::RunStarted,
                    url: target.into(),
                    timeout_ms: 1000,
                    can_block: false,
                    can_append_context: false,
                    failure_fatal: false
                }])
                .is_ok(),
                "{target} is a loopback hook and must be accepted"
            );
        }
    }

    #[test]
    fn internal_blocking_hook_is_enforced() {
        let bus = HookBus::new(vec![]).unwrap();
        bus.register_internal(
            HookEventKind::BeforeToolUse,
            Arc::new(|_| {
                Ok(HookOutcome {
                    blocked: true,
                    reason: Some("blocked".into()),
                    context: vec![],
                })
            }),
        );
        let result = super::super::executor().block_on(bus.emit(&HookBus::event(
            HookEventKind::BeforeToolUse,
            None,
            None,
            serde_json::json!({}),
        )));
        assert!(result.is_err());
    }
}
