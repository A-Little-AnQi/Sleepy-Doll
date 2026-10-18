use super::*;

impl Supervisor {
    /// Credentials belong to transport configuration, not model-visible context.
    /// Signed reasoning and tool calls cannot be rewritten safely: refuse such
    /// legacy history rather than corrupting protocol replay or leaking it.
    pub(super) fn model_messages(&self, messages: &[Message]) -> Result<Vec<Message>> {
        let config = self.config.read().unwrap();
        let mut secrets = config
            .models
            .iter()
            .filter_map(|model| model.api_key.clone())
            .collect::<Vec<_>>();
        if let Some(token) = &config.bridge.token {
            secrets.push(token.clone());
        }
        for model in &config.models {
            for (key, value) in &model.headers {
                let key = key.to_lowercase();
                if ["auth", "key", "token", "secret", "cookie"]
                    .iter()
                    .any(|part| key.contains(part))
                {
                    secrets.push(value.clone());
                }
            }
        }
        secrets.retain(|value| value.chars().count() >= 12);
        secrets.sort_by_key(|value| std::cmp::Reverse(value.len()));
        secrets.dedup();
        let contains = |text: &str| secrets.iter().any(|secret| text.contains(secret));
        let mut projected = messages.to_vec();
        for entry in &mut projected {
            if entry.reasoning.as_ref().is_some_and(|reasoning| {
                contains(&serde_json::to_string(reasoning).unwrap_or_default())
            }) || entry
                .tool_calls
                .iter()
                .any(|call| contains(&call.arguments.to_string()))
            {
                return Err(Error::Config("原对话的推理或工具参数中含当前连接凭据，无法安全继续发送；请改用新对话，软件不会发送这段内容。".into()));
            }
            for secret in &secrets {
                entry.content = entry.content.replace(secret, "[REDACTED]");
                let escaped = serde_json::to_string(secret)?;
                entry.content = entry
                    .content
                    .replace(&escaped[1..escaped.len() - 1], "[REDACTED]");
            }
        }
        Ok(projected)
    }

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
        let messages = self.model_messages(messages)?;
        let protocol = model.protocol;
        let input = context::estimate_messages_tokens(&messages);
        let output = policy::output_reserve(&model);
        let configured_window = self
            .config
            .read()
            .unwrap()
            .runtime
            .max_tokens
            .unwrap_or(model.options.context_window);
        if input + output > model.options.context_window.min(configured_window) {
            return Err(Error::Tool(
                "模型窗口不足以核对本次目标和操作范围，未发送辅助请求，也未执行该操作".into(),
            ));
        }
        let id = self
            .journal
            .begin_model_request(run, purpose, &model, input, output)?;
        let seconds = policy::auxiliary_timeout(purpose, run.deadline - unix_now());
        let result = tokio::time::timeout(
            Duration::from_secs(seconds),
            gateway::complete(model, &messages, &[], cancel, |_| Ok(())),
        )
        .await
        .map_err(|_| Error::Timeout("模型辅助请求超过时限；用量保留为估算".into()))
        .and_then(|r| r);
        self.journal.finish_model_request(&id, protocol, &result)?;
        result
    }
}
