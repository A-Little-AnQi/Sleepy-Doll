//! 检索分页是已观察到的覆盖状态；是否足以完成目标由同一 Agent 明确决定。
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Serialize, Deserialize)]
struct Ledger {
    goal_revision: Option<u64>,
    scopes: BTreeMap<String, Scope>,
}
#[derive(Serialize, Deserialize)]
struct Scope {
    tool: String,
    identity: Value,
    initial: Value,
    pages: BTreeMap<String, Page>,
    decision: Option<Decision>,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    cursor_argument: String,
}
#[derive(Serialize, Deserialize, PartialEq)]
struct Page {
    next: Value,
    more: bool,
    #[serde(default)]
    fingerprint: String,
}
#[derive(Serialize, Deserialize)]
struct Decision {
    disposition: String,
    reason: String,
}

impl Scope {
    fn frontier(&self) -> Option<Value> {
        let mut cursor = self.initial.clone();
        let mut seen = HashSet::new();
        loop {
            let key = cursor.to_string();
            if !seen.insert(key.clone()) {
                return Some(json!({"cursor":cursor,"stalled":true}));
            }
            let Some(page) = self.pages.get(&key) else {
                return Some(json!({"cursor":cursor,"stalled":false}));
            };
            if !page.more {
                return None;
            }
            if page.next.is_null() {
                return Some(json!({"cursor":cursor,"missingNextCursor":true}));
            }
            cursor = page.next.clone();
        }
    }
}

impl Supervisor {
    fn retrieval_ledger(&self, run: &Run) -> Result<Ledger> {
        let value = self
            .journal
            .latest_run_event_data(run, "retrieval.coverage")?;
        let state: Ledger = value
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        Ok(if state.goal_revision == run.goal_revision {
            state
        } else {
            Ledger {
                goal_revision: run.goal_revision,
                ..Default::default()
            }
        })
    }

    pub(super) fn record_retrieval_page(
        &self,
        run: &Run,
        call: &ToolCall,
        result: &Value,
    ) -> Result<()> {
        if result["ok"] != true {
            return Ok(());
        }
        let Some(spec) = self
            .definition(&call.name, &run.discovered.iter().cloned().collect())
            .and_then(|d| d.execution.pagination)
        else {
            return Ok(());
        };
        let body = &result["value"];
        let Some(more) = body.pointer(&spec.has_more).and_then(Value::as_bool) else {
            return Ok(());
        };
        let identity = spec
            .identity
            .iter()
            .map(|p| json!({"field":p,"value":body.pointer(p).cloned().unwrap_or(Value::Null)}))
            .collect::<Vec<_>>();
        if identity.is_empty() || identity.iter().all(|item| item["value"].is_null()) {
            return Ok(());
        }
        let identity = json!(identity);
        let id = hash(&json!({"tool":call.name,"identity":identity,"initial":spec.initial_cursor}));
        let mut ledger = self.retrieval_ledger(run)?;
        let scope = ledger.scopes.entry(id).or_insert_with(|| Scope {
            tool: call.name.clone(),
            identity,
            initial: spec.initial_cursor.clone(),
            pages: BTreeMap::new(),
            decision: None,
            arguments: call.arguments.clone(),
            cursor_argument: spec.cursor_argument.clone(),
        });
        let cursor = call
            .arguments
            .pointer(&spec.cursor_argument)
            .unwrap_or(&spec.initial_cursor)
            .to_string();
        let page = Page {
            more,
            next: body
                .pointer(&spec.next_cursor)
                .cloned()
                .unwrap_or(Value::Null),
            fingerprint: hash(&contracts::progress_value(body)),
        };
        if scope.pages.get(&cursor) != Some(&page) {
            scope.decision = None;
            scope.pages.insert(cursor, page);
        }
        self.journal
            .emit(run, "retrieval.coverage", serde_json::to_value(ledger)?)?;
        Ok(())
    }

    pub(super) fn retrieval_checkpoint(&self, run: &Run, draft: Value) -> Result<Option<Message>> {
        let ledger = self.retrieval_ledger(run)?;
        let pending = ledger
            .scopes
            .iter()
            .filter(|(_, s)| s.decision.is_none())
            .filter_map(|(id, s)| {
                s.frontier().map(
                    |next| json!({"scopeId":id,"tool":s.tool,"scope":s.identity,"unread":next}),
                )
            })
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(None);
        }
        self.journal.emit(
            run,
            "retrieval.completion.reopened",
            json!({"pending":pending,"uncommittedDraft":draft}),
        )?;
        Ok(Some(message(
            Role::Context,
            format!(
                "[准备交付时发现检索覆盖仍待处理；候选尚未发送，不是用户新要求]\n{}\n对照真实用户目标决定下一步。当前页没有相关内容，不能推出剩余页也没有；总体汇总、穷尽或不存在的结论必须覆盖其声称的范围。需要继续的页可调用 retrieval.continue，按已有查询自动续页，不改筛选；也可自主用原工具。不请求用户重复授权已委派的工作。更窄的关键词检索结束不能证明无关键词范围已读尽。若当前只问某个已查明的具体事实，未读部分确实不影响这个目标，或该查询不属于目标所需，可用 retrieval.resolve 的 notNeeded 说明；不要为了赶快结束把未读范围说成不相关。satisfied只用于实际读尽的范围。确实无法完成则标记 partial，并在输出中说明实际未完成范围。这里只检查已请求工具的覆盖，不要求所有搜索一律读完，不新增业务任务。",
                json!({"pending":pending})
            ),
        )))
    }

    pub(super) fn resolve_retrieval(&self, run: &Run, args: &Value) -> Result<Value> {
        let disposition = args["disposition"].as_str().unwrap_or_default();
        if !matches!(disposition, "satisfied" | "notNeeded" | "partial") {
            return Err(Error::Tool(
                "需要明确此范围已经足够、与目标无关，还是本次只能部分完成".into(),
            ));
        }
        let reason = args["reason"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| Error::Tool("覆盖决定需要对应真实用户目标的理由".into()))?;
        let ids = args["scopeIds"]
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or_else(|| Error::Tool("需要指定实际待处理的scopeId".into()))?;
        let mut ledger = self.retrieval_ledger(run)?;
        for id in ids {
            let id = id
                .as_str()
                .ok_or_else(|| Error::Tool("scopeId必须来自实际覆盖记录".into()))?;
            if !ledger.scopes.contains_key(id) {
                return Err(Error::Tool("该检索范围不存在于本次任务".into()));
            }
            if disposition == "satisfied" && ledger.scopes[id].frontier().is_some() {
                return Err(Error::Tool("此范围仍有未读页，不能标为satisfied；不同关键词或筛选的已结束查询不是本范围读尽的证明。需要的范围请用retrieval.continue续页；只有未读部分确实不影响具体目标时才可说明notNeeded".into()));
            }
        }
        for id in ids {
            ledger
                .scopes
                .get_mut(id.as_str().unwrap())
                .unwrap()
                .decision = Some(Decision {
                disposition: disposition.into(),
                reason: reason.into(),
            });
        }
        self.journal
            .emit(run, "retrieval.coverage", serde_json::to_value(&ledger)?)?;
        self.journal.emit(run, "retrieval.decision", args.clone())?;
        Ok(
            json!({"recorded":true,"disposition":disposition,"scopeIds":ids,"note":"覆盖决定不是新的事实；正文必须与实际证据和未完成范围一致"}),
        )
    }

    pub(super) fn retrieval_is_partial(&self, run: &Run) -> Result<bool> {
        Ok(self.retrieval_ledger(run)?.scopes.values().any(|s| {
            s.frontier().is_some()
                && s.decision
                    .as_ref()
                    .is_some_and(|d| d.disposition == "partial")
        }))
    }

    pub(super) fn retrieval_definition(&self) -> Option<ToolDefinition> {
        if !self
            .tools()
            .definitions()
            .iter()
            .any(|d| d.execution.pagination.is_some())
        {
            return None;
        }
        Some(ToolDefinition{name:"retrieval.resolve".into(),label:"确认检索范围与任务目标的关系".into(),description:"对真实用户目标明确决定：satisfied=实际页链已读尽；notNeeded=未读部分确实不影响当前具体目标或不属于目标所需；partial=本次无法完成，正文须说明范围。更窄的筛选不能冒充原范围覆盖，所需未读页用retrieval.continue自动续取或自主调用原工具，不以notNeeded绕过。scopeId来自运行时记录，不修改全局设置或权限。".into(),input_schema:json!({"type":"object","properties":{"scopeIds":{"type":"array","items":{"type":"string"},"minItems":1},"disposition":{"type":"string","enum":["satisfied","notNeeded","partial"]},"reason":{"type":"string","minLength":1}},"required":["scopeIds","disposition","reason"],"additionalProperties":false}),output_schema:None,source:"core:runtime".into(),provider_version:Some(env!("CARGO_PKG_VERSION").into()),execution:ToolExecution{effect:ToolEffect::InternalState,always_load:true,deferred:false,model_usage:crate::extension::ModelUsage::None,..ToolExecution::default()}})
    }

    pub(super) fn retrieval_continue_definition(&self) -> Option<ToolDefinition> {
        self.retrieval_definition()?;
        Some(ToolDefinition{name:"retrieval.continue".into(),label:"继续已有检索的分页".into(),description:"自动沿实际游标继续已选查询的未读页，保留原工具、筛选和权限，不重新选择查询范围。scopeIds来自待处理覆盖记录；省略则继续当前任务所有未处理范围。实际窗口剩余额度、工具预算或时限不足时保留进度，不谎称读尽。只是工具侧分页，不调用其它模型。".into(),input_schema:json!({"type":"object","properties":{"scopeIds":{"type":"array","items":{"type":"string"}}},"additionalProperties":false}),output_schema:None,source:"core:runtime".into(),provider_version:Some(env!("CARGO_PKG_VERSION").into()),execution:ToolExecution{effect:ToolEffect::ReadOnly,always_load:true,deferred:false,model_usage:crate::extension::ModelUsage::None,..ToolExecution::default()}})
    }

    pub(super) async fn continue_retrieval(
        &self,
        run: &mut Run,
        args: &Value,
        bridge: &host::bridge::Bridge,
        room: u64,
        cancel: &CancellationToken,
        exposed: &mut HashSet<String>,
    ) -> Result<Value> {
        let policy = self.config.read().unwrap().runtime.clone();
        let ledger = self.retrieval_ledger(run)?;
        let ids = if let Some(ids) = args["scopeIds"].as_array() {
            ids.iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| Error::Tool("scopeId必须来自当前覆盖记录".into()))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            ledger
                .scopes
                .iter()
                .filter(|(_, s)| s.decision.is_none() && s.frontier().is_some())
                .map(|(id, _)| id.clone())
                .collect()
        };
        if ids.iter().any(|id| !ledger.scopes.contains_key(id)) {
            return Err(Error::Tool("查询范围不属于本次任务".into()));
        }
        if ids.is_empty() {
            return Err(Error::Tool(
                "本轮没有可续页的已选查询，请先调用实际检索工具".into(),
            ));
        }
        let mut pages = Vec::new();
        let mut used = 0u64;
        let mut last_cost = 0u64;
        let mut stop = "exhausted";
        'collect: for id in ids {
            loop {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if unix_now() >= run.deadline {
                    stop = "deadline";
                    break 'collect;
                }
                if policy.max_tools > 0 && run.tool_calls >= policy.max_tools {
                    stop = "toolBudget";
                    break 'collect;
                }
                let state = self.retrieval_ledger(run)?;
                let Some(s) = state.scopes.get(&id) else {
                    stop = "goalChanged";
                    break 'collect;
                };
                let Some(frontier) = s.frontier() else {
                    break;
                };
                if used >= room || used.saturating_add(last_cost) > room {
                    stop = "contextBudget";
                    break 'collect;
                }
                if frontier["stalled"] == true || frontier["missingNextCursor"] == true {
                    stop = "cursorNotAdvancing";
                    break 'collect;
                }
                let definition = self
                    .definition(&s.tool, exposed)
                    .ok_or_else(|| Error::Tool("原检索入口已不可用".into()))?;
                if definition.execution.effect != ToolEffect::ReadOnly
                    || definition.execution.effect_is_dynamic
                    || definition.execution.pagination.is_none()
                {
                    return Err(Error::Tool("自动续页只能使用原来明确只读的分页入口".into()));
                }
                let mut arguments = s.arguments.clone();
                set_cursor(
                    &mut arguments,
                    &s.cursor_argument,
                    frontier["cursor"].clone(),
                )?;
                let call = ToolCall {
                    id: format!("page_{}", uuid::Uuid::new_v4()),
                    name: s.tool.clone(),
                    arguments,
                };
                run.tool_calls += 1;
                self.journal.emit(
                    run,
                    "retrieval.page.started",
                    json!({"scopeId":id,"tool":call.name,"arguments":call.arguments}),
                )?;
                let result = match self
                    .tool(run, &call, bridge, &policy, cancel, exposed)
                    .await
                {
                    Ok(value) => value,
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => {
                        pages.push(json!({"scopeId":id,"error":error.user_message()}));
                        stop = "sourceError";
                        break 'collect;
                    }
                };
                let observed = tool_outcome(Ok(result.clone()), &observed_at(run), &call.name)?;
                self.record_retrieval_page(run, &call, &observed)?;
                let page = json!({"scopeId":id,"tool":call.name,"arguments":call.arguments,"value":result});
                let text = page.to_string();
                let hints = context::InputHints {
                    references: self.input_references.read().unwrap().clone(),
                    ..Default::default()
                };
                last_cost = context::estimate_tokens(&text) + hints.reference_tokens(&text);
                used = used.saturating_add(last_cost);
                pages.push(page);
                self.journal.emit(
                    run,
                    "retrieval.page.completed",
                    json!({"scopeId":id,"tool":call.name}),
                )?;
                if used > room {
                    stop = "contextBudget";
                    break 'collect;
                }
                if self
                    .retrieval_ledger(run)?
                    .scopes
                    .get(&id)
                    .and_then(Scope::frontier)
                    == Some(frontier)
                {
                    stop = "cursorNotAdvancing";
                    break 'collect;
                }
            }
        }
        let state = self.retrieval_ledger(run)?;
        let pending = state
            .scopes
            .iter()
            .filter_map(|(id, s)| {
                s.frontier().map(
                    |next| json!({"scopeId":id,"tool":s.tool,"scope":s.identity,"unread":next}),
                )
            })
            .collect::<Vec<_>>();
        Ok(
            json!({"pages":pages,"pending":pending,"stopReason":stop,"note":"页内容来自原查询的真实工具调用；剩余范围仍需按用户目标处理，不能把关键词子集当成原范围全集"}),
        )
    }
}

fn set_cursor(arguments: &mut Value, pointer: &str, value: Value) -> Result<()> {
    let mut parts = pointer
        .strip_prefix('/')
        .ok_or_else(|| Error::Tool("分页游标参数路径无效".into()))?
        .split('/')
        .peekable();
    let mut current = arguments;
    while let Some(part) = parts.next() {
        let key = part.replace("~1", "/").replace("~0", "~");
        let object = current
            .as_object_mut()
            .ok_or_else(|| Error::Tool("分页游标必须写入对象参数".into()))?;
        if parts.peek().is_none() {
            object.insert(key, value);
            return Ok(());
        }
        current = object.entry(key).or_insert_with(|| json!({}));
    }
    Err(Error::Tool("分页游标参数路径为空".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope() -> Scope {
        Scope {
            tool: "source.search".into(),
            identity: json!({"source":"selected","query":"range"}),
            initial: json!(0),
            pages: BTreeMap::new(),
            decision: None,
            arguments: json!({}),
            cursor_argument: "/offset".into(),
        }
    }
    fn page(next: Value, more: bool) -> Page {
        Page {
            next,
            more,
            fingerprint: "actual-source-result".into(),
        }
    }
    #[test]
    fn terminal_page_does_not_cover_an_unread_gap_and_all_cursor_links_are_required() {
        let mut s = scope();
        s.pages.insert("50".into(), page(json!(100), false));
        assert_eq!(s.frontier().unwrap()["cursor"], 0);
        s.pages.insert("0".into(), page(json!(20), true));
        assert_eq!(s.frontier().unwrap()["cursor"], 20);
        s.pages.insert("20".into(), page(json!(50), true));
        assert!(s.frontier().is_none());
    }
    #[test]
    fn stalled_or_missing_cursor_is_not_silently_exhausted() {
        let mut s = scope();
        s.pages.insert("0".into(), page(json!(0), true));
        assert_eq!(s.frontier().unwrap()["stalled"], true);
        s.pages.insert("0".into(), page(Value::Null, true));
        assert_eq!(s.frontier().unwrap()["missingNextCursor"], true);
    }
}
