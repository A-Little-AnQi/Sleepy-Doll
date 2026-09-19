use crate::runtime::host::catalog::Predicate;
use crate::runtime::types::unix_now;
use serde_json::Value;

/// 按配置的谓词核对快照，返回 unknown / verifiedSucceeded / verifiedFailed。
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

/// 比较两个值：桥可能把同一个数写成 `1`、`1.0` 或字符串。
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
