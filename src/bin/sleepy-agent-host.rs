//! 无界面通道宿主。使用正式 Supervisor/Journal/工具内核，不包含第二套 Agent 循环。
use serde_json::{Value, json};
use sleepy_doll::{
    AppConfig, ModelConfig,
    error::{Error, Result},
    extension::skills::{EMBEDDED_SKILLS, SkillRegistry},
    extension::{
        ConcurrencyMode, FunctionTool, Tool, ToolDefinition, ToolEffect, ToolExecution,
        ToolRegistry,
    },
    model::Message,
    runtime::{
        Supervisor, gateway,
        operation::operations::{OperationEngine, OperationStore},
        store::artifacts::ArtifactStore,
        types::{RunState, unix_now},
    },
};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

type Reply = std::result::Result<Value, String>;
struct Wire {
    output: Mutex<std::io::Stdout>,
    closed: AtomicBool,
    pending: Mutex<HashMap<String, oneshot::Sender<Reply>>>,
}
struct CallbackLease<'a> {
    wire: &'a Wire,
    id: &'a str,
    finished: bool,
}
impl Drop for CallbackLease<'_> {
    fn drop(&mut self) {
        self.wire.pending.lock().unwrap().remove(self.id);
        if !self.finished {
            self.wire.emit(json!({"op":"tool_cancel","id":self.id}));
        }
    }
}
impl Wire {
    fn emit(&self, value: Value) {
        let mut output = self.output.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let _ = writeln!(output, "{value}");
        let _ = output.flush();
    }
    fn close(&self) {
        // 等正在写出的整条JSON完成，再阻止后台事件开启下一次写入。
        // main返回会结束进程，不能把半条事件留在协议管道中。
        let mut output = self.output.lock().unwrap();
        self.closed.store(true, Ordering::Release);
        let _ = output.flush();
    }
    async fn callback(
        &self,
        request: String,
        name: String,
        args: Value,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().unwrap().insert(id.clone(), sender);
        let mut lease = CallbackLease {
            wire: self,
            id: &id,
            finished: false,
        };
        self.emit(json!({"op":"tool","id":id,"request":request,"name":name,"arguments":args}));
        tokio::select! {
            _ = cancel.cancelled() => Err(Error::Cancelled),
            value = receiver => match value {
                Ok(Ok(value)) => { lease.finished = true; Ok(value) },
                Ok(Err(error)) => { lease.finished = true; Err(Error::Tool(error)) },
                Err(_) => Err(Error::Tool("通道工具回执连接已关闭".into())),
            }
        }
    }
}

struct CallbackTool {
    definition: ToolDefinition,
    wire: Arc<Wire>,
    request: String,
    priority: i32,
    completion: Value,
    delivery: Value,
    presentation: Option<String>,
}
impl Tool for CallbackTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }
    fn call(&self, _: &Value) -> Result<Value> {
        Err(Error::Tool("通道工具必须异步调用".into()))
    }
    fn call_async(
        self: Arc<Self>,
        args: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            self.wire
                .callback(
                    self.request.clone(),
                    self.definition.name.clone(),
                    args,
                    cancel,
                )
                .await
        })
    }
    fn response_priority(&self) -> i32 {
        self.priority
    }
    fn presentation(&self, arguments: &Value) -> Option<Value> {
        if arguments["preserve_text"] == true || arguments["user_quote"].is_string() {
            return None;
        }
        arguments.pointer(self.presentation.as_deref()?).cloned()
    }
    fn presents_content(&self) -> bool {
        self.presentation.is_some()
    }
    fn presentation_context(&self, arguments: &Value) -> Option<Value> {
        let pointer = self.presentation.as_deref()?;
        let (parent, field) = pointer.rsplit_once('/')?;
        let mut context = arguments.clone();
        let container = context.pointer_mut(parent)?;
        let field = field.replace("~1", "/").replace("~0", "~");
        match container {
            Value::Object(fields) => {
                fields.remove(&field);
            }
            Value::Array(items) => {
                *items.get_mut(field.parse::<usize>().ok()?)? = Value::Null;
            }
            _ => return None,
        }
        Some(context)
    }
    fn turn_completion(&self, value: &Value) -> Option<String> {
        let pointer = self.completion["pointer"].as_str()?;
        let observed = value.pointer(pointer)?;
        if self.completion["exists"] != true && observed != &self.completion["equals"] {
            return None;
        }
        Some(
            self.completion["messagePointer"]
                .as_str()
                .and_then(|path| value.pointer(path))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        )
    }
    fn has_delivery_receipt(&self, value: &Value) -> bool {
        let Some(pointer) = self.delivery["pointer"].as_str() else {
            return false;
        };
        let Some(observed) = value.pointer(pointer) else {
            return false;
        };
        self.delivery["exists"] == true || observed == &self.delivery["equals"]
    }
}

struct NativeTool {
    inner: Arc<dyn Tool>,
    roots: Vec<PathBuf>,
    protected: Vec<PathBuf>,
}
impl NativeTool {
    fn check(&self, args: &Value) -> Result<()> {
        for key in ["path", "cwd"] {
            if let Some(value) = args[key].as_str() {
                self.path(value)?;
            }
        }
        for value in args["paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            self.path(value)?;
        }
        Ok(())
    }
    fn path(&self, value: &str) -> Result<()> {
        let raw = Path::new(value);
        let path = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            self.roots[0].join(raw)
        };
        let canonical = path
            .canonicalize()
            .map_err(|_| Error::Tool("文件不存在或不在已授权的资料目录".into()))?;
        if self
            .protected
            .iter()
            .any(|protected| protected == &canonical || protected.starts_with(&canonical))
        {
            return Err(Error::Tool(
                "审批凭据与内部配置不能通过文件或目录检索读取".into(),
            ));
        }
        if self.roots.iter().any(|root| canonical.starts_with(root)) {
            Ok(())
        } else {
            Err(Error::Tool(
                "群聊文件读取只允许已配置的资料目录和本会话工作区".into(),
            ))
        }
    }
}
impl Tool for NativeTool {
    fn definition(&self) -> ToolDefinition {
        self.inner.definition()
    }
    fn call(&self, args: &Value) -> Result<Value> {
        self.check(args)?;
        self.inner.call(args)
    }
    fn call_async(
        self: Arc<Self>,
        args: Value,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>> {
        Box::pin(async move {
            self.check(&args)?;
            let mut value = self.inner.clone().call_async(args, cancel).await?;
            if self.definition().name == "workspace.environment" {
                value["knowledgeRoots"] = json!(self.roots.iter().skip(1).collect::<Vec<_>>());
                value.as_object_mut().map(|v| v.remove("userDirectory"));
            }
            Ok(value)
        })
    }
}

struct Channel {
    supervisor: Arc<Supervisor>,
    current: Arc<RwLock<String>>,
    active_run: Mutex<Option<String>>,
    workspace: PathBuf,
}
struct Host {
    wire: Arc<Wire>,
    channels: Mutex<HashMap<String, Arc<Channel>>>,
    completes: Mutex<HashMap<String, CancellationToken>>,
}
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

fn registry(input: &Value, wire: Arc<Wire>, workspace: &Path) -> Result<Arc<ToolRegistry>> {
    let task_timeout_ms = (config(input, workspace.parent().unwrap_or(workspace))?
        .runtime
        .duration_sec
        .max(0) as u64)
        .saturating_mul(1000);
    let mut tools = ToolRegistry::default();
    let mut native = ToolRegistry::default();
    sleepy_doll::runtime::workspace::register_tools(
        &mut native,
        Arc::new(sleepy_doll::runtime::workspace::Workspace::from_config(
            &workspace.join("config.json"),
            None,
        )),
    )?;
    let mut roots = vec![workspace.canonicalize()?];
    let protected = input["protectedPaths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|value| Path::new(value).canonicalize().ok())
        .collect::<Vec<_>>();
    for value in input["readRoots"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if let Ok(path) = Path::new(value).canonicalize() {
            roots.push(path);
        }
    }
    for definition in native.definitions() {
        // 无界面Agent进程只读，不提供本机写入/命令入口，旧信任标记无效。
        if definition.execution.effect != ToolEffect::ReadOnly {
            continue;
        }
        if let Some(inner) = native.tool(&definition.name) {
            tools.register(NativeTool {
                inner,
                roots: roots.clone(),
                protected: protected.clone(),
            })?;
        }
    }
    tools.register(
        FunctionTool::new(
            "plugins.list",
            "读取本通道接入的插件",
            json!({"type":"object","properties":{}}),
            "core:plugins",
            |_| Ok(json!({"plugins":[{"id":"qq","enabled":true}],"bridgeEnabled":false})),
        )
        .with_execution(ToolExecution {
            deferred: false,
            always_load: true,
            ..ToolExecution::read_only()
        }),
    )?;
    for value in input["tools"].as_array().into_iter().flatten() {
        let function = &value["function"];
        let name = function["name"]
            .as_str()
            .ok_or_else(|| Error::Config("通道工具缺少名称".into()))?;
        if name.starts_with("workspace.") {
            return Err(Error::Config("通道工具不能覆盖原生工作区命名空间".into()));
        }
        let read = value["host"]["readOnly"].as_bool().unwrap_or(false);
        let mut execution = ToolExecution {
            prompt_visible: value["host"]["promptVisible"].as_bool().unwrap_or(false),
            pagination: value["host"]
                .get("pagination")
                .filter(|v| !v.is_null())
                .cloned()
                .map(serde_json::from_value)
                .transpose()?,
            search_provider: read && value["host"]["searchProvider"].as_bool().unwrap_or(false),
            deferred: false,
            always_load: true,
            timeout_ms: value["host"]["timeoutMs"]
                .as_u64()
                .unwrap_or(task_timeout_ms),
            max_result_chars: 24_000,
            effect: if read {
                ToolEffect::ReadOnly
            } else {
                ToolEffect::ExternalWrite
            },
            ..if read {
                ToolExecution::read_only()
            } else {
                ToolExecution::default()
            }
        };
        if !read && value["host"]["leaseScope"] == "request" {
            let key = input["clientKey"]
                .as_str()
                .filter(|key| !key.trim().is_empty())
                .ok_or_else(|| Error::Config("按请求隔离的宿主工具缺少稳定请求标识".into()))?;
            // 同一用户请求的未知结果仍保留锁，独立请求不占用这个资源。
            execution.concurrency = ConcurrencyMode::ResourceExclusive;
            execution.lease_scope = Some(format!("request:{key}"));
        }
        tools.register(CallbackTool {
            priority: value["host"]["priority"].as_i64().unwrap_or(0) as i32,
            completion: value["host"]["completion"].clone(),
            delivery: value["host"]["finalReply"].clone(),
            presentation: value["host"]["presentation"].as_str().map(str::to_owned),
            definition: ToolDefinition {
                name: name.into(),
                label: name.into(),
                description: function["description"].as_str().unwrap_or("").into(),
                input_schema: function["parameters"].clone(),
                output_schema: None,
                source: "plugin:qq".into(),
                provider_version: Some("qq-channel-v1".into()),
                execution,
            },
            wire: wire.clone(),
            request: input["id"].as_str().unwrap_or_default().to_owned(),
        })?;
    }
    Ok(Arc::new(tools))
}

fn config(input: &Value, dir: &Path) -> Result<AppConfig> {
    Ok(serde_json::from_value(
        json!({"version":2,"activeModel":"qq","models":[input["model"].clone()],
        "agent":{"systemPrompt":input["instructions"].as_str().unwrap_or(""),"skillDirectories":[]},
        "bridge":{"enabled":false,"autoStart":false,"baseUrl":"http://127.0.0.1:1"},
        "plugins":{"enabled":["qq"],"disabled":["bgi"]},"storage":{"database":dir.join("journal.sqlite")},
        "runtime":{"catalogDirectory":dir,"permissionMode":"fullAccess","memoryEnabled":input["memoryEnabled"].as_bool().unwrap_or(true),"maxDecisions":input["maxDecisions"].as_u64().unwrap_or(0),"durationSec":input["durationSec"].as_i64().unwrap_or(900),"contextChars":input.get("contextChars").and_then(|v|v.as_u64()),"deliveryReview":input.get("deliveryReview").and_then(Value::as_str),"deliveryReviewContextual":input.get("deliveryReviewContextual").and_then(Value::as_bool),"deliveryTool":input.get("deliveryTool").and_then(Value::as_str),"responsePreference":input.get("responsePreference").and_then(Value::as_str),"standaloneInstructions":input["standaloneInstructions"].as_bool().unwrap_or(false),"reasoningEfforts":input.get("reasoningEfforts").cloned().unwrap_or(json!([]))}}),
    )?)
}

impl Host {
    fn channel(&self, input: &Value) -> Result<Arc<Channel>> {
        let key = input["channel"]
            .as_str()
            .ok_or_else(|| Error::Config("缺少通道".into()))?;
        let mut channels = self.channels.lock().unwrap();
        if let Some(channel) = channels.get(key) {
            return Ok(channel.clone());
        }
        let dir = PathBuf::from(
            input["dataDir"]
                .as_str()
                .ok_or_else(|| Error::Config("缺少独立运行目录".into()))?,
        );
        std::fs::create_dir_all(&dir)?;
        let workspace = dir.join("workspace");
        std::fs::create_dir_all(&workspace)?;
        let configuration = config(input, &dir)?;
        let current = Arc::new(RwLock::new(String::new()));
        let tools = registry(input, self.wire.clone(), &workspace)?;
        let mut skills = SkillRegistry::default();
        for text in EMBEDDED_SKILLS {
            skills.load_embedded(text)?;
        }
        for directory in input["skillDirectories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            skills.load(&[(
                PathBuf::from(directory),
                sleepy_doll::extension::skills::SkillSource::User,
            )])?;
        }
        let operations = Arc::new(OperationEngine::new(
            Arc::new(OperationStore::open(&configuration.storage.database)?),
            Arc::new(ArtifactStore::new(dir.join("artifacts"), 64 * 1024 * 1024)?),
        ));
        let supervisor =
            Supervisor::new(configuration, Arc::new(skills), tools, operations, vec![])?;
        let channel = Arc::new(Channel {
            supervisor,
            current,
            active_run: Mutex::new(None),
            workspace,
        });
        channels.insert(key.into(), channel.clone());
        Ok(channel)
    }
    fn handle(self: &Arc<Self>, input: Value) -> Result<()> {
        let id = input["id"].as_str().unwrap_or("").to_owned();
        if !matches!(
            input["op"].as_str(),
            Some("run" | "complete" | "input_references" | "release" | "cancel")
        ) {
            return Err(Error::Config("不支持的Agent通道操作".into()));
        }
        if input["op"] == "input_references" {
            if let Some(key) = input["channel"].as_str()
                && let Some(channel) = self.channels.lock().unwrap().get(key)
                && *channel.current.read().unwrap() == id
            {
                channel
                    .supervisor
                    .update_input_references(serde_json::from_value(input["references"].clone())?);
            }
            return Ok(());
        }
        if input["op"] == "release" {
            let channel = input["channel"].as_str().and_then(|key| {
                let mut channels = self.channels.lock().unwrap();
                let owned = channels
                    .get(key)
                    .is_some_and(|channel| *channel.current.read().unwrap() == id);
                if owned { channels.remove(key) } else { None }
            });
            if let Some(channel) = channel {
                channel.supervisor.shutdown();
            }
            return Ok(());
        }
        if input["op"] == "cancel" {
            if let Some(cancel) = self.completes.lock().unwrap().get(&id) {
                cancel.cancel();
            }
            if let Some(key) = input["channel"].as_str()
                && let Some(channel) = self.channels.lock().unwrap().get(key)
                && *channel.current.read().unwrap() == id
                && let Some(run) = channel.active_run.lock().unwrap().as_ref()
            {
                let _ = channel.supervisor.cancel(run);
            }
            return Ok(());
        }
        if input["op"] == "complete" {
            let model: ModelConfig = serde_json::from_value(input["model"].clone())?;
            let messages: Vec<Message> = serde_json::from_value(input["messages"].clone())?;
            let hints = sleepy_doll::runtime::context::InputHints {
                references: serde_json::from_value(
                    input.get("inputReferences").cloned().unwrap_or(json!({})),
                )?,
                ..Default::default()
            };
            let estimated = sleepy_doll::runtime::context::estimate_messages_tokens(&messages)
                .saturating_add(hints.message_tokens(&messages))
                .saturating_add(1024);
            let budget =
                sleepy_doll::runtime::policy::input_token_budget(&Default::default(), &model);
            if estimated > budget {
                return Err(Error::Config(format!(
                    "本次文字及媒体输入预算约{estimated} token，超过可用输入预算{budget}"
                )));
            }
            let wire = self.wire.clone();
            let owner = self.clone();
            let cancel = CancellationToken::new();
            self.completes
                .lock()
                .unwrap()
                .insert(id.clone(), cancel.clone());
            runtime().spawn(async move {
                let result=gateway::complete(model,&messages,&[],&cancel, |_|Ok(())).await;
                owner.completes.lock().unwrap().remove(&id);
                match result {Ok(answer)=>wire.emit(json!({"op":"result","id":id,"text":answer.text,"state":"answered","engine":"Sleepy Doll"})),Err(error)=>wire.emit(json!({"op":"result","id":id,"error":error.user_message(),"protocolError":match &error {Error::ModelProtocol(reason)=>Some(reason),_=>None}}))}
            });
            return Ok(());
        }
        let channel = self.channel(&input)?;
        if let Some(previous) = channel.active_run.lock().unwrap().as_ref() {
            let previous = channel.supervisor.journal.get(previous)?;
            if !previous.state.terminal() && previous.state != RunState::AwaitingUser {
                return Err(Error::Conflict(
                    "当前通道仍有任务在运行，请先取消或等待结束".into(),
                ));
            }
        }
        channel
            .supervisor
            .update_input_references(serde_json::from_value(
                input.get("inputReferences").cloned().unwrap_or(json!({})),
            )?);
        *channel.current.write().unwrap() = id.clone();
        let dir = channel.workspace.parent().unwrap();
        channel.supervisor.update_config(config(&input, dir)?)?;
        let mut skills = SkillRegistry::default();
        for text in EMBEDDED_SKILLS {
            skills.load_embedded(text)?;
        }
        for directory in input["skillDirectories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            skills.load(&[(
                PathBuf::from(directory),
                sleepy_doll::extension::skills::SkillSource::User,
            )])?;
        }
        channel.supervisor.update_extensions(
            Arc::new(skills),
            registry(&input, self.wire.clone(), &channel.workspace)?,
            vec![],
        );
        let conversation = input["conversationId"].as_str().unwrap_or("qq");
        let previous = match channel.active_run.lock().unwrap().clone() {
            Some(id) => {
                let previous = channel.supervisor.journal.get(&id)?;
                if previous.conversation_id == conversation
                    || input["resumeAwaitingUser"].as_bool().unwrap_or(false)
                        && previous.state == RunState::AwaitingUser
                {
                    Some(id)
                } else {
                    None
                }
            }
            None => None,
        };
        let prompt = input["prompt"].as_str().unwrap_or("");
        let context = input["context"].as_str().unwrap_or("");
        let run = if let Some(previous) = previous {
            let run = channel.supervisor.journal.get(&previous)?;
            if run.state == RunState::AwaitingUser {
                if !context.is_empty() {
                    channel.supervisor.journal.append_message(
                        &run,
                        &sleepy_doll::runtime::context::message(
                            sleepy_doll::model::Role::Context,
                            context,
                        ),
                    )?;
                }
                let requests = channel
                    .supervisor
                    .journal
                    .open_question_requests(&run.conversation_id)?;
                if requests.len() == 1
                    && requests[0]["questions"]
                        .as_array()
                        .is_some_and(|q| q.len() == 1)
                {
                    let question_id = requests[0]["questions"][0]["id"]
                        .as_str()
                        .unwrap_or("answer");
                    let mut answers = serde_json::Map::new();
                    answers.insert(
                        question_id.into(),
                        json!({"answers":[input["userText"].as_str().unwrap_or(prompt)]}),
                    );
                    channel.supervisor.journal.answer_question_request(
                        &run.id,
                        requests[0]["requestId"].as_str().unwrap_or(""),
                        &Value::Object(answers),
                        Some(&id),
                    )?;
                } else {
                    channel
                        .supervisor
                        .journal
                        .input(&run.id, "supplement", prompt)?;
                }
                run
            } else {
                channel.supervisor.submit_with_context(
                    prompt,
                    context,
                    Some(conversation),
                    input["clientKey"].as_str().unwrap_or(&id),
                    None,
                    Some("qq"),
                )?
            }
        } else {
            channel.supervisor.submit_with_context(
                prompt,
                context,
                Some(conversation),
                input["clientKey"].as_str().unwrap_or(&id),
                None,
                Some("qq"),
            )?
        };
        if let Some(references) = input["sourceReferences"].as_array() {
            for reference in references {
                if reference["kind"] == "source-reference" {
                    channel.supervisor.journal.append_message(
                        &run,
                        &sleepy_doll::runtime::context::message(
                            sleepy_doll::model::Role::Context,
                            reference.to_string(),
                        ),
                    )?;
                }
            }
        }
        *channel.active_run.lock().unwrap() = Some(run.id.clone());
        self.wire.emit(json!({"op":"started","id":id,"runId":run.id,"engine":"Sleepy Doll","tools":channel.supervisor.tool_definitions().iter().map(|d|d.name.clone()).collect::<Vec<_>>(),"bridgeEnabled":false}));
        let wire = self.wire.clone();
        runtime().spawn(async move {
            let mut cursor=0;
            loop {
                match channel.supervisor.journal.events(&run.conversation_id,cursor) {
                    Ok(events)=>for event in events {cursor=event.sequence;if event.run_id==run.id {wire.emit(json!({"op":"event","id":id,"event":event}));}},
                    Err(error)=>{wire.emit(json!({"op":"result","id":id,"error":error.user_message()}));break;}
                }
                match channel.supervisor.journal.get(&run.id) {
                    Ok(latest) if latest.state.terminal()=>{wire.emit(json!({"op":"result","id":id,"runId":run.id,"text":latest.result.unwrap_or_default(),"state":latest.state,"error":latest.error,"engine":"Sleepy Doll","contextTokens":latest.context_tokens,"toolCalls":latest.tool_calls}));break;}
                    Ok(latest) if matches!(latest.state,RunState::AwaitingUser|RunState::AwaitingApproval)=>{
                        let questions=channel.supervisor.journal.open_question_requests(&run.conversation_id).unwrap_or_default();
                        if !questions.is_empty(){wire.emit(json!({"op":"result","id":id,"runId":run.id,"text":questions.iter().map(|q|q.to_string()).collect::<Vec<_>>().join("\n"),"state":latest.state,"questions":questions}));break;}
                    }
                    Err(error)=>{wire.emit(json!({"op":"result","id":id,"error":error.user_message()}));break;}
                    _=>{}
                }
                if unix_now()>run.deadline+5 { let _=channel.supervisor.cancel(&run.id); }
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
        });
        Ok(())
    }
}

fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("本宿主不支持本机命令执行模式");
        std::process::exit(2);
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let wire = Arc::new(Wire {
        output: Mutex::new(std::io::stdout()),
        closed: AtomicBool::new(false),
        pending: Mutex::new(HashMap::new()),
    });
    let host = Arc::new(Host {
        wire: wire.clone(),
        channels: Mutex::new(HashMap::new()),
        completes: Mutex::new(HashMap::new()),
    });
    wire.emit(json!({"op":"ready","engine":"Sleepy Doll","version":env!("CARGO_PKG_VERSION"),"revision":env!("SLEEPY_SOURCE_REVISION"),"bridgeEnabled":false}));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(input) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if input["op"] == "shutdown" {
            break;
        }
        if input["op"] == "tool_result" {
            if let Some(id) = input["id"].as_str()
                && let Some(sender) = wire.pending.lock().unwrap().remove(id)
            {
                let _ = sender.send(if let Some(error) = input["error"].as_str() {
                    Err(error.into())
                } else {
                    Ok(input["result"].clone())
                });
            }
            continue;
        }
        if let Err(error) = host.handle(input.clone()) {
            wire.emit(json!({"op":"result","id":input["id"],"error":error.user_message()}));
        }
    }
    for (_, sender) in wire.pending.lock().unwrap().drain() {
        let _ = sender.send(Err("QQ通道已断开".into()));
    }
    for cancel in host.completes.lock().unwrap().values() {
        cancel.cancel();
    }
    for channel in host.channels.lock().unwrap().values() {
        channel.supervisor.shutdown();
    }
    wire.close();
}
