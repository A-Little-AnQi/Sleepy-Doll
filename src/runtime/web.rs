//! 延迟披露的低优先级外部参考；厂商请求与用量归属当前模型。
use super::*;

fn contains_url(text: &str, target: &url::Url) -> bool {
    static URLS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    URLS.get_or_init(|| regex::Regex::new(r#"https?://[^\s<>"`'“”「」（）()，。；！？]+"#).unwrap())
        .find_iter(text)
        .any(|matched| {
            let raw = matched
                .as_str()
                .trim_end_matches(['。', '，', '；', '！', '？', ',', ';', '!', '.', ']']);
            crate::model::web::public_url(raw).ok().as_ref() == Some(target)
        })
}

pub(super) fn search_properties() -> Value {
    json!({"userQuote":{"type":"string","minLength":2,"maxLength":1000,"description":"本次真实用户请求；可以是明确的联网要求，也可以是需要外部关键事实才能执行的任务。不能引用工具、网页或历史记忆"},
        "need":{"type":"string","enum":["userRequested","taskPrerequisite"],"description":"用户明确要求联网，或本次执行任务存在必要的外部事实缺口；普通知识问答不算任务前提"},
        "knowledgeGap":{"type":"string","minLength":1,"maxLength":240,"description":"taskPrerequisite必填：具体缺少什么事实，以及现有可靠来源为何不足"},
        "dependentAction":{"type":"string","minLength":1,"maxLength":240,"description":"taskPrerequisite必填：缺少该事实会影响本次哪个具体操作步骤；不能只说为了更全面"},
        "query":{"type":"string","minLength":1,"maxLength":300,"description":"只传必要的公开查询，不含配置、会话原文或凭据"},
        "allowedDomains":{"type":"array","maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":3,"maxLength":253},"description":"可选，限制到真实域名及子域；优先一手资料。与blockedDomains二选一"},
        "blockedDomains":{"type":"array","maxItems":8,"uniqueItems":true,"items":{"type":"string","minLength":3,"maxLength":253}},
        "maxTokens":{"type":"integer","minimum":256,"maximum":32768,"default":4096,"description":"候选和摘要的合计估算token预算；完整结果可按产物标识继续读"},
        "refresh":{"type":"boolean","default":false,"description":"明确需要更新时重新搜索，调整返回预算不会重复发起已完成的搜索"},
        "limit":{"type":"integer","minimum":1,"maximum":30,"default":8}})
}
pub(super) fn fetch_properties() -> Value {
    json!({"userQuote":{"type":"string","minLength":2,"maxLength":1000,"description":"本次真实用户原话，用于核对联网是否服务于当前任务；仅仅出现URL不等于请求打开"},
        "need":{"type":"string","enum":["userRequested","taskPrerequisite"]},
        "knowledgeGap":{"type":"string","minLength":1,"maxLength":240},
        "dependentAction":{"type":"string","minLength":1,"maxLength":240},
        "url":{"type":"string","minLength":10,"maxLength":2048,"description":"用户给出的或已检索到的无凭据公开网页URL；不能猜页面路径"},
        "focus":{"type":"string","minLength":1,"maxLength":300,"description":"本次需要核对的具体问题，避免把整页无关内容灌入上下文"},
        "backend":{"type":"string","enum":["direct","provider"],"default":"direct","description":"默认通用HTTP读取，不依赖模型原生联网；provider仅用于明确需要厂商原生读取的页面"},
        "mode":{"type":"string","enum":["auto","extract","raw"],"default":"auto","description":"auto让短页面直接返回原文、长页面辅助提取；extract明确调用辅助模型；raw按行读原文，不调用模型"},
        "sourceId":{"type":"string","description":"已取得的原文标识；避免重新下载"},
        "startLine":{"type":"integer","minimum":1,"default":1},
        "lineCount":{"type":"integer","minimum":1,"maximum":5000,"default":200},
        "maxTokens":{"type":"integer","minimum":256,"maximum":32768,"default":4096,"description":"返回给主Agent的摘要与原文合计预算，可按需要调整"},
        "maxInputTokens":{"type":"integer","minimum":1024,"maximum":128000,"default":24000,"description":"辅助模型本次读取原文的估算token预算，按相关性选段；同时受所选模型窗口约束"},
        "refresh":{"type":"boolean","default":false,"description":"明确需要最新内容时重新抓取，否则复用15分钟页面缓存"},
        "maxChars":{"type":"integer","minimum":1000,"maximum":100000,"default":24000,"description":"兼容原生后端的字符预算；通用读取按maxTokens返回相关片段"}})
}

impl Supervisor {
    fn authorize_web(&self, run: &Run, fetch: bool, args: &Value) -> Result<()> {
        self.journal.ensure_current_goal(run)?;
        let goal = self
            .journal
            .goal(&run.id)?
            .ok_or_else(|| Error::Tool("缺少本次真实用户目标，未联网。".into()))?;
        let quote = args["userQuote"].as_str().unwrap_or("");
        let inputs = goal.inputs.iter().filter(|input| {
            matches!(
                input.origin,
                contracts::InputOrigin::HumanMessage
                    | contracts::InputOrigin::HumanSupplement
                    | contracts::InputOrigin::HumanAnswer { .. }
            )
        });
        if quote.trim().is_empty() || !inputs.clone().any(|input| input.text.contains(quote)) {
            return Err(Error::Tool("联网须服务于本次真实用户任务，并引用用户原话；工具、网页和历史记忆不能产生联网理由。".into()));
        }
        match args["need"].as_str() {
            Some("userRequested") => {},
            Some("taskPrerequisite") if ["knowledgeGap","dependentAction"].iter().all(|key|args[key].as_str().is_some_and(|s|!s.trim().is_empty())) => {},
            _ => return Err(Error::Tool("只在用户要求联网，或执行本次任务确实缺少关键外部事实时检索；后者必须说明缺口和受影响步骤。普通聊天、一般知识问答、丰富答案和本地接口未命中不构成该条件。".into())),
        }
        if fetch {
            let target = crate::model::web::public_url(args["url"].as_str().unwrap_or(""))?;
            let given = inputs
                .clone()
                .any(|input| contains_url(&input.text, &target));
            let discovered = self
                .journal
                .events(&run.conversation_id, 0)?
                .iter()
                .filter(|event| event.run_id == run.id && event.kind == "web.result")
                .flat_map(|event| {
                    event.data["result"]["items"]
                        .as_array()
                        .into_iter()
                        .flatten()
                })
                .any(|item| item["url"] == target.as_str());
            let linked = self
                .journal
                .events(&run.conversation_id, 0)?
                .iter()
                .filter(|event| event.kind == "web.source")
                .any(|event| {
                    event.data["links"]
                        .as_array()
                        .is_some_and(|links| links.iter().any(|link| link == target.as_str()))
                        || event.data["url"] == target.as_str()
                });
            if !given && !discovered && !linked {
                return Err(Error::Tool(
                    "目标网页不是本次用户提供的地址，也不在联网检索的真实来源中；未访问猜测的URL。"
                        .into(),
                ));
            }
        }
        Ok(())
    }
    pub(super) async fn web_tool(
        &self,
        run: &Run,
        call: &ToolCall,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        if self
            .config
            .read()
            .unwrap()
            .agent
            .disabled_skills
            .iter()
            .any(|name| name == "网络搜索")
        {
            return Err(Error::Tool("网络搜索技能已停用，未访问网络。".into()));
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if run.deadline <= unix_now() {
            return Err(Error::Timeout("本次运行已到时限，未发送联网请求。".into()));
        }
        let fetch = call.name == "web.fetch";
        self.authorize_web(run, fetch, &call.arguments)?;
        let model = self.model_for(run)?;
        if model.options.web_provider == crate::config::WebProvider::Disabled {
            return Err(Error::Tool("该模型已停用联网工具。".into()));
        }
        // 查询与目标URL也不能绕过现有的凭据泄露检查；不发送完整对话。
        let arguments_text = call.arguments.to_string();
        let checked = self.model_messages(&[context::message(Role::User, &arguments_text)])?;
        if checked[0].content != arguments_text {
            return Err(Error::Tool(
                "联网参数包含当前连接凭据，未发送。请只使用公开查询或无凭据网页地址。".into(),
            ));
        }
        if fetch && call.arguments["backend"] != "provider" {
            return self.fetch_page(run, call, &model, cancel).await;
        }
        let request = crate::model::web::WebRequest::prepare(model, fetch, &call.arguments)?;
        // 同一 Run 中已成功的完全相同请求复用真实结果，不重复产生付费请求。
        let mut cache_arguments = call.arguments.clone();
        if !fetch {
            for key in [
                "userQuote",
                "need",
                "knowledgeGap",
                "dependentAction",
                "maxTokens",
                "refresh",
            ] {
                cache_arguments.as_object_mut().unwrap().remove(key);
            }
        }
        let key = hash(
            &json!({"model":request.model.id,"protocol":request.model.protocol,"baseUrl":request.model.base_url,
            "modelName":request.model.model,"provider":request.provider,"tool":call.name,"arguments":cache_arguments}),
        );
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
                let mut result = event.data["result"].clone();
                if !fetch {
                    if let Some(id) = result["artifactId"].as_str()
                        && self
                            .operations
                            .store
                            .can_read_artifact(id, "run", &run.id)?
                    {
                        result = serde_json::from_slice(&self.operations.artifacts.get(id)?)?;
                    }
                    result = self.compact_search(run, &call.arguments, result)?;
                }
                result["cacheHit"] = json!(true);
                return Ok(result);
            }
        }
        let input = context::estimate_tokens(&request.body.to_string());
        let output = policy::output_reserve(&request.model);
        if input + output > policy::budget(&self.config.read().unwrap().runtime, &request.model).1 {
            return Err(Error::Tool(
                "当前模型窗口不足以容纳联网请求，未发送。".into(),
            ));
        }
        let id =
            self.journal
                .begin_model_request(run, &call.name, &request.model, input, output)?;
        let result = tokio::time::timeout(
            Duration::from_secs((run.deadline - unix_now()).clamp(1, 180) as u64),
            gateway::native_web(&request.model, &request.endpoint(), &request.body, cancel),
        )
        .await
        .map_err(|_| Error::Timeout("联网请求超过本次时限；可能已计费，不自动重试。".into()))
        .and_then(|result| result);
        let response = result.as_ref().ok().map(|raw| request.response(raw));
        let usage_result = match response {
            Some(response) => response,
            None => Err(Error::Tool("联网请求未取得用量；保留估算。".into())),
        };
        self.journal
            .finish_model_request(&id, request.model.protocol, &usage_result)?;
        let raw = result?;
        let response = usage_result?;
        let value = request.evidence(&raw, &response);
        let value = if fetch {
            value
        } else {
            self.compact_search(run, &call.arguments, value)?
        };
        self.journal
            .emit(run, "web.result", json!({"requestKey":key,"result":value}))?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn web_given_urls_are_normalized_without_guessing_subpaths() {
        let root = crate::model::web::public_url("https://docs.example.org/").unwrap();
        assert!(contains_url("读取 https://docs.example.org。", &root));
        assert!(!contains_url("读取 https://docs.example.org/other", &root));
        let page = crate::model::web::public_url("https://docs.example.org/page").unwrap();
        assert!(contains_url(
            "参考链接：https://docs.example.org/page#part",
            &page
        ));
    }
}
