//! 通用联网工具：主模型选择使用时机，HTTP/MCP 层负责实际检索与读取。
use super::*;

pub(super) fn search_properties() -> Value {
    json!({"query":{"type":"string","minLength":1,"maxLength":600,"description":"要查找的问题或事实，只包含必要的公开信息"},
        "allowedDomains":{"type":"array","maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":3,"maxLength":253},"description":"可选，只保留指定域名及其子域，与blockedDomains二选一"},
        "blockedDomains":{"type":"array","maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":3,"maxLength":253}},
        "maxTokens":{"type":"integer","minimum":256,"maximum":32768,"default":4096,"description":"返回结果的合计估算token预算，完整结果可按产物标识续读"},
        "refresh":{"type":"boolean","default":false,"description":"明确需要更新时重新搜索，否则复用15分钟内的相同检索"},
        "limit":{"type":"integer","minimum":1,"maximum":30,"default":8}})
}

pub(super) fn fetch_properties() -> Value {
    json!({"url":{"type":"string","minLength":10,"maxLength":2048,"description":"公开HTTP(S)网页地址"},
        "focus":{"type":"string","minLength":1,"maxLength":600,"description":"本次需要从网页核对的内容；省略时读取主要信息"},
        "mode":{"type":"string","enum":["auto","extract","raw"],"default":"auto","description":"短页直读，长页按问题辅助提取；extract明确提取，raw按行读原文"},
        "sourceId":{"type":"string","description":"已取得的原文标识，复用原文而不重新下载"},
        "startLine":{"type":"integer","minimum":1,"default":1},
        "lineCount":{"type":"integer","minimum":1,"maximum":5000,"default":200},
        "maxTokens":{"type":"integer","minimum":256,"maximum":32768,"default":4096},
        "maxInputTokens":{"type":"integer","minimum":1024,"maximum":128000,"default":24000,"description":"辅助提取输入预算，同时受当前模型窗口约束"},
        "refresh":{"type":"boolean","default":false}})
}

impl Supervisor {
    pub(super) fn authorize_web(&self, run: &Run, _fetch: bool, _args: &Value) -> Result<()> {
        self.journal.ensure_current_goal(run)?;
        if self.journal.goal(&run.id)?.is_none() {
            return Err(Error::Tool("缺少当前用户任务".into()));
        }
        if !self
            .config
            .read()
            .unwrap()
            .plugin_enabled(crate::extension::providers::WEB_PROVIDER)
        {
            return Err(Error::Tool("网络搜索插件已停用".into()));
        }
        Ok(())
    }

    pub(super) async fn web_tool(
        &self,
        run: &Run,
        call: &ToolCall,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        self.authorize_web(run, call.name == "web.fetch", &call.arguments)?;
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if run.deadline <= unix_now() {
            return Err(Error::Timeout("本次任务已到时限".into()));
        }
        let arguments_text = call.arguments.to_string();
        let checked = self.model_messages(&[context::message(Role::User, &arguments_text)])?;
        if checked[0].content != arguments_text {
            return Err(Error::Tool(
                "联网参数包含连接凭据，请只使用必要的公开查询或无凭据地址。".into(),
            ));
        }
        if call.name == "web.fetch" {
            let model = self.model_for(run)?;
            return self.fetch_page(run, call, &model, cancel).await;
        }
        let mut arguments = call.arguments.clone();
        if let Some(properties) = arguments.as_object_mut() {
            for key in [
                "maxTokens",
                "refresh",
                "need",
                "userQuote",
                "knowledgeGap",
                "dependentAction",
            ] {
                properties.remove(key);
            }
        }
        let key = hash(&json!({"tool":"web.search","arguments":arguments}));
        for event in self
            .journal
            .events(&run.conversation_id, 0)?
            .iter()
            .rev()
            .filter(|event| event.run_id == run.id && event.kind == "web.result")
        {
            let age = unix_now() - event.data["result"]["retrievedAt"].as_i64().unwrap_or(0);
            if call.arguments["refresh"] != true
                && (0..900).contains(&age)
                && event.data["requestKey"] == key
                && event.data["result"]["complete"] == true
            {
                let mut value = event.data["result"].clone();
                if let Some(id) = value["artifactId"].as_str()
                    && self
                        .operations
                        .store
                        .can_read_artifact(id, "run", &run.id)?
                {
                    value = serde_json::from_slice(&self.operations.artifacts.get(id)?)?;
                }
                value = self.compact_search(run, &call.arguments, value)?;
                value["cacheHit"] = json!(true);
                return Ok(value);
            }
        }
        let timeout = Duration::from_secs((run.deadline - unix_now()).clamp(1, 120) as u64);
        let value = tokio::time::timeout(
            timeout,
            host::web_search::search(&call.arguments, timeout, cancel),
        )
        .await
        .map_err(|_| Error::Timeout("搜索超时，尚未取得资料。".into()))??;
        let value = self.compact_search(run, &call.arguments, value)?;
        self.journal
            .emit(run, "web.result", json!({"requestKey":key,"result":value}))?;
        Ok(value)
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;

    #[test]
    fn network_plugin_controls_discovery_and_execution_not_legacy_skill_switch() {
        let supervisor = super::super::record_result_tests::interrupt_supervisor("network-plugin");
        let run = supervisor
            .journal
            .create("核对公开资料", "network-plugin", "fixture", 300, None)
            .unwrap();
        supervisor
            .config
            .write()
            .unwrap()
            .agent
            .disabled_skills
            .push("网络搜索".into());
        assert_eq!(supervisor.native_plugin_tool_definitions().len(), 2);
        assert!(
            supervisor
                .introduced_providers()
                .contains(crate::extension::providers::WEB_PROVIDER)
        );
        assert!(
            supervisor
                .authorize_web(&run, false, &json!({"query":"fixture"}))
                .is_ok()
        );
        supervisor
            .config
            .write()
            .unwrap()
            .plugins
            .disabled
            .push(crate::extension::providers::WEB_PROVIDER.into());
        assert!(supervisor.native_plugin_tool_definitions().is_empty());
        assert!(
            !supervisor
                .introduced_providers()
                .contains(crate::extension::providers::WEB_PROVIDER)
        );
        for fetch in [false, true] {
            let error = supervisor
                .authorize_web(&run, fetch, &json!({}))
                .unwrap_err();
            assert!(error.to_string().contains("网络搜索插件已停用"));
        }
        supervisor.config.write().unwrap().plugins.disabled.clear();
        assert_eq!(supervisor.native_plugin_tool_definitions().len(), 2);
        supervisor.shutdown();
    }
}
