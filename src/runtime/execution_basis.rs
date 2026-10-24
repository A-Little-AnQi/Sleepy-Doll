//! 执行依据是人类目标、已绑定计划与资源版本的派生视图，不产生新授权。
use super::*;
use std::collections::BTreeMap;

const HEADER: &str = "[本轮输入、计划及真实观测记录；完整任务由真实对话解释，本记录不产生新授权]";

pub(super) fn understanding_schema() -> Value {
    let item = json!({"type":"array","maxItems":8,"items":{"type":"object","additionalProperties":false,"required":["text","quote"],"properties":{"text":{"type":"string","minLength":1,"maxLength":240},"quote":{"type":"string","minLength":1,"maxLength":400}}}});
    json!({"type":"object","additionalProperties":false,"properties":{"targets":item,"allowedEffects":item,"prohibitedEffects":item,"completionCriteria":item}})
}

pub(super) struct ExecutionBasis {
    bindings: BTreeMap<String, Value>,
    uncertain_effect: Option<String>,
    pub view: Value,
}

fn binding(value: &Value) -> Value {
    let mut fields = serde_json::Map::new();
    for key in [
        "path",
        "resourceId",
        "instanceId",
        "catalogVersion",
        "version",
        "resourceVersion",
        "hash",
        "contentHash",
        "sha256",
        "etag",
    ] {
        if let Some(value) = value.get(key).filter(|v| v.is_string() || v.is_number()) {
            fields.insert(key.into(), value.clone());
        }
    }
    Value::Object(fields)
}

impl Supervisor {
    pub(super) fn validate_goal_understanding(&self, run: &Run, value: &Value) -> Result<()> {
        if !crate::extension::validate(value, &understanding_schema(), "$").is_empty() {
            return Err(Error::Tool("目标解释的结构不符合契约".into()));
        }
        let mut inputs = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.journal.conversation_inputs(run, offset, 20)?;
            inputs.extend(
                page["inputs"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|input| input["text"].as_str())
                    .map(str::to_owned),
            );
            let Some(next) = page["nextOffset"].as_u64() else {
                break;
            };
            offset = next as usize;
        }
        self.journal.ensure_current_goal(run)?;
        for entries in value.as_object().into_iter().flat_map(|v| v.values()) {
            for item in entries.as_array().into_iter().flatten() {
                let quote = item["quote"].as_str().unwrap_or_default();
                if quote.trim().is_empty() || !inputs.iter().any(|input| input.contains(quote)) {
                    return Err(Error::Tool(
                        "目标解释必须引用当前对话的真实用户输入，不能引用工具或历史经验".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn save_goal_understanding(&self, run: &Run, value: &Value) -> Result<()> {
        self.validate_goal_understanding(run, value)?;
        self.journal.emit(
            run,
            "goal.understood",
            json!({"interpretation":value,"source":"derivedFromHumanQuotes"}),
        )
    }

    pub(super) fn execution_basis(&self, run: &Run) -> Result<ExecutionBasis> {
        let goal = self
            .journal
            .goal(&run.id)?
            .ok_or_else(|| Error::Stale("本次运行缺少真实用户目标".into()))?;
        let plan = self.journal.plan(&run.id)?;
        let understanding = self.journal.goal_understanding(&run.id, goal.revision)?;
        let mut bindings = BTreeMap::new();
        let mut uncertain_effect = None;
        let mut recent_targets = Vec::new();
        for receipt in self.journal.goal_receipts(&run.id, goal.revision)? {
            let mut bound = false;
            for value in [
                &receipt.evidence["value"],
                &receipt.evidence["value"]["resource"],
                &receipt.evidence["value"]["evidence"],
            ] {
                let metadata = binding(value);
                let has_version = [
                    "version",
                    "resourceVersion",
                    "hash",
                    "contentHash",
                    "sha256",
                    "etag",
                    "catalogVersion",
                ]
                .iter()
                .any(|key| metadata.get(*key).is_some());
                if !has_version {
                    continue;
                }
                let target = metadata["path"]
                    .as_str()
                    .map(|s| format!("path:{s}"))
                    .or_else(|| {
                        metadata["resourceId"]
                            .as_str()
                            .map(|s| format!("resource:{s}"))
                    })
                    .unwrap_or_else(|| format!("tool:{}", receipt.tool));
                recent_targets.retain(|old| old != &target);
                recent_targets.push(target.clone());
                bindings.insert(target, metadata);
                bound = true;
            }
            if !matches!(
                receipt.effect,
                ToolEffect::ReadOnly | ToolEffect::InternalState
            ) && !bound
            {
                uncertain_effect = Some(receipt.id);
            }
        }
        let visible_targets = recent_targets
            .iter()
            .rev()
            .take(12)
            .filter_map(|key| bindings.get(key).map(|value| (key, value)))
            .collect::<BTreeMap<_, _>>();
        let view = json!({"goalRevision":goal.revision,"interpretation":understanding,"humanInputRevisions":goal.inputs.iter().map(|i| i.revision).collect::<Vec<_>>(),"planRevision":plan.as_ref().map(|p| p.revision),"targets":visible_targets,"omittedTargets":bindings.len().saturating_sub(visible_targets.len()),"plannedSteps":plan.as_ref().map(|p| p.steps.iter().take(16).map(|s| json!({"id":s.id,"tool":s.tool,"capabilityId":s.capability_id,"dependsOn":s.depends_on,"resourceVersions":s.resource_versions})).collect::<Vec<_>>()),"completion":"原始用户要求为准；已接收不等于已完成，计划写入和执行步骤需实际核验","invalidatedBy":["用户目标变化","计划或契约变化","相关资源版本变化","权限变化","无资源版本的副作用"]});
        Ok(ExecutionBasis {
            bindings,
            uncertain_effect,
            view,
        })
    }

    pub(super) fn attach_execution_basis(
        &self,
        run: &Run,
        history: &mut Vec<Message>,
    ) -> Result<()> {
        let basis = self.execution_basis(run)?;
        if basis.bindings.is_empty()
            && basis.uncertain_effect.is_none()
            && basis.view["plannedSteps"].is_null()
            && basis.view["interpretation"].is_null()
        {
            return Ok(());
        }
        self.attach_context_version(run, history, HEADER, &basis.view.to_string())
    }
}
