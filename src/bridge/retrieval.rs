use crate::model::{Message, Role};
use crate::{
    error::{Error, Result},
    extension::ToolRegistry,
    runtime::host::bridge::Bridge,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;

/// BGI 提供方从本次请求的工具证据恢复检索进度。
#[derive(Default)]
pub struct RetrievalState {
    pub interface_searches: usize,
    pub empty_searches: usize,
    pub empty_repository_searches: usize,
    pub deferred_interface_searches: usize,
    pub deferred_searches: usize,
    named_scripts: Vec<String>,
    script_evidence: bool,
}

impl RetrievalState {
    pub fn from_history(history: &[Message]) -> Self {
        let mut state = Self::default();
        let mut names = HashMap::new();
        let mut deferred_rounds = HashSet::new();
        for (round, message) in history.iter().enumerate() {
            if message.role == Role::User {
                state = Self::default();
                names.clear();
                deferred_rounds.clear();
            }
            for call in &message.tool_calls {
                names.insert(call.id.as_str(), (call.name.as_str(), round));
            }
            if message.role != Role::Tool {
                continue;
            }
            let Some((name, round)) = message.tool_call_id.as_deref().and_then(|id| names.get(id))
            else {
                continue;
            };
            let Ok(result) = serde_json::from_str::<Value>(&message.content) else {
                continue;
            };
            if result["ok"] != true {
                continue;
            }
            let value = &result["value"];
            if matches!(*name, "bgi.api.search" | "bgi.repo.search") {
                let items = if *name == "bgi.api.search" {
                    &value["scriptCandidates"]["items"]
                } else {
                    &value["items"]
                };
                for item in items.as_array().into_iter().flatten() {
                    if item["namedInQuery"] == true
                        && let Some(path) =
                            item["path"].as_str().filter(|path| path.starts_with("js/"))
                        && !state.named_scripts.iter().any(|known| known == path)
                    {
                        state.named_scripts.push(path.to_owned());
                    }
                }
                if *name == "bgi.api.search" {
                    state.interface_searches += 1;
                }
                if value["total"].as_u64().unwrap_or(0) > 0 {
                    state.empty_searches = 0;
                    state.empty_repository_searches = 0;
                    state.deferred_searches = 0;
                    state.deferred_interface_searches = 0;
                    deferred_rounds.clear();
                }
                if value["total"] == 0 {
                    if *name == "bgi.api.search" {
                        state.empty_searches += 1;
                    } else {
                        state.empty_repository_searches += 1;
                    }
                }
                if value["searchDeferred"] == true && *name == "bgi.api.search" {
                    state.deferred_interface_searches += 1;
                }
                if value["searchDeferred"] == true && deferred_rounds.insert(*round) {
                    state.deferred_searches += 1;
                }
            } else if matches!(
                *name,
                "bgi.repo.read"
                    | "bgi.user.read"
                    | "bgi.user.inspect_script"
                    | "bgi.api.read"
                    | "skills.reference"
            ) && !value.is_null()
                && value["truncated"] != true
            {
                let path = value["path"].as_str().unwrap_or("").replace('\\', "/");
                let script_read = state.named_scripts.iter().any(|known| {
                    let folder = known.strip_prefix("js/").unwrap_or(known);
                    let from_repo = *name == "bgi.repo.read"
                        && path.starts_with(&format!("{known}/"))
                        && !path.ends_with("/manifest.json")
                        && value["lines"]
                            .as_array()
                            .is_some_and(|lines| !lines.is_empty());
                    let from_user = (*name == "bgi.user.inspect_script"
                        && value["folderName"] == folder)
                        || (*name == "bgi.user.read"
                            && path
                                .to_lowercase()
                                .contains(&format!("/jsscript/{folder}/").to_lowercase())
                            && !path.ends_with("/manifest.json"));
                    from_repo || from_user
                });
                if !state.named_scripts.is_empty() && !script_read {
                    continue;
                }
                state.script_evidence |= script_read;
                state.empty_searches = 0;
                state.empty_repository_searches = 0;
                state.deferred_searches = 0;
                state.deferred_interface_searches = 0;
                deferred_rounds.clear();
            }
        }
        state
    }

    pub fn needs_source_change(&self) -> bool {
        self.empty_searches >= 2
            || self.deferred_interface_searches > 0
            || (!self.named_scripts.is_empty() && !self.script_evidence)
    }

    pub fn search_stopped(&self) -> bool {
        self.deferred_searches >= 2
    }
}

fn reading_guide(value: &mut Value, guide: Option<&str>) {
    if value["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
        && guide.is_some()
    {
        value["readingGuide"] = json!({"skill":"bgi-javascript","next":"skills.read 加载 JS 阅读规则，再按 manifest 的 settings_ui/main 和字段引用读取必要源码；不自动装入整份手册。"});
    }
}

/// BGI 操作任务不通过软件目录里的桥二进制发现能力。
pub fn workspace_restriction(
    prompt: &str,
    history: &[Message],
    name: &str,
    arguments: &Value,
) -> Option<&'static str> {
    if !name.starts_with("workspace.") {
        return None;
    }
    if [
        "开发桥",
        "调试桥",
        "桥源码",
        "程序集分析",
        "代码审查",
        "开发接口",
    ]
    .iter()
    .any(|word| prompt.contains(word))
    {
        return None;
    }
    let bgi_task = history
        .iter()
        .rev()
        .take_while(|message| message.role != Role::User)
        .any(|message| {
            message
                .tool_calls
                .iter()
                .any(|call| call.name.starts_with("bgi."))
        });
    if !bgi_task {
        return None;
    }
    let subject = if name == "workspace.shell" {
        arguments["command"].as_str()
    } else {
        arguments["path"].as_str()
    }
    .unwrap_or("")
    .to_lowercase();
    if [
        "bridge-cache",
        "bgibridge",
        "reflection.assembly",
        "system.reflection",
        "getmethods(",
        "gettypes(",
    ]
    .iter()
    .any(|word| subject.contains(word))
    {
        Some(
            "BGI 操作禁止通过桥缓存或程序集扫描发现接口。地图追踪用仓库 pathing 父目录，直接 describe/invoke bgi.subscribe_script_resources、bgi.prepare_pathing_group、bgi.run_script_group；游戏未就绪直接 bgi.start_game/get_status。接口不可用如实报告该阻塞，不改用 PowerShell 绕过。",
        )
    } else {
        None
    }
}

/// 只约束 BGI 接口检索，通用 Agent 与其他工具保持可用。
pub async fn search_interface(
    prompt: &str,
    history: &[Message],
    arguments: &Value,
    tools: Arc<ToolRegistry>,
    bridge: &Bridge,
    cancel: &CancellationToken,
    guide: Option<&str>,
) -> Result<Value> {
    let state = RetrievalState::from_history(history);
    let query = {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("q", arguments["query"].as_str().unwrap_or(""))
            .append_pair(
                "limit",
                &arguments["limit"].as_u64().unwrap_or(8).to_string(),
            )
            .append_pair(
                "offset",
                &arguments["offset"].as_u64().unwrap_or(0).to_string(),
            );
        if let Some(group) = arguments["group"].as_str() {
            query.append_pair("group", group);
        }
        query.finish()
    };
    let mut result = bridge
        .get(&format!("/bridge/v1/catalog?{query}"), cancel)
        .await?;
    if result["total"].as_u64().unwrap_or(0) > 0 || arguments["query"] == "" {
        return Ok(result);
    }
    if state.interface_searches == 0 || state.needs_source_change() {
        let candidates = tools.call_async("bgi.repo.search", &json!({"query":prompt.chars().take(200).collect::<String>(),"category":"all","limit":8}), cancel.clone()).await;
        match candidates {
            Ok(scripts) => {
                if scripts["items"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["namedInQuery"] == true))
                {
                    attach_resource_candidates(&mut result, &scripts, guide);
                }
            }
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(_) => {}
        }
    }
    if state.search_stopped() || state.empty_searches >= 2 {
        result["retrievalAdvice"] = json!(
            "此类条件已多次未命中，核对检索对象，不扫描程序集。已找到资源后，订阅/准备/启动是新的执行阶段，可直接 describe 已知稳定入口；其他目录查询保持可用。"
        );
    }
    Ok(result)
}

fn attach_resource_candidates(result: &mut Value, resources: &Value, guide: Option<&str>) {
    let pathing = resources["items"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["category"] == "pathing" && item["namedInQuery"] == true)
    });
    let mut scripts = resources.clone();
    if let Some(items) = scripts["items"].as_array_mut() {
        items.retain(|item| item["category"] == "js");
    }
    if scripts["items"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
    {
        scripts["total"] = json!(scripts["items"].as_array().unwrap().len());
        scripts["offset"] = json!(0);
        scripts["nextOffset"] = Value::Null;
        scripts["next"] = json!("JS 候选按自己的参数定义和必要源码配置。");
        reading_guide(&mut scripts, guide);
        result["scriptCandidates"] = scripts;
    }
    result["resourceCandidates"] = resources.clone();
    result["next"] = if pathing {
        json!(
            "用户目标已命中地图追踪资源。使用 resourceCandidates 的完整父目录订阅、准备配置组并运行；不再查 JS 参数或本体功能，不需要刷新已命中的仓库。"
        )
    } else {
        resources["next"].clone()
    };
}

pub async fn search_repository(
    history: &[Message],
    arguments: &Value,
    tools: Arc<ToolRegistry>,
    cancel: &CancellationToken,
    guide: Option<&str>,
) -> Result<Value> {
    let state = RetrievalState::from_history(history);
    let mut result = tools
        .call_async("bgi.repo.search", arguments, cancel.clone())
        .await?;
    if result["items"].as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item["path"]
                .as_str()
                .is_some_and(|path| path.starts_with("js/"))
        })
    }) {
        reading_guide(&mut result, guide);
    }
    if result["total"] == 0 && state.empty_repository_searches >= 2 {
        result["retrievalAdvice"] = json!(
            "仅此分类未命中。采集路线核对 pathing/all；参数问题核对 JS 名称。不要把不同分类或新执行阶段一起熔断。"
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolCall;
    use crate::runtime::context::message;
    use serde_json::json;

    #[test]
    fn interface_fallback_preserves_pathing_instead_of_redirecting_everything_to_js() {
        let mut result = json!({"total":0});
        let resources = json!({"items":[
            {"category":"pathing","path":"pathing/地方特产/稻妻/血斛","namedInQuery":true},
            {"category":"js","path":"js/血斛统计","namedInQuery":false}
        ]});
        attach_resource_candidates(&mut result, &resources, Some("读取JS源码"));
        assert_eq!(
            result["resourceCandidates"]["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            result["scriptCandidates"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(result["next"].as_str().unwrap().contains("地图追踪"));
        assert!(result["resourceCandidates"].get("readingGuide").is_none());
        assert_eq!(
            result["scriptCandidates"]["readingGuide"]["skill"],
            "bgi-javascript"
        );
    }

    fn add(history: &mut Vec<Message>, name: &str, value: Value) {
        let id = history.len().to_string();
        let mut assistant = message(Role::Assistant, "");
        assistant.tool_calls.push(ToolCall {
            id: id.clone(),
            name: name.into(),
            arguments: json!({}),
        });
        history.push(assistant);
        let mut tool = message(Role::Tool, json!({"ok":true,"value":value}).to_string());
        tool.tool_call_id = Some(id);
        history.push(tool);
    }

    #[test]
    fn unsuccessful_queries_change_source_and_repeated_deferrals_stop_bgi_search() {
        let mut history = vec![message(Role::User, "脚本参数有什么用")];
        add(&mut history, "bgi.api.search", json!({"total":0}));
        assert!(!RetrievalState::from_history(&history).needs_source_change());
        add(&mut history, "bgi.user.list", json!({"entries":[]}));
        add(&mut history, "bgi.api.search", json!({"total":0}));
        assert!(RetrievalState::from_history(&history).needs_source_change());
        add(
            &mut history,
            "bgi.api.search",
            json!({"searchDeferred":true}),
        );
        assert!(!RetrievalState::from_history(&history).search_stopped());
        add(
            &mut history,
            "bgi.api.search",
            json!({"searchDeferred":true}),
        );
        assert!(RetrievalState::from_history(&history).search_stopped());
        add(
            &mut history,
            "bgi.repo.read",
            json!({"lines":[{"text":"参数定义"}],"truncated":false}),
        );
        assert!(!RetrievalState::from_history(&history).search_stopped());
        history.push(message(Role::User, "查询一个新的本体设置"));
        assert_eq!(RetrievalState::from_history(&history).interface_searches, 0);
    }

    #[test]
    fn parallel_queries_allow_one_round_to_follow_the_redirect() {
        let mut history = vec![message(Role::User, "脚本参数")];
        let mut assistant = message(Role::Assistant, "");
        for id in ["a", "b", "c"] {
            assistant.tool_calls.push(ToolCall {
                id: id.into(),
                name: "bgi.api.search".into(),
                arguments: json!({}),
            });
        }
        history.push(assistant);
        for id in ["a", "b", "c"] {
            let mut result = message(
                Role::Tool,
                json!({"ok":true,"value":{"searchDeferred":true}}).to_string(),
            );
            result.tool_call_id = Some(id.into());
            history.push(result);
        }
        assert_eq!(RetrievalState::from_history(&history).deferred_searches, 1);
        assert!(!RetrievalState::from_history(&history).search_stopped());
        add(
            &mut history,
            "bgi.api.search",
            json!({"searchDeferred":true}),
        );
        assert!(RetrievalState::from_history(&history).search_stopped());
    }

    #[test]
    fn unrelated_files_do_not_reset_an_identified_script_route() {
        let mut history = vec![message(Role::User, "脚本参数")];
        add(
            &mut history,
            "bgi.api.search",
            json!({"searchDeferred":true,
            "scriptCandidates":{"items":[{"path":"js/目标脚本","namedInQuery":true}]}}),
        );
        add(
            &mut history,
            "bgi.user.read",
            json!({"path":"D:/BetterGI/User/OneDragon/default.json","text":"无关配置"}),
        );
        add(
            &mut history,
            "skills.reference",
            json!({"content":"本体功能全景"}),
        );
        assert_eq!(RetrievalState::from_history(&history).deferred_searches, 1);
        assert!(RetrievalState::from_history(&history).needs_source_change());
        add(
            &mut history,
            "bgi.repo.read",
            json!({"path":"js/目标脚本/settings.json","lines":[{"text":"参数定义"}],"truncated":false}),
        );
        assert!(!RetrievalState::from_history(&history).needs_source_change());
    }

    #[test]
    fn found_pathing_resource_releases_interface_search_for_execution() {
        let mut history = vec![message(Role::User, "跑血斛")];
        add(&mut history, "bgi.api.search", json!({"total":0}));
        add(&mut history, "bgi.api.search", json!({"total":0}));
        add(
            &mut history,
            "bgi.repo.search",
            json!({"total":1,"category":"pathing","items":[{"path":"pathing/地方特产/稻妻/血斛","namedInQuery":true}]}),
        );
        assert!(!RetrievalState::from_history(&history).needs_source_change());
        assert!(!RetrievalState::from_history(&history).search_stopped());
        assert!(
            workspace_restriction(
                "跑血斛",
                &history,
                "workspace.shell",
                &json!({"command":"Select-String bridge-cache/abc/BgiBridge.dll"})
            )
            .is_some()
        );
        assert!(
            workspace_restriction("跑血斛", &history, "workspace.list", &json!({"path":""}))
                .is_none()
        );
        assert!(
            workspace_restriction(
                "调试桥源码",
                &history,
                "workspace.read",
                &json!({"path":"bridge-cache/a/BgiBridge.dll"})
            )
            .is_none()
        );
    }

    #[test]
    fn empty_repository_queries_do_not_block_the_first_interface_lookup() {
        let mut history = vec![message(Role::User, "未知名称")];
        add(&mut history, "bgi.repo.search", json!({"total":0}));
        add(&mut history, "bgi.repo.search", json!({"total":0}));
        add(
            &mut history,
            "bgi.repo.search",
            json!({"searchDeferred":true}),
        );
        let state = RetrievalState::from_history(&history);
        assert_eq!(state.empty_repository_searches, 2);
        assert!(!state.needs_source_change());
        assert!(!state.search_stopped());
        add(
            &mut history,
            "bgi.repo.search",
            json!({"searchDeferred":true}),
        );
        assert!(RetrievalState::from_history(&history).search_stopped());
    }
}
