use super::*;
use crate::config::ModelConfig;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicBool, Ordering},
    thread::JoinHandle,
};

struct ModelFixture {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ModelFixture {
    fn new(mut reply: impl FnMut(&Value) -> (u16, Value) + Send + 'static) -> Self {
        Self::with_headers(move |request| {
            let (status, body) = reply(request);
            (status, body, String::new())
        })
    }

    fn with_headers(
        mut reply: impl FnMut(&Value) -> (u16, Value, String) + Send + 'static,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stopped = Arc::new(AtomicBool::new(false));
        let recorded = requests.clone();
        let stop = stopped.clone();
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let request = read_request(&mut stream);
                recorded.lock().unwrap().push(request.clone());
                let (status, body, extra_headers) = reply(&request);
                let body = body.to_string();
                let header = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(body.as_bytes()).unwrap();
            }
        });
        Self {
            url,
            requests,
            stopped,
            thread: Some(thread),
        }
    }
}

#[tokio::test]
async fn long_retry_after_does_not_replay_http_request() {
    let fixture = ModelFixture::with_headers(|_| {
        (
            429,
            json!({"error":{"message":"rate limited"}}),
            "Retry-After: 120\r\n".into(),
        )
    });
    let config: ModelConfig = serde_json::from_value(json!({"id":"fixture","name":"fixture",
        "protocol":"openai-chat","model":"glm-5.3-flash","baseUrl":fixture.url}))
    .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        gateway::complete(
            config,
            &[message(Role::User, "fixture")],
            &[],
            &CancellationToken::new(),
            |_| Ok(()),
        ),
    )
    .await
    .unwrap();
    assert!(result.unwrap_err().to_string().contains("120 秒"));
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn glm_http_tool_round_replays_original_reasoning() {
    let original = "\n核对现场状态。 \n";
    let fixture = ModelFixture::new(move |request| {
        if request["messages"].as_array().unwrap().len() == 1 {
            let mut body = tool_calls(vec![("workspace.environment", json!({}))]);
            body["choices"][0]["message"]["reasoning_content"] = json!(original);
            (200, body)
        } else {
            (200, answer("done"))
        }
    });
    let mut config: ModelConfig = serde_json::from_value(json!({"id":"fixture","name":"fixture",
        "protocol":"openai-chat","model":"glm-5.3-flash","baseUrl":fixture.url}))
    .unwrap();
    config.options.reasoning_effort = Some("low".into());
    let cancel = CancellationToken::new();
    let mut history = vec![message(Role::User, "fixture")];
    let first = gateway::complete(config.clone(), &history, &[], &cancel, |_| Ok(()))
        .await
        .unwrap();
    let assistant = Message {
        role: Role::Assistant,
        content: first.text,
        tool_call_id: None,
        tool_calls: first.tool_calls,
        reasoning: first.reasoning,
    };
    let assistant: Message =
        serde_json::from_str(&serde_json::to_string(&assistant).unwrap()).unwrap();
    let call_id = assistant.tool_calls[0].id.clone();
    history.push(assistant);
    history.push(Message {
        role: Role::Tool,
        content: "{}".into(),
        tool_call_id: Some(call_id),
        tool_calls: vec![],
        reasoning: None,
    });
    let second = gateway::complete(config, &history, &[], &cancel, |_| Ok(()))
        .await
        .unwrap();
    assert_eq!(second.text, "done");
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1]["messages"][1]["reasoning_content"], original);
    assert_eq!(requests[1]["reasoning_effort"], "low");
}

impl Drop for ModelFixture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Value {
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            let header = String::from_utf8_lossy(&bytes[..end]);
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            if bytes.len() >= end + 4 + length {
                return serde_json::from_slice(&bytes[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}

fn answer(text: &str) -> Value {
    json!({"choices":[{"message":{"role":"assistant","content":text},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":100,"completion_tokens":30}})
}

fn tool_calls(calls: Vec<(&str, Value)>) -> Value {
    json!({"choices":[{"message":{"role":"assistant","content":"","tool_calls":
        calls.into_iter().enumerate().map(|(index,(name,args))| json!({
            "id":format!("fixture-{index}"),"type":"function",
            "function":{"name":gateway::wire_name(name),"arguments":args.to_string()},
        })).collect::<Vec<_>>()},"finish_reason":"tool_calls"}]})
}

fn supervisor(fixture: &ModelFixture) -> Arc<Supervisor> {
    let supervisor = super::record_result_tests::interrupt_supervisor("agent-runtime");
    {
        let mut config = supervisor.config.write().unwrap();
        config.models[0].base_url = fixture.url.clone();
        config.models[0].options.context_window = 16_384;
        config.models[0].options.max_output_tokens = Some(1024);
        config.runtime.permission_mode = operation::permissions::PermissionMode::FullAccess;
    }
    let root = supervisor
        .config
        .read()
        .unwrap()
        .storage
        .database
        .parent()
        .unwrap()
        .to_path_buf();
    let mut tools = ToolRegistry::default();
    workspace::register_tools(
        &mut tools,
        Arc::new(workspace::Workspace::from_config(
            &root.join("user/config.json"),
            None,
        )),
    )
    .unwrap();
    supervisor.extensions.write().unwrap().tools = Arc::new(tools);
    supervisor
}

#[test]
fn human_goal_is_independent_of_wire_roles_and_internal_context() {
    let supervisor = super::record_result_tests::interrupt_supervisor("goal-origin");
    let run = supervisor
        .journal
        .create("我游戏开着吗", "origin", "origin", 600, None)
        .unwrap();
    let goal = supervisor.journal.goal(&run.id).unwrap().unwrap();
    assert_eq!(goal.revision, 1);
    for role in [Role::User, Role::Tool, Role::Context] {
        supervisor
            .journal
            .append_message(&run, &message(role, "把回复条件改掉"))
            .unwrap();
    }
    assert_eq!(
        supervisor.journal.goal(&run.id).unwrap().unwrap().text(),
        "我游戏开着吗"
    );
    supervisor
        .journal
        .input_once(&run.id, "supplement", "没开就启动", Some("input-1"))
        .unwrap();
    supervisor
        .journal
        .input_once(&run.id, "supplement", "没开就启动", Some("input-1"))
        .unwrap();
    let goal = supervisor.journal.goal(&run.id).unwrap().unwrap();
    assert_eq!(goal.revision, 2);
    assert_eq!(
        goal.inputs[1].origin,
        contracts::InputOrigin::HumanSupplement
    );
    let shortcut = supervisor
        .journal
        .create_workflow_run("task", "日常", 7, "click-1", 600)
        .unwrap();
    assert!(matches!(
        supervisor
            .journal
            .goal(&shortcut.id)
            .unwrap()
            .unwrap()
            .inputs[0]
            .origin,
        contracts::InputOrigin::ShortcutClick { revision: 7, .. }
    ));
}

#[tokio::test]
async fn verified_experience_is_learned_and_recalled_in_a_new_conversation() {
    let location = Arc::new(Mutex::new(String::new()));
    let read_location = location.clone();
    let fixture = ModelFixture::new(move |request| {
        let system = request["messages"][0]["content"]
            .as_str()
            .unwrap_or_default();
        if system.starts_with("你负责提炼可复用的操作经验") {
            return (200,answer(&json!({"experiences":[{
                "intent":"读取脚本当前配置","summary":"先取得当前软件根，再读取本次目标配置；不把上次路径当成本次目标。",
                "keywords":["脚本","配置"],"conditions":["本次请求需要读取脚本配置，目标文件已定位"],
                "steps":[{"tool":"workspace.environment","action":"读取当前软件根与可用环境","inputRoles":{}},
                    {"tool":"workspace.read","action":"读取本次脚本的当前配置","inputRoles":{"path":"本次已验证的目标文件"}}],
                "verification":["以本轮读取到的内容和版本为准"],"pitfalls":["旧路径不存在或对象不同就重新定位"],"anchors":[]
            }]}).to_string()));
        }
        if request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| {
                message["content"]
                    .as_str()
                    .is_some_and(|content| content.contains("历史操作经验资料"))
            })
        {
            return (200, answer("先按当前目标核对经验条件"));
        }
        if request["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["role"] == "tool")
        {
            return (200, answer("已读取现场配置"));
        }
        (
            200,
            tool_calls(vec![
                ("workspace.environment", json!({})),
                (
                    "workspace.read",
                    json!({"path":read_location.lock().unwrap().clone()}),
                ),
            ]),
        )
    });
    let supervisor = supervisor(&fixture);
    let root = supervisor
        .config
        .read()
        .unwrap()
        .storage
        .database
        .parent()
        .unwrap()
        .to_path_buf();
    let path = root.join("script-config.json");
    std::fs::write(&path, "{\"enabled\":true}").unwrap();
    *location.lock().unwrap() = path.to_string_lossy().into_owned();
    let cancel = CancellationToken::new();
    let mut first = supervisor
        .journal
        .create("读取脚本配置", "first", "first", 600, None)
        .unwrap();
    supervisor.session(&mut first, &cancel).await.unwrap();
    assert_eq!(first.state, RunState::Answered);
    let learned = supervisor
        .learn_experience(&first.id, &cancel, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(learned.0, 1);
    let mut second = supervisor
        .journal
        .create("读取另一个脚本配置", "second", "second", 600, None)
        .unwrap();
    supervisor.session(&mut second, &cancel).await.unwrap();
    let history = supervisor.journal.run_history(&second).unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|message| message.role == Role::User)
            .count(),
        1
    );
    assert!(history.iter().any(
        |message| message.role == Role::Context && message.content.contains("读取脚本当前配置")
    ));
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(
        requests[0]["messages"][0],
        requests.last().unwrap()["messages"][0]
    );
    assert!(
        !requests
            .last()
            .unwrap()
            .to_string()
            .contains(path.to_string_lossy().as_ref())
    );
    drop(requests);
    supervisor.shutdown();
}

#[test]
fn observed_write_route_is_saved_before_background_refinement() {
    let fixture = ModelFixture::new(|_| (200, answer("unused")));
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .journal
        .create(
            "修改脚本配置",
            "trace-conversation",
            "trace-client",
            600,
            None,
        )
        .unwrap();
    supervisor
        .journal
        .save(&mut run, RunState::Deciding)
        .unwrap();
    let mut assistant = message(Role::Assistant, "");
    assistant.tool_calls = vec![
        ToolCall {
            id: "read".into(),
            name: "workspace.read".into(),
            arguments: json!({"path":"current-config.json"}),
        },
        ToolCall {
            id: "write".into(),
            name: "workspace.write".into(),
            arguments: json!({"path":"current-config.json","expectedHash":"old-value-not-for-reuse","text":"new config"}),
        },
    ];
    supervisor.journal.append_message(&run, &assistant).unwrap();
    for (id, value) in [
        (
            "read",
            json!({"content":"old config","version":"old-value-not-for-reuse"}),
        ),
        ("write", json!({"written":true})),
    ] {
        let mut result = message(Role::Tool, json!({"ok":true,"value":value}).to_string());
        result.tool_call_id = Some(id.into());
        supervisor.journal.append_message(&run, &result).unwrap();
    }
    supervisor
        .journal
        .save(&mut run, RunState::Answered)
        .unwrap();
    assert_eq!(supervisor.remember_observed_route(&run).unwrap(), 1);
    let memories = supervisor.recall_memories("修改另一个脚本配置").unwrap();
    assert_eq!(memories.len(), 1);
    assert!(memories[0].id.starts_with("trace:"));
    assert!(
        !serde_json::to_string(&memories[0].experience)
            .unwrap()
            .contains("old-value-not-for-reuse")
    );
    supervisor.shutdown();
}

#[test]
fn initial_builtin_tool_set_is_twenty_and_expands_with_provider_evidence() {
    use crate::extension::{FunctionTool, ToolExposure};
    let fixture = ModelFixture::new(|_| (200, answer("unused")));
    let supervisor = supervisor(&fixture);
    let mut registry = ToolRegistry::default();
    workspace::register_tools(
        &mut registry,
        Arc::new(workspace::Workspace::from_config(
            &crate::test_root("tools").join("user/config.json"),
            None,
        )),
    )
    .unwrap();
    crate::bridge::register_tools(
        &mut registry,
        Arc::new(crate::bridge::BgiClient::new(
            supervisor.config.read().unwrap().bridge.clone(),
        )),
    )
    .unwrap();
    for name in ["skills.search", "skills.read", "plugins.list"] {
        registry
            .register(
                FunctionTool::new(
                    name,
                    "read",
                    json!({"type":"object"}),
                    "core:skills",
                    |_| Ok(json!({})),
                )
                .with_execution(ToolExecution {
                    always_load: true,
                    deferred: false,
                    exposure: ToolExposure::Always,
                    ..ToolExecution::read_only()
                }),
            )
            .unwrap();
    }
    supervisor.extensions.write().unwrap().tools = Arc::new(registry);
    let definitions = supervisor.definitions(&HashSet::new());
    assert_eq!(definitions.len(), 44);
    let policy = tool_policy::TurnTools::new(
        &definitions,
        &[],
        &HashSet::new(),
        operation::permissions::PermissionMode::AskEach,
        false,
    );
    assert_eq!(policy.select(&definitions).len(), 20);
    assert!(
        !policy
            .select(&definitions)
            .iter()
            .any(|tool| tool.name.starts_with("memory."))
    );
    let alias = definitions
        .iter()
        .find(|tool| tool.name == "task.save")
        .unwrap();
    assert_eq!(alias.execution.effect, ToolEffect::LocalWrite);
    let readonly = tool_policy::TurnTools::new(
        &definitions,
        &[],
        &HashSet::from(["task.save".into()]),
        operation::permissions::PermissionMode::PlanOnly,
        false,
    );
    assert!(!readonly.allows(alias));
    let mut call = message(Role::Assistant, "");
    call.tool_calls.push(ToolCall {
        id: "read".into(),
        name: "bgi.user.read".into(),
        arguments: json!({}),
    });
    let mut reply = message(
        Role::Tool,
        json!({"ok":true,"value":{"path":"group","sha256":"version"}}).to_string(),
    );
    reply.tool_call_id = Some("read".into());
    let policy = tool_policy::TurnTools::new(
        &definitions,
        &[call, reply],
        &HashSet::new(),
        operation::permissions::PermissionMode::AskEach,
        false,
    );
    let offered = policy.select(&definitions);
    assert!(offered.iter().any(|tool| tool.name == "bgi.api.invoke"));
    assert!(!offered.iter().any(|tool| tool.name == "workspace.write"));
    assert!(!offered.iter().any(|tool| tool.name == "bgi.job.get"));
}

#[tokio::test]
async fn discovered_builtin_shell_becomes_callable_without_loading_a_plugin() {
    let mut turn = 0;
    let fixture = ModelFixture::new(move |_| {
        turn += 1;
        (
            200,
            match turn {
                1 => tool_calls(vec![("tools.search", json!({"query":"workspace.shell"}))]),
                2 => tool_calls(vec![(
                    "workspace.shell",
                    json!({"command":"Write-Output POLICY_SHELL","timeoutMs":10000}),
                )]),
                _ => answer("done"),
            },
        )
    });
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .submit("执行指定命令", None, "shell", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert!(
        !requests[0]["tools"]
            .to_string()
            .contains(&gateway::wire_name("workspace.shell"))
    );
    assert!(
        requests[1]["tools"]
            .to_string()
            .contains(&gateway::wire_name("workspace.shell"))
    );
    let result = requests[2]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|message| message["role"] == "tool")
        .unwrap();
    let result: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
    assert_eq!(result["ok"], true);
    assert!(result["value"].to_string().contains("POLICY_SHELL"));
}

#[tokio::test]
async fn large_read_keeps_version_evidence_and_artifact_pages_do_not_rewrap() {
    let root = crate::test_root("artifact-page");
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("large.txt");
    std::fs::write(&file, "x".repeat(50_000)).unwrap();
    let path = file.to_string_lossy().into_owned();
    let mut turn = 0;
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        (
            200,
            match turn {
                1 => tool_calls(vec![(
                    "workspace.read",
                    json!({"path":path,"maxChars":50000}),
                )]),
                2 => {
                    let message = request["messages"].as_array().unwrap().last().unwrap();
                    let result: Value =
                        serde_json::from_str(message["content"].as_str().unwrap()).unwrap();
                    assert_eq!(result["ok"], true);
                    assert_eq!(result["value"]["sha256"].as_str().unwrap().len(), 64);
                    assert_eq!(result["truncated"], true);
                    tool_calls(vec![(
                        "artifact.read",
                        json!({"id":result["artifactId"],"offset":4000,"maxChars":1000}),
                    )])
                }
                _ => answer("done"),
            },
        )
    });
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .submit("读取大文件", None, "artifact", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    let result = requests[2]["messages"].as_array().unwrap().last().unwrap();
    let result: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
    assert!(result["artifactId"].is_null());
    assert_eq!(result["value"]["nextOffset"], 5000);
    assert_eq!(
        result["value"]["content"].as_str().unwrap().chars().count(),
        1000
    );
}

#[tokio::test]
async fn compaction_preserves_current_user_authority_after_summarizing_its_message() {
    let fixture = ModelFixture::new(|request| {
        (
            200,
            if request["messages"][0]["content"] == protected_compaction_prompt() {
                answer("压缩后的事实摘要")
            } else {
                answer("done")
            },
        )
    });
    let supervisor = supervisor(&fixture);
    let goal = "当前用户授权原文，不能用摘要替代";
    let mut run = supervisor
        .submit(goal, None, "current-goal", Some(30), None)
        .unwrap();
    let mut call = message(Role::Assistant, "");
    call.tool_calls.push(ToolCall {
        id: "large".into(),
        name: "workspace.write".into(),
        arguments: json!({}),
    });
    supervisor.journal.append_message(&run, &call).unwrap();
    let mut reply = message(
        Role::Tool,
        json!({"ok":true,"value":{"proof":"证据".repeat(20_000)}}).to_string(),
    );
    reply.tool_call_id = Some("large".into());
    supervisor.journal.append_message(&run, &reply).unwrap();
    supervisor
        .journal
        .append_message(
            &run,
            &message(Role::User, "从截断处继续，不重复已经输出的内容"),
        )
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert!(
        requests.last().unwrap()["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["role"] == "user" && message["content"] == goal)
    );
    assert!(run.context_compacted);
}

#[tokio::test]
async fn parallel_limit_pairs_every_requested_tool() {
    let root = crate::test_root("parallel-pairing");
    std::fs::create_dir_all(&root).unwrap();
    let files = (0..3)
        .map(|index| {
            let path = root.join(format!("{index}.txt"));
            std::fs::write(&path, format!("VALUE-{index}")).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect::<Vec<_>>();
    let mut turn = 0;
    let fixture = ModelFixture::new(move |_| {
        turn += 1;
        (
            200,
            if turn == 1 {
                tool_calls(
                    files
                        .iter()
                        .map(|path| ("workspace.read", json!({"path":path})))
                        .collect(),
                )
            } else {
                answer("done")
            },
        )
    });
    let supervisor = supervisor(&fixture);
    supervisor
        .config
        .write()
        .unwrap()
        .agent
        .max_tool_calls_per_turn = 2;
    let mut run = supervisor
        .submit("读取三个文件", None, "pairing", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let requests = fixture.requests.lock().unwrap();
    let results = requests[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "tool")
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 3);
    assert!(results[0]["content"].as_str().unwrap().contains("VALUE-0"));
    assert!(results[1]["content"].as_str().unwrap().contains("VALUE-1"));
    assert!(results[2]["content"].as_str().unwrap().contains("未执行"));
    assert_eq!(run.tool_calls, 2);
}

#[tokio::test]
async fn execution_policy_blocks_unread_write_then_allows_read_modify_verify() {
    let root = crate::test_root("read-modify-verify");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("settings.txt");
    std::fs::write(&path, "before").unwrap();
    let file = path.to_string_lossy().into_owned();
    let mut turn = 0;
    let fixture = ModelFixture::new(move |request| {
        turn += 1;
        let body = match turn {
            1 => {
                assert!(
                    !request["tools"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|tool| tool["function"]["name"]
                            == gateway::wire_name("workspace.write"))
                );
                tool_calls(vec![(
                    "workspace.write",
                    json!({"path":file,"content":"wrong"}),
                )])
            }
            2 => {
                let result = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap();
                let result: Value =
                    serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
                assert_eq!(result["ok"], false);
                tool_calls(vec![("workspace.read", json!({"path":file}))])
            }
            3 => {
                let result = request["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .rev()
                    .find(|message| message["role"] == "tool")
                    .unwrap();
                let result: Value =
                    serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
                assert_eq!(result["value"]["text"], "before");
                tool_calls(vec![(
                    "workspace.write",
                    json!({"path":file,"content":"after",
                    "expectedSha256":result["value"]["sha256"]}),
                )])
            }
            4 => tool_calls(vec![("workspace.read", json!({"path":file}))]),
            _ => answer("verified"),
        };
        (200, body)
    });
    let supervisor = supervisor(&fixture);
    let mut run = supervisor
        .submit("修改设置并核验", None, "modify", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "after");
    let requests = fixture.requests.lock().unwrap();
    assert!(requests[4]["messages"].to_string().contains("after"));
}

#[tokio::test]
async fn oversized_tool_history_is_compacted_without_losing_input_or_pairing() {
    let mut rejected_summary = false;
    let fixture =
        ModelFixture::new(move |request| {
            let summarizing = request["messages"][0]["content"] == protected_compaction_prompt();
            if summarizing {
                assert!(request["tools"].as_array().unwrap().is_empty());
                assert!(request["messages"].as_array().unwrap().iter().all(|message|
                message["role"] != "tool" && message["tool_calls"].is_null()));
                let data = request["messages"][2]["content"].as_str().unwrap();
                if data.chars().count() > 2000 && !rejected_summary {
                    rejected_summary = true;
                    return (400, json!({"error":{"message":"context length exceeded"}}));
                }
                let previous = request["messages"][1]["content"].as_str().unwrap();
                let mut summary = "目标：保留原始操作事实。已确认：".to_owned();
                for marker in ["FIRST_MARKER", "LAST_MARKER"] {
                    if data.contains(marker) || previous.contains(marker) {
                        summary.push_str(marker);
                    }
                }
                return (200, answer(&summary));
            }
            (200, answer("continued"))
        });
    let supervisor = supervisor(&fixture);
    let mut old = supervisor
        .submit("旧目标", Some("compact"), "old", Some(30), None)
        .unwrap();
    let mut call = message(Role::Assistant, "");
    call.tool_calls.push(ToolCall {
        id: "old-write".into(),
        name: "workspace.write".into(),
        arguments: json!({"path":"old"}),
    });
    supervisor.journal.append_message(&old, &call).unwrap();
    let mut result = message(
        Role::Tool,
        json!({"ok":true,"value":{
            "evidence":format!("FIRST_MARKER{}LAST_MARKER", "旧证据".repeat(12_000))
        }})
        .to_string(),
    );
    result.tool_call_id = Some("old-write".into());
    supervisor.journal.append_message(&old, &result).unwrap();
    supervisor
        .journal
        .save(&mut old, RunState::Succeeded)
        .unwrap();
    let mut run = supervisor
        .submit(
            "当前目标原文必须保留",
            Some("compact"),
            "new",
            Some(30),
            None,
        )
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.result.as_deref(), Some("continued"));
    let requests = fixture.requests.lock().unwrap();
    assert!(requests.len() > 3);
    let final_request = requests.last().unwrap();
    assert!(
        final_request["messages"]
            .to_string()
            .contains("当前目标原文必须保留")
    );
    assert!(
        final_request["messages"]
            .to_string()
            .contains("FIRST_MARKER")
    );
    assert!(
        final_request["messages"]
            .to_string()
            .contains("LAST_MARKER")
    );
    assert!(
        supervisor
            .journal
            .conversation_messages("compact")
            .unwrap()
            .iter()
            .any(|message| message.message.content.contains("LAST_MARKER"))
    );
}

#[tokio::test]
async fn server_context_overflow_forces_summary_even_when_estimate_fits() {
    let mut rejected = false;
    let fixture = ModelFixture::new(move |request| {
        if request["messages"][0]["content"] == protected_compaction_prompt() {
            return (200, answer("已确认旧目标与旧结果；继续当前目标。"));
        }
        if !rejected {
            rejected = true;
            return (400, json!({"error":{"message":"context length exceeded"}}));
        }
        (200, answer("recovered"))
    });
    let supervisor = supervisor(&fixture);
    let mut old = supervisor
        .submit("旧目标", Some("overflow"), "old", Some(30), None)
        .unwrap();
    supervisor
        .journal
        .append_message(&old, &message(Role::Assistant, "旧结果"))
        .unwrap();
    supervisor
        .journal
        .save(&mut old, RunState::Answered)
        .unwrap();
    let mut run = supervisor
        .submit("当前目标", Some("overflow"), "new", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(run.result.as_deref(), Some("recovered"));
    assert!(run.context_compacted);
    assert!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request["messages"][0]["content"] == protected_compaction_prompt())
    );
}

#[tokio::test]
async fn truncated_summary_is_retried_and_never_committed_as_complete() {
    let mut rejected = false;
    let mut summary_attempts = 0;
    let fixture = ModelFixture::new(move |request| {
        if request["messages"][0]["content"] == protected_compaction_prompt() {
            summary_attempts += 1;
            if summary_attempts == 1 {
                let mut response = answer("PARTIAL_SUMMARY");
                response["choices"][0]["finish_reason"] = json!("length");
                return (200, response);
            }
            return (200, answer("COMPLETE_SUMMARY"));
        }
        if !rejected {
            rejected = true;
            return (400, json!({"error":{"message":"context_window_exceeded"}}));
        }
        (200, answer("continued"))
    });
    let supervisor = supervisor(&fixture);
    let mut old = supervisor
        .submit("old", Some("truncated"), "old", Some(30), None)
        .unwrap();
    supervisor
        .journal
        .append_message(&old, &message(Role::Assistant, "old answer"))
        .unwrap();
    supervisor
        .journal
        .save(&mut old, RunState::Answered)
        .unwrap();
    let mut run = supervisor
        .submit("new", Some("truncated"), "new", Some(30), None)
        .unwrap();
    supervisor
        .session(&mut run, &CancellationToken::new())
        .await
        .unwrap();
    let (summary, _) = supervisor.journal.history_window(&run).unwrap();
    let summary = summary.unwrap().content;
    assert!(summary.contains("COMPLETE_SUMMARY"));
    assert!(!summary.contains("PARTIAL_SUMMARY"));
    let event = supervisor
        .journal
        .events(&run.conversation_id, 0)
        .unwrap()
        .into_iter()
        .find(|event| event.kind == "context.compacted" && event.data["mode"] == "summary")
        .unwrap();
    assert_eq!(event.data["outputTokens"], 60);
    assert_eq!(run.result.as_deref(), Some("continued"));
}
