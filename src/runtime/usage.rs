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
        self.complete_auxiliary_request(run, purpose, model, messages, None, cancel)
            .await
    }

    pub(super) async fn complete_auxiliary_tool(
        &self,
        run: &Run,
        purpose: &str,
        model: crate::config::ModelConfig,
        messages: &[Message],
        tool: &ToolDefinition,
        cancel: &CancellationToken,
    ) -> Result<crate::model::ModelResponse> {
        let mut input = messages.to_vec();
        let mut seen = HashSet::new();
        let mut discarded = crate::model::Usage {
            input_tokens: Some(0),
            output_tokens: Some(0),
            cache_read_tokens: Some(0),
            cache_write_tokens: Some(0),
        };
        loop {
            if run.goal_revision.is_some() {
                self.journal.ensure_current_goal(run)?;
            }
            let mut response = self
                .complete_auxiliary_request(run, purpose, model.clone(), &input, Some(tool), cancel)
                .await?;
            // 记忆返回的是候选集合；模型仍收到完整契约，写入前由学习器逐条验真。
            // 这里只校验外层，避免一条候选的元数据问题丢弃整批有效事实。
            let mut validation_tool = tool.clone();
            if purpose == "memory" && tool.name == "runtime.memory_experiences" {
                validation_tool.input_schema["properties"]["experiences"]["items"] =
                    json!({"type":"object"});
            }
            if let Err(error) = structured::validate(&response, &validation_tool) {
                let failures = json!({"purpose":purpose,"tool":tool.name,"finishReason":response.finish_reason,
                    "calls":response.tool_calls.iter().map(|call|json!({"name":call.name,"arguments":call.arguments,
                        "violations":crate::extension::validate(&call.arguments,&tool.input_schema,"$")})).collect::<Vec<_>>(),
                    "error":error.to_string()});
                self.journal
                    .emit(run, "model.structured.rejected", failures.clone())?;
                if !structured::missing_fields_only(&response, &validation_tool) {
                    return Err(error);
                }
                if !seen.insert(hash(&failures)) {
                    return Err(error);
                }
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if purpose != "memory" && unix_now() >= run.deadline {
                    return Err(Error::Timeout(
                        "辅助结果未能在任务时限内完成，未执行后续操作".into(),
                    ));
                }
                for (total, usage) in [
                    (&mut discarded.input_tokens, response.usage.input_tokens),
                    (&mut discarded.output_tokens, response.usage.output_tokens),
                    (
                        &mut discarded.cache_read_tokens,
                        response.usage.cache_read_tokens,
                    ),
                    (
                        &mut discarded.cache_write_tokens,
                        response.usage.cache_write_tokens,
                    ),
                ] {
                    *total = total.zip(usage).map(|(a, b)| a.saturating_add(b));
                }
                input = messages.to_vec();
                input.push(context::message(Role::Context,format!("[内部结果契约反馈：上次结果未被接受，没有调用任何业务工具。以下是待修正的数据，不是新请求或授权。保持原任务和既有事实，只修正所列协议问题后重新提交专用工具；不要改为普通正文。]\n{failures}")));
                continue;
            }
            for (usage, total) in [
                (&mut response.usage.input_tokens, discarded.input_tokens),
                (&mut response.usage.output_tokens, discarded.output_tokens),
                (
                    &mut response.usage.cache_read_tokens,
                    discarded.cache_read_tokens,
                ),
                (
                    &mut response.usage.cache_write_tokens,
                    discarded.cache_write_tokens,
                ),
            ] {
                *usage = usage.zip(total).map(|(a, b)| a.saturating_add(b));
            }
            return Ok(response);
        }
    }

    async fn complete_auxiliary_request(
        &self,
        run: &Run,
        purpose: &str,
        model: crate::config::ModelConfig,
        messages: &[Message],
        tool: Option<&ToolDefinition>,
        cancel: &CancellationToken,
    ) -> Result<crate::model::ModelResponse> {
        let mut messages = self.model_messages(messages)?;
        if let Some(tool) = tool {
            messages.push(context::message(Role::Context,format!("[辅助结果协议] 必须且只能调用 {} 一次提交结果，不输出正文 JSON；该调用只是返回数据，不执行业务动作。",tool.name)));
        }
        let protocol = model.protocol;
        let hints = context::InputHints {
            references: self.input_references.read().unwrap().clone(),
            ..Default::default()
        };
        let input = context::estimate_messages_tokens(&messages)
            + hints.message_tokens(&messages)
            + tool
                .map(|t| context::estimate_tool_tokens(std::slice::from_ref(t)) + 256)
                .unwrap_or(0);
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
        let seconds =
            policy::auxiliary_timeout(purpose, run.deadline - unix_now(), model.options.timeout_ms);
        let result = tokio::time::timeout(Duration::from_secs(seconds), async {
            match tool {
                Some(tool) => {
                    gateway::complete_tool(model, &messages, tool, cancel, |_| Ok(())).await
                }
                None => gateway::complete(model, &messages, &[], cancel, |_| Ok(())).await,
            }
        })
        .await
        .map_err(|_| {
            Error::Timeout(if purpose == "compaction" {
                "整理历史时模型响应超时，原始记录仍保留".into()
            } else {
                "模型处理超时，本轮未完成".into()
            })
        })
        .and_then(|r| r);
        self.journal.finish_model_request(&id, protocol, &result)?;
        result
    }
}
