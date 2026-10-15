use super::*;

impl Supervisor {
    /// Persist a conservative charge before sending; replace it only with a
    /// received usage report. Cancellation/discarding is never reported as free.
    pub(super) async fn complete_auxiliary(
        &self,
        run: &Run,
        purpose: &str,
        model: crate::config::ModelConfig,
        messages: &[Message],
        cancel: &CancellationToken,
    ) -> Result<crate::model::ModelResponse> {
        let protocol = model.protocol;
        let input = context::estimate_messages_tokens(messages);
        let output = policy::output_reserve(&model);
        if input + output > model.options.context_window {
            return Err(Error::Tool(
                "模型窗口不足以核对本次目标和操作范围，未发送辅助请求，也未执行该操作".into(),
            ));
        }
        let id = self
            .journal
            .begin_model_request(run, purpose, &model, input, output)?;
        let seconds = if purpose == "memory" {
            120
        } else {
            (run.deadline - unix_now()).clamp(1, 1800) as u64
        };
        let result = tokio::time::timeout(
            Duration::from_secs(seconds),
            gateway::complete(model, messages, &[], cancel, |_| Ok(())),
        )
        .await
        .map_err(|_| Error::Timeout("模型辅助请求超过时限；用量保留为估算".into()))
        .and_then(|r| r);
        self.journal.finish_model_request(&id, protocol, &result)?;
        result
    }
}
