//! 通道显式开放的推理档位切换：只影响当前运行，不换模型或全局设置。
use super::*;

impl Supervisor {
    pub(super) fn reasoning_definition(&self) -> Option<ToolDefinition> {
        let efforts = self
            .config
            .read()
            .unwrap()
            .runtime
            .reasoning_efforts
            .clone();
        if efforts.is_empty() {
            return None;
        }
        Some(ToolDefinition {
            name: "reasoning.set".into(), label: "调整本轮思考深度".into(),
            description: "在当前模型与同一任务内调整后续请求的思考深度，保留上下文及已查结果。变更档位时本批其它动作暂不执行，下一轮用实际生效档位重新决定和提交；本次已生成的答案不会自动获得新档位的推理。按推理难度和延迟需要选择，缺资料仍应读取，不因某个关键词固定切换。不修改全局配置或权限。".into(),
            input_schema: json!({"type":"object","properties":{"effort":{"type":"string","enum":efforts},"reason":{"type":"string","minLength":1}},"required":["effort","reason"],"additionalProperties":false}),
            output_schema: None, source: "core:runtime".into(), provider_version: Some(env!("CARGO_PKG_VERSION").into()),
            execution: ToolExecution { effect: ToolEffect::InternalState, deferred: false, always_load: true, model_usage: crate::extension::ModelUsage::None, ..ToolExecution::default() },
        })
    }

    pub(super) fn reasoning_for_run(&self, run: &Run) -> Result<Option<String>> {
        let allowed = self
            .config
            .read()
            .unwrap()
            .runtime
            .reasoning_efforts
            .clone();
        Ok(self
            .journal
            .latest_run_event_data(run, "reasoning.changed")?
            .filter(|value| value["goalRevision"] == json!(run.goal_revision))
            .and_then(|value| {
                value["effort"]
                    .as_str()
                    .filter(|effort| allowed.iter().any(|v| v == effort))
                    .map(str::to_owned)
            }))
    }

    pub(super) fn change_reasoning(
        &self,
        run: &Run,
        arguments: &Value,
        active: &mut Option<String>,
    ) -> Result<Value> {
        let config = self.config.read().unwrap();
        let effort = arguments["effort"].as_str().unwrap_or_default();
        if !config
            .runtime
            .reasoning_efforts
            .iter()
            .any(|value| value == effort)
        {
            return Err(Error::Tool(
                "本轮未开放该思考档位；用户指定的档位不能自行覆盖".into(),
            ));
        }
        let reason = arguments["reason"]
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Error::Tool("调整思考深度需要说明当前推理需要".into()))?;
        drop(config);
        self.journal.emit(run,"reasoning.changed",json!({"effort":effort,"reason":reason,"goalRevision":run.goal_revision,"modelId":run.model_id}))?;
        *active = Some(effort.into());
        Ok(
            json!({"effort":effort,"appliesTo":"currentRun","modelUnchanged":true,"contextPreserved":true}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_is_opt_in_run_scoped_and_manual_lock_cannot_be_overridden() {
        let s = crate::runtime::record_result_tests::interrupt_supervisor("reasoning-policy");
        let mut run = s
            .journal
            .create("当前问题", "reasoning", "reasoning", 60, None)
            .unwrap();
        run.goal_revision = Some(1);
        let mut active = None;
        let args = json!({"effort":"max","reason":"需要核对矛盾条件"});
        assert!(s.change_reasoning(&run, &args, &mut active).is_err());
        assert!(active.is_none());
        let before = s.model_for(&run).unwrap();
        s.config.write().unwrap().runtime.reasoning_efforts = vec!["low".into(), "max".into()];
        s.change_reasoning(&run, &args, &mut active).unwrap();
        assert_eq!(s.reasoning_for_run(&run).unwrap().as_deref(), Some("max"));
        assert_eq!(s.model_for(&run).unwrap().model, before.model);
        assert_eq!(
            s.model_for(&run).unwrap().options.reasoning_effort,
            before.options.reasoning_effort
        );
        run.goal_revision = Some(2);
        assert!(s.reasoning_for_run(&run).unwrap().is_none());
        s.config.write().unwrap().runtime.reasoning_efforts.clear();
        assert!(s.change_reasoning(&run, &args, &mut active).is_err());
    }
}
