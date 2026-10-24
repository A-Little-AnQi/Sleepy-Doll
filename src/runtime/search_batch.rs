//! 只读检索的有界调度；依赖、收齐与提前返回由调用参数决定。
use super::*;
use futures_util::{StreamExt, future::BoxFuture, stream::FuturesUnordered};
use serde::Deserialize;

const MAX_REQUESTS: usize = 8;
const MAX_CONCURRENCY: usize = 4;

pub(super) fn discovery_properties() -> Value {
    json!({"query":{"type":"string","minLength":1,"maxLength":200},
        "limit":{"type":"integer","minimum":1,"maximum":8,"default":8},
        "sourcePrefix":{"type":"string","minLength":1,"maxLength":120,"description":"按摘要中真实 source 的前缀筛选，保留其来源类型前缀；不是应用名。不知道 source 时省略。"},
        "effect":{"type":"string","enum":["readOnly","internalState","localWrite","gameWrite","externalWrite","write","unknown"]},
        "searchOnly":{"type":"boolean","description":"仅查可供 search.batch 使用的检索入口"}})
}

pub(super) fn properties() -> Value {
    json!({
        "requests":{"type":"array","minItems":1,"maxItems":MAX_REQUESTS,"items":{
            "type":"object","additionalProperties":false,"required":["id","tool","arguments"],"properties":{
                "id":{"type":"string","minLength":1,"maxLength":40},
                "tool":{"type":"string","minLength":1,"maxLength":120,"description":"searchProvider=true 的真实工具名；不是宿主 methodId"},
                "arguments":{"type":"object"},
                "after":{"type":"array","maxItems":MAX_REQUESTS,"uniqueItems":true,"items":{"type":"string"},"description":"前面请求的id；全部结束后才判断when"},
                "when":{"type":"string","enum":["always","allSuccess","allHits","anyHit","allEmpty","anyError"],"default":"always"}
            }}},
        "order":{"type":"string","enum":["parallel","sequential"]},
        "concurrency":{"type":"integer","minimum":1,"maximum":MAX_CONCURRENCY},
        "completion":{"type":"string","enum":["all","firstHit"]},
        "timeoutMs":{"type":"integer","minimum":1000,"maximum":60000,"default":20000}
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    id: String,
    tool: String,
    arguments: Value,
    #[serde(default)]
    after: Vec<String>,
    #[serde(default = "always")]
    when: String,
}
fn always() -> String {
    "always".into()
}
fn timeout_ms() -> u64 {
    20_000
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Batch {
    requests: Vec<Request>,
    order: String,
    concurrency: usize,
    completion: String,
    #[serde(default = "timeout_ms")]
    timeout_ms: u64,
}

impl Batch {
    fn parse(value: &Value, definitions: &[ToolDefinition]) -> Result<Self> {
        let schema = json!({"type":"object","properties":properties(),
            "required":["requests","order","concurrency","completion"],"additionalProperties":false});
        if !crate::extension::validate(value, &schema, "$").is_empty() {
            return Err(Error::Tool(
                "检索批次参数不符合契约；最多8项、并发1–4、超时1–60秒。".into(),
            ));
        }
        let batch: Self = serde_json::from_value(value.clone())?;
        if batch.order == "sequential" && batch.concurrency != 1 {
            return Err(Error::Tool("串行检索的 concurrency 必须为1。".into()));
        }
        let mut previous = HashSet::new();
        for request in &batch.requests {
            if request.id.trim().is_empty() || previous.contains(&request.id) {
                return Err(Error::Tool("检索id不能为空或重复。".into()));
            }
            if request.after.iter().any(|id| !previous.contains(id)) {
                return Err(Error::Tool(
                    "after 只能引用本批前面的请求id，不能引用自身、后项或未知id。".into(),
                ));
            }
            if request.when != "always" && request.after.is_empty() {
                return Err(Error::Tool("条件检索必须通过 after 指定前置请求。".into()));
            }
            let tool = definitions
                .iter()
                .find(|tool| tool.name == request.tool)
                .ok_or_else(|| Error::Tool(format!("未找到检索入口：{}", request.tool)))?;
            if !tool.execution.search_provider
                || tool.execution.effect != ToolEffect::ReadOnly
                || tool.execution.effect_is_dynamic
                || !tool.execution.can_run_concurrently()
                || tool.execution.goal_observation.is_some()
            {
                return Err(Error::Tool(format!(
                    "{} 不是可批量调用的只读检索入口。",
                    request.tool
                )));
            }
            if !crate::extension::validate(&request.arguments, &tool.input_schema, "$").is_empty() {
                return Err(Error::Tool(format!(
                    "检索 {} 的参数不符合 {} 当前契约；本批尚未执行。",
                    request.id, request.tool
                )));
            }
            previous.insert(request.id.clone());
        }
        Ok(batch)
    }
}

/// 只有明确的候选才算命中；未知形状与失败不会触发 allEmpty 或 firstHit。
fn outcome(value: &Value) -> &'static str {
    if value["ok"] == false
        || value.get("error").is_some_and(|v| !v.is_null())
        || value["verdict"] == "lookupFailed"
    {
        return "error";
    }
    if let Some(items) = value.as_array() {
        return if items.is_empty() { "empty" } else { "hit" };
    }
    if value["ok"] == true
        && let Some(inner) = value.get("value")
    {
        return outcome(inner);
    }
    if value["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty())
    {
        return "error";
    }
    if value["complete"] == false {
        return "unknown";
    }
    if let Some(patterns) = value["patterns"].as_array() {
        return if patterns
            .iter()
            .any(|p| p["totalMatches"].as_u64().unwrap_or(0) > 0)
        {
            "hit"
        } else {
            "empty"
        };
    }
    for key in ["resourceCandidates", "scriptCandidates", "legacyCatalog"] {
        if value.get(key).is_some_and(|v| outcome(v) == "hit") {
            return "hit";
        }
    }
    for key in [
        "items",
        "entries",
        "resources",
        "matches",
        "tools",
        "results",
    ] {
        if let Some(items) = value[key].as_array() {
            return if items.is_empty() { "empty" } else { "hit" };
        }
    }
    if let Some(found) = value["found"].as_bool() {
        return if found { "hit" } else { "empty" };
    }
    if let Some(total) = value["total"].as_u64() {
        return if total > 0 { "hit" } else { "empty" };
    }
    "unknown"
}

fn condition(when: &str, dependencies: &[&Value]) -> bool {
    let is = |value: &&Value, state: &str| value["outcome"] == state;
    match when {
        "always" => true,
        "allSuccess" => dependencies
            .iter()
            .all(|v| v["state"] == "completed" && v["outcome"] != "error"),
        "allHits" => dependencies.iter().all(|v| is(v, "hit")),
        "anyHit" => dependencies.iter().any(|v| is(v, "hit")),
        "allEmpty" => dependencies.iter().all(|v| is(v, "empty")),
        "anyError" => dependencies.iter().any(|v| is(v, "error")),
        _ => false,
    }
}

impl Supervisor {
    pub(super) fn discover_tools(&self, a: &Value, exposed: &mut HashSet<String>) -> Result<Value> {
        let schema = json!({"type":"object","properties":discovery_properties(),"required":["query"],"additionalProperties":false});
        if !crate::extension::validate(a, &schema, "$").is_empty() {
            return Err(Error::Tool("工具检索参数不符合契约。".into()));
        }
        let query = a["query"].as_str().unwrap_or("").trim();
        if query.is_empty() {
            return Err(Error::Tool("工具检索需要明确名称或关键词".into()));
        }
        let mut candidates = self.definitions(exposed);
        candidates.extend(self.tools().definitions());
        candidates.sort_by(|left, right| left.name.cmp(&right.name));
        candidates.dedup_by(|left, right| left.name == right.name);
        if let Some(prefix) = a["sourcePrefix"].as_str()
            && !candidates
                .iter()
                .any(|tool| tool.source.starts_with(prefix))
        {
            let sources = candidates
                .iter()
                .map(|tool| tool.source.as_str())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .take(8)
                .collect::<Vec<_>>()
                .join("、");
            return Err(Error::Tool(format!(
                "sourcePrefix 不匹配当前来源；这不是业务能力未找到。省略该筛选，或使用摘要中真实 source 的前缀。当前来源示例：{sources}。"
            )));
        }
        candidates.retain(|tool| {
            a["sourcePrefix"]
                .as_str()
                .is_none_or(|s| tool.source.starts_with(s))
                && (a["effect"].is_null() || json!(tool.execution.effect) == a["effect"])
                && (a["searchOnly"] != true || tool.execution.search_provider)
        });
        let found = discovery::search(
            candidates,
            query,
            a["limit"].as_u64().unwrap_or(8).clamp(1, 8) as usize,
        );
        for tool in &found {
            exposed.insert(tool.name.clone());
        }
        Ok(json!(found.iter().map(|tool|json!({"name":tool.name,"label":tool.label,
            "description":tool.description.chars().take(400).collect::<String>(),
            "descriptionTruncated":tool.description.chars().count()>400,"source":tool.source,"effect":tool.execution.effect,
            "searchProvider":tool.execution.search_provider,"contractExposed":true})).collect::<Vec<_>>()))
    }

    pub(super) fn search_batch<'a>(
        &'a self,
        run: &'a Run,
        call: &'a ToolCall,
        bridge: &'a host::bridge::Bridge,
        cancel: &'a CancellationToken,
        exposed: &'a mut HashSet<String>,
    ) -> BoxFuture<'a, Result<Value>> {
        Box::pin(async move {
            let mut definitions = self.tools().definitions();
            definitions.extend(self.definitions(exposed));
            // 内核定义优先于插件注册表中的同名入口。
            definitions.reverse();
            let batch = Batch::parse(&call.arguments, &definitions)?;
            let policy = self.config.read().unwrap().runtime.clone();
            let mut results: Vec<Option<Value>> = vec![None; batch.requests.len()];
            let mut started = vec![false; batch.requests.len()];
            let indices = batch
                .requests
                .iter()
                .enumerate()
                .map(|(i, r)| (r.id.as_str(), i))
                .collect::<HashMap<_, _>>();
            let token = cancel.child_token();
            let _cancel_on_drop = token.clone().drop_guard();
            let deadline = tokio::time::Instant::now()
                + Duration::from_millis(
                    batch
                        .timeout_ms
                        .min((run.deadline - unix_now()).max(0) as u64 * 1000),
                );
            type Job<'job> = BoxFuture<'job, (usize, Result<Value>, HashSet<String>)>;
            let mut active: FuturesUnordered<Job<'_>> = FuturesUnordered::new();
            let mut arrival_order = Vec::new();
            let mut evidence = Vec::new();
            let stop_reason = loop {
                if cancel.is_cancelled() {
                    break "cancelled";
                }
                if tokio::time::Instant::now() >= deadline {
                    break "timeout";
                }
                for (i, request) in batch.requests.iter().enumerate() {
                    if started[i] || results[i].is_some() {
                        continue;
                    }
                    let deps = request
                        .after
                        .iter()
                        .map(|id| results[indices[id.as_str()]].as_ref())
                        .collect::<Option<Vec<_>>>();
                    let Some(deps) = deps else {
                        if batch.order == "sequential" {
                            break;
                        }
                        continue;
                    };
                    if !condition(&request.when, &deps) {
                        results[i] = Some(
                            json!({"id":request.id,"tool":request.tool,"state":"skipped","outcome":"skipped","reason":"conditionNotMet"}),
                        );
                        continue;
                    }
                    if active.len() >= batch.concurrency {
                        break;
                    }
                    started[i] = true;
                    self.journal.emit(
                        run,
                        "search.started",
                        json!({"batchCallId":call.id,"id":request.id,"tool":request.tool}),
                    )?;
                    let mut local_run = run.clone();
                    let mut local_exposed = exposed.clone();
                    local_exposed.insert(request.tool.clone());
                    let child = ToolCall {
                        id: format!("{}:{}", call.id, request.id),
                        name: request.tool.clone(),
                        arguments: request.arguments.clone(),
                    };
                    let token = token.clone();
                    let policy = &policy;
                    active.push(Box::pin(async move {
                        let result = if child.name == "tools.search" {
                            self.discover_tools(&child.arguments, &mut local_exposed)
                        } else {
                            Box::pin(self.tool(
                                &mut local_run,
                                &child,
                                bridge,
                                policy,
                                &token,
                                &mut local_exposed,
                            ))
                            .await
                        };
                        (i, result, local_exposed)
                    }));
                }
                if results.iter().all(Option::is_some) {
                    break "allSettled";
                }
                if active.is_empty() {
                    return Err(Error::Tool("检索依赖没有可执行节点，未继续。".into()));
                }
                let completed = tokio::select! {
                    _ = cancel.cancelled() => { break "cancelled"; },
                    _ = tokio::time::sleep_until(deadline) => { break "timeout"; },
                    value = active.next() => value,
                };
                let Some((i, result, discovered)) = completed else {
                    continue;
                };
                exposed.extend(discovered);
                let request = &batch.requests[i];
                arrival_order.push(request.id.clone());
                let (state, classification, value) = match result {
                    Ok(value) => ("completed", outcome(&value), value),
                    Err(error) => ("failed", "error", json!({"error":error.user_message()})),
                };
                if classification == "hit" && request.tool != "tools.search" {
                    evidence.push(request.tool.clone());
                }
                let result = json!({"id":request.id,"tool":request.tool,"state":state,"outcome":classification,"value":value});
                self.journal.emit(run,"search.completed",json!({"batchCallId":call.id,"id":request.id,"state":state,"outcome":classification}))?;
                results[i] = Some(result);
                if batch.completion == "firstHit" && classification == "hit" {
                    break "firstHit";
                }
            };
            token.cancel();
            drop(active);
            let all_settled = results.iter().all(Option::is_some);
            for (i, result) in results.iter_mut().enumerate() {
                if result.is_none() {
                    let request = &batch.requests[i];
                    let state = if started[i] {
                        "abandoned"
                    } else {
                        "notStarted"
                    };
                    *result = Some(
                        json!({"id":request.id,"tool":request.tool,"state":state,"outcome":"unknown","reason":stop_reason}),
                    );
                    self.journal.emit(run,"search.completed",json!({"batchCallId":call.id,"id":request.id,"state":state,"reason":stop_reason}))?;
                }
            }
            if stop_reason == "cancelled" {
                return Err(Error::Cancelled);
            }
            evidence.sort();
            evidence.dedup();
            let found = if results.iter().flatten().any(|r| r["outcome"] == "hit") {
                Some(true)
            } else if all_settled
                && results
                    .iter()
                    .flatten()
                    .all(|r| r["outcome"] == "empty" || r["state"] == "skipped")
            {
                Some(false)
            } else {
                None
            };
            Ok(
                json!({"order":batch.order,"concurrency":batch.concurrency,"completion":batch.completion,
            "stopReason":stop_reason,"allSettled":all_settled,"completionOrder":arrival_order,
                "found":found,"searchEvidence":evidence,"results":results}),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hit_classification_does_not_turn_errors_unknown_or_empty_pages_into_matches() {
        assert_eq!(outcome(&json!({"ok":false,"total":0})), "error");
        assert_eq!(outcome(&json!({"message":"query accepted"})), "unknown");
        assert_eq!(outcome(&json!({"items":[],"total":12})), "empty");
        assert_eq!(
            outcome(&json!({"entries":[],"requiresCurrentVerification":true})),
            "empty"
        );
        assert_eq!(
            outcome(&json!({"entries":[{"entry":{"id":"experience"}}]})),
            "hit"
        );
        assert_eq!(
            outcome(&json!({"items":[],"resourceCandidates":{"items":["route"]}})),
            "hit"
        );
        assert_eq!(
            outcome(&json!({"ok":true,"value":{"items":["method"]}})),
            "hit"
        );
        assert_eq!(
            outcome(&json!({"complete":false,"patterns":[{"totalMatches":0}]})),
            "unknown"
        );
        assert_eq!(
            outcome(&json!({"complete":true,"patterns":[{"totalMatches":2}]})),
            "hit"
        );
    }
}
