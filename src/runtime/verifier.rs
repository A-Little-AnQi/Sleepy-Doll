use super::{catalog::Predicate, types::unix_now};
use serde_json::Value;

/// Only configured predicates over fresh observations may strengthen a Job's
/// unknown business outcome. Natural-language assistant text is never evidence.
pub fn verify(predicates: &[Predicate], snapshot: &Value, instance: &str) -> &'static str {
    if predicates.is_empty() || snapshot["instanceId"] != instance {
        return "unknown";
    }
    let observed = snapshot["observedAt"]
        .as_str()
        .and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|t| t.unix_timestamp());
    let Some(observed) = observed else {
        return "unknown";
    };
    let age = unix_now() - observed;
    let mut failed = false;
    for predicate in predicates {
        match predicate {
            Predicate::Equals {
                pointer,
                value,
                max_age_sec,
            } => {
                if age < -2 || age > *max_age_sec as i64 {
                    return "unknown";
                }
                let Some(actual) = snapshot.pointer(pointer) else {
                    return "unknown";
                };
                if actual.is_null() {
                    return "unknown";
                }
                failed |= !equivalent(actual, value);
            }
        }
    }
    if failed {
        "verifiedFailed"
    } else {
        "verifiedSucceeded"
    }
}

/// Bridges may serialize the same value differently, for example `1` against
/// `1.0`, or a number delivered as a string. A serialization difference alone
/// must not report a successful write as `verifiedFailed`.
fn equivalent(actual: &Value, expected: &Value) -> bool {
    if actual == expected {
        return true;
    }
    matches!((number(actual), number(expected)), (Some(a), Some(b)) if a == b)
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}
