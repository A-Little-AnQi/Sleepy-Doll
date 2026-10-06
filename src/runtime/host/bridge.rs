use crate::runtime::{policy::RuntimeConfig, store::journal::Journal, types::*};
use crate::{
    config::BridgeConfig,
    error::{Error, Result},
    extension::validate,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

/// 游戏是否真正可执行动作：截图器就绪且已在主界面。旧版桥没有 inMainUi
/// 字段时退化为仅检查截图器，避免新旧混合窗口期把一切调用都拒掉。
fn game_ready(snapshot: &Value) -> bool {
    if snapshot["runtime"]["captureReady"] != true {
        return false;
    }
    snapshot["runtime"]["inMainUi"].as_bool().unwrap_or(true)
}

/// 提交前就绪闸门的判定，由契约元数据驱动：
/// - 实例不一致 → 拒绝；
/// - 不需要截图的能力（只读不进入该通道；configurationWrite/hostCommand）→ 放行；
/// - 已就绪 → 放行；
/// - 未就绪且契约声明 preparation=ensureGameReady → 先执行原生就绪准备；
/// - 其余 → 按原语义拒绝（不让模型空等）。
#[derive(Debug, PartialEq, Eq)]
enum ReadinessGate {
    Proceed,
    Prepare,
    InstanceMismatch,
    NotReady,
}

fn readiness_gate(
    effect: Option<&str>,
    preparation: Option<&str>,
    snapshot: &Value,
    instance: &str,
) -> ReadinessGate {
    if snapshot["instanceId"].as_str() != Some(instance) {
        return ReadinessGate::InstanceMismatch;
    }
    // 只读、配置写与宿主命令都不依赖截图就绪；防御性地一并不触发准备。
    let requires_capture = !matches!(
        effect,
        Some("readOnly" | "configurationWrite" | "hostCommand")
    );
    if !requires_capture || game_ready(snapshot) {
        return ReadinessGate::Proceed;
    }
    if preparation == Some("ensureGameReady") {
        ReadinessGate::Prepare
    } else {
        ReadinessGate::NotReady
    }
}

/// Bridge 观测可以作为执行依据的最大年龄（秒）。
const OBSERVATION_MAX_AGE_SEC: i64 = 15;

#[derive(Clone)]
pub struct Bridge {
    config: BridgeConfig,
    client: reqwest::Client,
    /// Job 事件流专用：用空闲超时，不用总超时。
    stream_client: reqwest::Client,
    /// 生产恒为 true：每个非 info 请求前都核对对端进程来源。只有传输层协议
    /// 静态测试（本地假服务器）经 for_transport_tests 置 false；不构成公开 API。
    origin_preflight: bool,
}
pub struct Invocation<'a> {
    pub authorization: &'a std::sync::RwLock<crate::config::AppConfig>,
    pub call_id: &'a str,
    pub binding: &'a crate::runtime::host::catalog::Capability,
    pub arguments: &'a Value,
    pub resources: Value,
    pub catalog: &'a crate::runtime::host::catalog::Catalog,
}
impl Bridge {
    pub fn new(config: BridgeConfig) -> Result<Self> {
        let idle = Duration::from_millis(config.timeout_ms);
        Ok(Self {
            client: reqwest::Client::builder().timeout(idle).build()?,
            stream_client: reqwest::Client::builder()
                .connect_timeout(idle)
                .read_timeout(idle)
                .build()?,
            config,
            origin_preflight: true,
        })
    }

    /// 仅供传输层协议静态测试：跳过对端进程来源核对（本地假服务器没有官方
    /// BetterGI 进程）。生产路径一律走 new()，预检不会被跳过。
    #[cfg(test)]
    pub(crate) fn for_transport_tests(config: BridgeConfig) -> Result<Self> {
        Ok(Self {
            origin_preflight: false,
            ..Self::new(config)?
        })
    }
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        if !self.config.enabled {
            return Err(Error::Tool("BGI Bridge 未启用".into()));
        }
        if self.origin_preflight && path != "/bridge/v1/info" {
            let config = self.config.clone();
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                checked = tokio::task::spawn_blocking(move || crate::bridge::control::info(&config)) => {
                    checked.map_err(|_| Error::Tool(crate::bridge::origin::REJECTED_MESSAGE.into()))??;
                }
            }
        }
        let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);
        let mut req = self
            .client
            .request(
                if method == "GET" {
                    reqwest::Method::GET
                } else {
                    reqwest::Method::POST
                },
                url,
            )
            .bearer_auth(self.config.token.as_deref().unwrap_or(""));
        if let Some(body) = body {
            req = req.json(body);
            if let Some(key) = body["requestId"].as_str() {
                req = req.header("idempotency-key", key);
            }
        }
        if matches!(path, "/bridge/v1/invoke" | "/bridge/v1/read")
            && body.is_some_and(|value| value["methodId"] == "bgi.wait_ready")
        {
            let seconds = body
                .and_then(|value| value["arguments"]["timeoutSeconds"].as_u64())
                .unwrap_or(120)
                .clamp(1, 180);
            req = req.timeout(Duration::from_secs(seconds + 10));
        }
        let response =
            tokio::select! {_ = cancel.cancelled()=>return Err(Error::Cancelled),r=req.send()=>r?};
        let status = response.status();
        let value = crate::runtime::gateway::read_json(response, cancel).await?;
        if !status.is_success() {
            // 桥的错误体是 {code, message}；INTERNAL 是未处理异常的兜底，
            // REQUEST_FAILED 表示响应不是桥写的。
            let code = value["code"].as_str().unwrap_or("REQUEST_FAILED");
            if status.is_server_error() && matches!(code, "INTERNAL" | "REQUEST_FAILED") {
                return Err(Error::Http(format!(
                    "Bridge 响应 {} 无法确认执行结果",
                    status.as_u16()
                )));
            }
            let detail = value["message"].as_str().unwrap_or("");
            return Err(Error::Tool(if detail.is_empty() {
                format!("Bridge {}: {code}", status.as_u16())
            } else {
                format!("Bridge {}: {code} {detail}", status.as_u16())
            }));
        }
        if self.origin_preflight && path == "/bridge/v1/info" {
            let info = value.clone();
            tokio::select! {
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                checked = tokio::task::spawn_blocking(move || crate::bridge::origin::require_info(&info)) => {
                    checked.map_err(|_| Error::Tool(crate::bridge::origin::REJECTED_MESSAGE.into()))??;
                }
            }
        }
        Ok(value)
    }
    pub async fn get(&self, path: &str, cancel: &CancellationToken) -> Result<Value> {
        let mut last = None;
        for attempt in 0..3 {
            match self.request("GET", path, None, cancel).await {
                Ok(v) => return Ok(v),
                Err(Error::Http(s)) => last = Some(Error::Http(s)),
                Err(Error::Timeout(s)) => last = Some(Error::Timeout(s)),
                Err(e) => return Err(e),
            }
            if attempt == 2 {
                break;
            }
            tokio::select! {_ = cancel.cancelled()=>return Err(Error::Cancelled),_ = tokio::time::sleep(Duration::from_millis(250*(1<<attempt)))=>{}}
        }
        Err(last.unwrap_or_else(|| Error::Http("Bridge 请求失败".into())))
    }
    pub async fn search(&self, query: &str, cancel: &CancellationToken) -> Result<Value> {
        let encoded: String = reqwest::Url::parse_with_params("http://localhost", [("q", query)])
            .unwrap()
            .query()
            .unwrap()
            .into();
        self.get(&format!("/bridge/v1/catalog?{encoded}"), cancel)
            .await
    }
    pub async fn describe(&self, id: &str, cancel: &CancellationToken) -> Result<Value> {
        if id.is_empty() || id.contains(['/', '?', '#', '%']) {
            return Err(Error::Tool("能力标识不合法".into()));
        }
        self.get(&format!("/bridge/v1/catalog/{id}"), cancel).await
    }
    pub async fn invoke(
        &self,
        journal: &Arc<Journal>,
        run: &mut Run,
        invocation: Invocation<'_>,
        policy: &RuntimeConfig,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let Invocation {
            authorization,
            call_id,
            binding,
            arguments: args,
            resources,
            catalog,
        } = invocation;
        let id = &binding.method_id;
        let info = self.get("/bridge/v1/info", cancel).await?;
        if info["protocolVersion"] != "1"
            || !info["features"]
                .as_array()
                .is_some_and(|f| f.iter().any(|v| v == "jobs"))
        {
            return Err(Error::Tool("Bridge 协议不兼容或缺少 Job 能力".into()));
        }
        let instance = info["instanceId"]
            .as_str()
            .ok_or_else(|| Error::Tool("Bridge 未返回实例标识".into()))?;
        if self
            .config
            .instance_id
            .as_deref()
            .is_some_and(|expected| expected != instance)
        {
            return Err(Error::Tool("BGI 实例已变化".into()));
        }
        let descriptor = self.describe(id, cancel).await?;
        if descriptor["callable"] != true
            || descriptor["methodId"] != *id
            || descriptor["catalogVersion"] != binding.catalog_version
        {
            return Err(Error::Tool("能力不可用或已变化".into()));
        }
        let issues = validate(args, &descriptor["inputSchema"], "$");
        if !issues.is_empty() {
            return Err(Error::Tool(format!(
                "参数不符合能力契约: {}",
                json!(issues)
            )));
        }
        let version = descriptor["catalogVersion"]
            .as_str()
            .ok_or_else(|| Error::Tool("缺少能力版本，无法安全提交".into()))?;
        let plan = journal.plan(&run.id)?;
        let attempts = journal.attempts(&run.id)?;
        let mut step_id = None;
        // 同参数动作不重复执行：取原请求重发，桥按幂等键返回同一个 Job 的最新
        // 状态。之前这里直接拒绝并要求模型「核对结果」，却不告诉它结果在哪，
        // 模型只能反复撞墙，白烧大量 token。
        let mut replay: Option<Value> = None;
        if plan.is_none()
            && let Some(original) = attempts.iter().find(|a| {
                a.request["capabilityId"] == binding.id && a.request["arguments"] == *args
            })
        {
            replay = Some(original.request["wire"].clone());
        }
        if let Some(plan) = &plan {
            let completed_reads = journal.completed_read_steps(&run.id, plan)?;
            let done = |step: &str| {
                completed_reads.contains(step)
                    || attempts
                        .iter()
                        .any(|a| a.request["stepId"] == step && a.outcome == "verifiedSucceeded")
            };
            let next = plan
                .steps
                .iter()
                .find(|s| !done(&s.id))
                .ok_or_else(|| Error::Tool("计划已执行完成，请先修订计划".into()))?;
            let matches_step = (next.capability_id.as_deref() == Some(binding.id.as_str())
                && next.arguments == *args)
                || (next.tool.as_deref() == Some("bgi.api.invoke")
                    && next.arguments["methodId"] == binding.id
                    && next.arguments["arguments"] == *args);
            if !matches_step || !next.depends_on.iter().all(|id| done(id)) {
                return Err(Error::Tool(
                    "调用不匹配当前可执行步骤或前序结果未验证".into(),
                ));
            }
            if attempts.iter().any(|a| a.request["stepId"] == next.id) {
                return Err(Error::Tool("该步骤已有调用尝试，请核对结果后再继续".into()));
            }
            step_id = Some(next.id.clone());
        }
        let mut request = json!({"instanceId":instance,"methodId":id,"capabilityId":binding.id,"stepId":step_id,"binding":binding,"resources":resources,"bridgeFeatures":info["features"],"catalogVersion":version,"arguments":args,"planRevision":plan.map(|p|p.revision).unwrap_or(0)});
        // 呈现始终附加到同一 request 上（哈希覆盖它）：有专门人话呈现用之，
        // 否则退回通用 fallback，不再把内部 binding.description 当用户摘要。
        request["presentation"] =
            crate::bridge::approval::presentation(id, args).unwrap_or_else(|| {
                crate::bridge::approval::fallback_presentation(
                    descriptor["displayName"].as_str().unwrap_or(""),
                    args,
                    descriptor["effect"].as_str().unwrap_or(""),
                )
            });
        let effect =
            serde_json::from_value::<crate::extension::ToolEffect>(descriptor["effect"].clone())
                .unwrap_or(crate::extension::ToolEffect::Unknown);
        let current_permission = crate::runtime::operation::permissions::PermissionRequest {
            provider_id: "core:bgi",
            resource_ids: &[],
            resource_kinds: &[],
            effect,
            // 契约未声明风险等级时按标准处理。
            risk: serde_json::from_value(descriptor["risk"].clone())
                .unwrap_or(crate::extension::RiskLevel::Standard),
            unattended: crate::extension::UnattendedPolicy::Forbidden,
            // 这一层看不到字段级差异，按未界定处理。
            scope: None,
        };
        // 授权取最新配置，而不是运行开始时的快照：会话中切审批级别立即生效。
        let (latest_mode, latest_grants) = {
            let latest = authorization.read().unwrap();
            (
                latest.runtime.permission_mode,
                latest.runtime.trust_grants.clone(),
            )
        };
        let decision = crate::runtime::operation::permissions::PermissionEngine::decide(
            latest_mode,
            &current_permission,
            &latest_grants,
        );
        if decision == crate::runtime::operation::permissions::PermissionDecision::Deny {
            return Err(Error::Tool("当前为只读级别，不能修改配置或执行命令".into()));
        }
        // 复核时按放行的同一条依据重查。
        let mut approval_expires = None;
        let mut allowed_by_mode = false;
        if decision == crate::runtime::operation::permissions::PermissionDecision::Ask
            && !policy.allows(&request)
        {
            let approval = Approval {
                id: uuid::Uuid::new_v4().to_string(),
                run_id: run.id.clone(),
                request_hash: hash(&request),
                request: request.clone(),
                expires_at: unix_now() + 300,
                decision: None,
            };
            journal.approval(&approval)?;
            approval_expires = Some(approval.expires_at);
            journal.save(run, RunState::AwaitingApproval)?;
            journal.emit(run, "approval.requested", json!(approval))?;
            let notifier = journal.notifier();
            loop {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                if unix_now() > run.deadline {
                    journal.save(run, RunState::Executing)?;
                    return Err(Error::Tool("等待授权超过任务时限".into()));
                }
                let result = journal.approval_result(&approval.id)?;
                if result.expires_at < unix_now() {
                    journal.save(run, RunState::Executing)?;
                    return Err(Error::Tool("授权等待已过期".into()));
                }
                // 会话中切审批级别：等待中的审批按新级别继续（完全控制同意、
                // 只读拒绝为可恢复错误）；已答复/过期审批不被覆盖。
                if let Some(approved) = crate::runtime::apply_permission_switch_to_pending(
                    journal,
                    authorization.read().unwrap().runtime.permission_mode,
                    run,
                    &approval.id,
                )? {
                    journal.save(run, RunState::Executing)?;
                    if !approved {
                        return Err(Error::Tool(
                            crate::runtime::operation::permissions::plan_only_blocked_message()
                                .into(),
                        ));
                    }
                    // 级别切换放行与级别直通同权：提交前按最新模式复核。
                    allowed_by_mode = true;
                    approval_expires = None;
                    break;
                }
                if let Some(allowed) = result.decision {
                    if !allowed {
                        journal.save(run, RunState::Executing)?;
                        return Err(Error::Tool("用户拒绝了此操作".into()));
                    }
                    if result.request_hash != hash(&request) {
                        return Err(Error::Conflict("审批所绑定的请求已变化".into()));
                    }
                    break;
                }
                tokio::select! {
                    _ = notifier.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                }
            }
            journal.save(run, RunState::Executing)?;
        } else if decision == crate::runtime::operation::permissions::PermissionDecision::Allow {
            allowed_by_mode = true;
        }
        // 审批等待结束后重新核对可变状态。
        if self.describe(id, cancel).await? != descriptor {
            return Err(Error::Tool("能力契约已变化，请重新确认".into()));
        }
        if catalog.resolve(&binding.id, args)?.1 != resources {
            return Err(Error::Tool("资源绑定已变化".into()));
        }
        let mut snapshot = self.get("/bridge/v1/state", cancel).await?;
        // 通用前置就绪准备：只有契约声明 preparation=ensureGameReady 的已授权
        // 写动作才执行（只读不进入该通道，configurationWrite/hostCommand 不需要
        // 截图）。准备本身作为被记录的动作走原生 bgi.ensure_game_ready，完成后
        // 重取快照，原有的实例/就绪/前台核验链全部保留。
        let preparation = descriptor["preparation"].as_str();
        match readiness_gate(
            descriptor["effect"].as_str(),
            preparation,
            &snapshot,
            instance,
        ) {
            ReadinessGate::Proceed => {}
            ReadinessGate::InstanceMismatch => {
                return Err(Error::Tool("桥实例已变化，请重新确认操作。".into()));
            }
            ReadinessGate::NotReady => {
                return Err(Error::Tool(
                    "游戏尚未就绪（以进入主界面为准）。先调用 bgi.start_game 启动原神，用 bgi.wait_ready 阻塞等到 ready=true 再重试，不由模型轮询；截图器就绪但仍在登录或加载画面时不要提交动作。"
                        .into(),
                ));
            }
            ReadinessGate::Prepare => {
                // 就绪准备按生产写动作通道提交：独立 Attempt（requestId/幂等键、
                // 执行锁、Job 跟随），不内联读 result。它是已授权运行的必要组成，
                // 不再递归触发就绪闸门；失败/取消时 Job 证据保留在 attempts。
                let prep_call_id = format!("{call_id}#prep");
                let prep_request = json!({
                    "instanceId":instance,
                    "methodId":"bgi.ensure_game_ready",
                    "capabilityId":"bgi.ensure_game_ready",
                    "arguments":{},
                    "bridgeFeatures":info["features"],
                    "catalogVersion":version,
                });
                journal.emit(
                    run,
                    "game.prepare",
                    json!({"attemptId":prep_call_id,"methodId":"bgi.ensure_game_ready","reason":"gameNotReady"}),
                )?;
                let mut prep = journal.prepare(run, &prep_call_id, prep_request, instance)?;
                while !journal.acquire(&prep)? {
                    if cancel.is_cancelled() {
                        prep.outcome = "cancelled".into();
                        journal.attempt(&prep)?;
                        return Err(Error::Cancelled);
                    }
                    if unix_now() > run.deadline {
                        prep.outcome = "notSubmitted".into();
                        journal.attempt(&prep)?;
                        return Err(Error::Tool("等待游戏执行权超时（就绪准备）".into()));
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                prep.request["requestId"] = json!(prep.id);
                prep.request["execution"] = json!({
                    "onDisconnect":"continue",
                    "deadlineMs":(run.deadline - unix_now()).max(1) * 1000
                });
                prep.request_hash = hash(&prep.request);
                prep.outcome = "submitting".into();
                journal.attempt(&prep)?;
                let prep_wire = json!({
                    "requestId":prep.id,"instanceId":instance,"catalogVersion":version,
                    "methodId":"bgi.ensure_game_ready","arguments":{},
                    "execution":prep.request["execution"]
                });
                prep.request["wire"] = prep_wire.clone();
                prep.request_hash = hash(&prep_wire);
                journal.attempt(&prep)?;
                let accepted = self
                    .request("POST", "/bridge/v1/invoke", Some(&prep_wire), cancel)
                    .await;
                match accepted {
                    Ok(v) => {
                        prep.job_id = v["jobId"].as_str().map(str::to_owned);
                        if prep.job_id.is_none() {
                            prep.outcome = "unknown".into();
                            prep.evidence = v;
                            journal.attempt(&prep)?;
                            return Err(Error::Conflict("就绪准备未返回可恢复的 Job".into()));
                        }
                        prep.evidence = v;
                    }
                    Err(Error::Tool(message)) => {
                        prep.outcome = "failed".into();
                        prep.evidence = json!({"reason":message});
                        journal.attempt(&prep)?;
                        journal.release(&prep)?;
                        return Err(Error::Tool(format!("就绪准备被拒绝：{message}")));
                    }
                    Err(e) => {
                        prep.outcome = "unknown".into();
                        prep.evidence = json!({"reason":e.to_string()});
                        journal.attempt(&prep)?;
                        return Err(Error::Conflict(
                            "就绪准备提交结果未知；证据已保留，需要对账".into(),
                        ));
                    }
                }
                prep.outcome = "running".into();
                journal.attempt(&prep)?;
                journal.save(run, RunState::WaitingJob)?;
                let prepared = self.wait(journal, run, &mut prep, cancel).await?;
                // 准备 Job 结束后 run 处于 Verifying；主动作继续前回到 Executing，
                // 与会话循环在每个工具调用后重置运行状态的做法一致。
                journal.save(run, RunState::Executing)?;
                journal.emit(
                    run,
                    "game.prepare.result",
                    json!({
                        "attemptId":prep.id,"jobId":prepared["jobId"],
                        "outcome":prepared["outcome"],
                        "ready":prepared["evidence"]["result"]["ready"],
                    }),
                )?;
                if prepared["outcome"] != "verifiedSucceeded"
                    || prepared["evidence"]["result"]["ready"] != true
                {
                    return Err(Error::Tool(format!(
                        "就绪准备未达成 ready=true（outcome={}）；按返回的具体阻碍处理后重试。",
                        prepared["outcome"]
                    )));
                }
                // 准备后重取快照：实例/就绪/时效核验继续生效，不因准备而放宽。
                snapshot = self.get("/bridge/v1/state", cancel).await?;
                if snapshot["instanceId"].as_str() != Some(instance) || !game_ready(&snapshot) {
                    return Err(Error::Tool(
                        "就绪准备完成后快照仍未就绪；以最新快照为准，不继续提交。".into(),
                    ));
                }
            }
        }
        let requires_capture = !matches!(
            descriptor["effect"].as_str(),
            Some("configurationWrite" | "hostCommand")
        );
        // 需要画面的动作必须让游戏前台：后台时模拟输入会被系统丢弃。这里
        // 阻塞几秒是可接受的——就绪检查本来就在提交路径上。
        let needs_foreground = !matches!(
            descriptor["effect"].as_str(),
            Some("configurationWrite" | "hostCommand")
        );
        if needs_foreground {
            let game = &snapshot["runtime"];
            let handle = game["gameHandle"].as_i64().unwrap_or(0);
            let active = game["windowActive"].as_bool().unwrap_or(false);
            if !active && handle != 0 {
                journal.emit(
                    run,
                    "game.focus",
                    json!({"attemptId":call_id,"window":handle}),
                )?;
                let window = handle as isize;
                let focused = tokio::task::spawn_blocking(move || {
                    super::foreground::focus_game_window(window, Duration::from_secs(5))
                })
                .await
                .unwrap_or(false);
                if !focused {
                    return Err(Error::Tool(
                        "无法把原神切到前台（可能被系统前台锁拒绝）。请手动点击一次游戏窗口，再重试。"
                            .into(),
                    ));
                }
                // 前台刚切过去，重取一次快照让后续观测反映最新状态。
                snapshot = self.get("/bridge/v1/state", cancel).await?;
                if snapshot["instanceId"] != instance || !game_ready(&snapshot) {
                    return Err(Error::Tool("游戏状态在前置后不再就绪".into()));
                }
            }
        }
        request["preconditionSnapshot"] = snapshot["snapshotId"].clone();
        let mut a = journal.prepare(run, call_id, request, instance)?;
        while !journal.acquire(&a)? {
            if cancel.is_cancelled() {
                a.outcome = "cancelled".into();
                journal.attempt(&a)?;
                return Err(Error::Cancelled);
            }
            if unix_now() > run.deadline {
                a.outcome = "notSubmitted".into();
                journal.attempt(&a)?;
                return Err(Error::Tool("等待游戏执行权超时".into()));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if cancel.is_cancelled() {
            a.outcome = "cancelled".into();
            journal.attempt(&a)?;
            journal.release(&a)?;
            return Err(Error::Cancelled);
        }
        let check = async {
            {
                let latest = authorization.read().unwrap();
                if !latest.bridge.enabled
                    || latest.bridge.base_url != self.config.base_url
                    || latest.bridge.token != self.config.token
                {
                    return Err(Error::Tool("Bridge 配置已变化，请重新发起操作".into()));
                }
                // 提交前最终复核：任何路径（级别直通或审批放行）下，最新级别已
                // 切为只读就不再提交本次写入；已发出的动作不假称撤回。
                if latest.runtime.permission_mode
                    == crate::runtime::operation::permissions::PermissionMode::PlanOnly
                {
                    return Err(Error::Tool(
                        crate::runtime::operation::permissions::plan_only_blocked_message().into(),
                    ));
                }
                if allowed_by_mode {
                    if crate::runtime::operation::permissions::PermissionEngine::decide(
                        latest.runtime.permission_mode,
                        &current_permission,
                        &latest.runtime.trust_grants,
                    ) != crate::runtime::operation::permissions::PermissionDecision::Allow
                    {
                        return Err(Error::Tool("审批级别已改变，请重新发起操作".into()));
                    }
                } else if approval_expires.is_none() && !latest.runtime.allows(&a.request) {
                    return Err(Error::Tool("预授权已撤销或改变".into()));
                }
            }
            if approval_expires.is_some_and(|t| t < unix_now())
                || (!allowed_by_mode && approval_expires.is_none() && !policy.allows(&a.request))
            {
                return Err(Error::Tool("排队期间授权已过期".into()));
            }
            if self.describe(id, cancel).await? != descriptor {
                return Err(Error::Tool("排队期间能力版本改变".into()));
            }
            if catalog.resolve(&binding.id, args)?.1 != resources {
                return Err(Error::Tool("排队期间资源变化".into()));
            }
            let fresh = self.get("/bridge/v1/state", cancel).await?;
            if fresh["instanceId"] != instance || (requires_capture && !game_ready(&fresh)) {
                return Err(Error::Tool("游戏状态不再满足前置条件".into()));
            }
            let stamp = fresh["observedAt"]
                .as_str()
                .and_then(|s| {
                    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
                        .ok()
                })
                .map(|t| t.unix_timestamp());
            // 桥可能返回缓存的截图状态。
            let age = stamp.map(|t| unix_now() - t).unwrap_or(i64::MAX);
            if !stamp.is_some_and(|t| t <= unix_now() + 2 && age <= OBSERVATION_MAX_AGE_SEC) {
                return Err(Error::Tool(format!("游戏观测已过期（{age} 秒前）")));
            }
            Ok(())
        }
        .await;
        if let Err(error) = check {
            a.outcome = "notSubmitted".into();
            journal.attempt(&a)?;
            journal.release(&a)?;
            return Err(error);
        }
        a.request["requestId"] = json!(a.id);
        a.request["execution"] =
            json!({"onDisconnect":"continue","deadlineMs":(run.deadline-unix_now()).max(1)*1000});
        a.request_hash = hash(&a.request);
        a.outcome = "submitting".into();
        journal.attempt(&a)?;
        let wire = replay.unwrap_or_else(|| {
            json!({"requestId":a.id,"instanceId":instance,"catalogVersion":version,"methodId":id,"arguments":args,"execution":a.request["execution"]})
        });
        a.request["wire"] = wire.clone();
        a.request_hash = hash(&wire);
        journal.attempt(&a)?;
        let mut accepted = self
            .request("POST", "/bridge/v1/invoke", Some(&wire), cancel)
            .await;
        for retry in 0..3u32 {
            // BUSY / GAME_BUSY：有互斥执行；QUEUE_FULL：容量已满。按契约有界退避重试。
            if !matches!(&accepted,Err(Error::Tool(message)) if message.contains("BUSY")||message.contains("QUEUE_FULL"))
            {
                break;
            }
            if unix_now() >= run.deadline || cancel.is_cancelled() {
                break;
            }
            journal.emit(
                run,
                "capacity.wait",
                json!({"attemptId":a.id,"retry":retry+1}),
            )?;
            tokio::select! {_ = cancel.cancelled()=>{accepted=Err(Error::Cancelled);break;},_ = tokio::time::sleep(Duration::from_millis(250*(1<<retry)))=>{}}
            accepted = self
                .request("POST", "/bridge/v1/invoke", Some(&wire), cancel)
                .await;
        }
        let deduplicates = info["features"]
            .as_array()
            .is_some_and(|f| f.iter().any(|f| f == "idempotency"));
        if deduplicates && matches!(&accepted, Err(Error::Http(_))) && !cancel.is_cancelled() {
            // 只有协商出幂等能力时才允许重发。
            accepted = self
                .request("POST", "/bridge/v1/invoke", Some(&wire), cancel)
                .await;
        }
        match accepted {
            Ok(v) => {
                a.job_id = v["jobId"].as_str().map(str::to_owned);
                a.evidence = v;
            }
            Err(Error::Tool(e)) => {
                a.outcome = "failed".into();
                a.evidence = json!({"reason":e});
                journal.attempt(&a)?;
                journal.release(&a)?;
                // 桥的 message 带缺项与拒绝原因，原样交给模型。
                return Err(Error::Tool(format!("Bridge 拒绝执行：{e}")));
            }
            Err(e) => {
                // 传输失败（含发送期间取消）的结果无法判断。
                a.outcome = "unknown".into();
                a.evidence = json!({"reason":e.to_string()});
                journal.attempt(&a)?;
                return Err(Error::Conflict(
                    "提交结果未知；已保留游戏执行锁，需要对账".into(),
                ));
            }
        }
        if a.job_id.is_none() {
            a.outcome = "unknown".into();
            journal.attempt(&a)?;
            return Err(Error::Conflict("Bridge 未返回可恢复的 Job".into()));
        }
        a.outcome = "running".into();
        journal.attempt(&a)?;
        journal.save(run, RunState::WaitingJob)?;
        self.wait(journal, run, &mut a, cancel).await
    }
    pub async fn wait(
        &self,
        journal: &Journal,
        run: &mut Run,
        a: &mut Attempt,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let control = CancellationToken::new();
        let mut cancelling = false;
        let mut cancel_deadline = 0;
        let supports_events = a.request["bridgeFeatures"]
            .as_array()
            .is_some_and(|f| f.iter().any(|f| f == "events"));
        let mut events: Option<futures_util::stream::BoxStream<'static, Result<Vec<u8>>>> = None;
        let mut event_buffer = Vec::new();
        let mut reconnect_at = std::time::Instant::now();
        let job_id = a
            .job_id
            .clone()
            .ok_or_else(|| Error::Conflict("Job 标识未知".into()))?;
        loop {
            if (cancel.is_cancelled() || unix_now() > run.deadline) && !cancelling {
                cancelling = true;
                cancel_deadline = unix_now() + 15;
                journal.save(run, RunState::Cancelling)?;
                let _ = self
                    .request(
                        "POST",
                        &format!("/bridge/v1/jobs/{job_id}/cancel"),
                        Some(&json!({})),
                        &control,
                    )
                    .await;
            }
            if cancelling && unix_now() > cancel_deadline {
                a.outcome = "unknown".into();
                journal.attempt(a)?;
                return Err(Error::Conflict("取消尚未确认；保留游戏执行锁".into()));
            }
            let v = match self
                .get(&format!("/bridge/v1/jobs/{job_id}"), &control)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    a.outcome = "unknown".into();
                    journal.attempt(a)?;
                    return Err(e);
                }
            };
            let state = v["state"].as_str().unwrap_or("");
            if matches!(state, "completed" | "failed" | "cancelled" | "interrupted") {
                a.evidence = v.clone();
                a.outcome = match state {
                    "cancelled" => "cancelled",
                    "failed" => "failed",
                    "completed" if v["verification"]["status"] == "succeeded" => {
                        "verifiedSucceeded"
                    }
                    "completed" if v["verification"]["status"] == "failed" => "verifiedFailed",
                    _ => "unknown",
                }
                .into();
                journal.attempt(a)?;
                if cancelling {
                    if state != "interrupted" {
                        journal.release(a)?;
                    }
                    return if state == "cancelled" {
                        Err(Error::Cancelled)
                    } else {
                        Err(Error::Conflict(
                            "取消与执行完成发生竞争，请检查执行结果".into(),
                        ))
                    };
                }
                journal.save(run, RunState::Verifying)?;
                let predicates = serde_json::from_value::<
                    Vec<crate::runtime::host::catalog::Predicate>,
                >(a.request["binding"]["postconditions"].clone())
                .unwrap_or_default();
                if state == "completed" && (a.outcome == "unknown" || !predicates.is_empty()) {
                    a.outcome = "unknown".into();
                    journal.attempt(a)?;
                    let fresh = self.get("/bridge/v1/state", &control).await?;
                    a.outcome = crate::runtime::operation::verifier::verify(
                        &predicates,
                        &fresh,
                        &a.instance_id,
                    )
                    .into();
                    a.evidence["postObservation"] = fresh.clone();
                    journal.attempt(a)?;
                    journal.emit(run, "observation", fresh)?;
                }
                if state != "interrupted" {
                    journal.release(a)?;
                }
                journal.emit(run, "attempt.finished", json!(a))?;
                return Ok(json!({"jobId":job_id,"outcome":a.outcome,"evidence":v}));
            }
            if supports_events && events.is_none() && std::time::Instant::now() >= reconnect_at {
                let mut request = self
                    .stream_client
                    .get(format!(
                        "{}/bridge/v1/events",
                        self.config.base_url.trim_end_matches('/')
                    ))
                    .bearer_auth(self.config.token.as_deref().unwrap_or(""));
                if let Some(cursor) = a.request["eventCursor"].as_str() {
                    request = request.header("last-event-id", cursor);
                }
                let response = tokio::select! {_ = cancel.cancelled()=>None,r=tokio::time::timeout(Duration::from_secs(2),request.send())=>r.ok().and_then(|r|r.ok())};
                if let Some(response) = response.filter(|r| r.status().is_success()) {
                    events = Some(
                        response
                            .bytes_stream()
                            .map(|r| r.map(|b| b.to_vec()).map_err(Error::from))
                            .boxed(),
                    );
                }
                reconnect_at = std::time::Instant::now() + Duration::from_secs(3);
            }
            if let Some(stream) = events.as_mut() {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(500))=>{},
                    chunk=stream.next()=>match chunk{
                        Some(Ok(bytes))=>{
                            event_buffer.extend(bytes);
                            while let Some(end)=event_buffer.iter().position(|b|*b==b'\n'){
                                let line=String::from_utf8_lossy(&event_buffer.drain(..=end).collect::<Vec<_>>()).into_owned();
                                if let Some(id)=line.trim().strip_prefix("id:"){a.request["eventCursor"]=json!(id.trim());journal.attempt(a)?;}
                            }
                            if event_buffer.len()>1024*1024{events=None;event_buffer.clear();}
                        },
                        _=>{events=None;event_buffer.clear();}
                    }
                }
            } else {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
}

#[cfg(test)]
mod readiness_gate_tests {
    use super::*;
    use serde_json::json;

    fn snapshot(instance: &str, capture: bool, main_ui: bool) -> Value {
        json!({
            "instanceId": instance,
            "snapshotId": "s1",
            "observedAt": "2026-10-03T00:00:00Z",
            "runtime": {"captureReady": capture, "inMainUi": main_ui, "gameHandle": 1}
        })
    }

    #[test]
    fn game_write_without_preparation_keeps_the_old_rejection() {
        // 旧语义：未声明 preparation 的 gameWrite 未就绪时仍被拒，不静默准备。
        assert_eq!(
            readiness_gate(Some("gameWrite"), None, &snapshot("i", false, false), "i"),
            ReadinessGate::NotReady
        );
    }

    #[test]
    fn game_write_with_preparation_routes_to_prepare_when_not_ready() {
        // 真实故障场景：进程在跑、截图器未挂上（captureReady=false、inMainUi=false）。
        assert_eq!(
            readiness_gate(
                Some("gameWrite"),
                Some("ensureGameReady"),
                &snapshot("i", false, false),
                "i"
            ),
            ReadinessGate::Prepare
        );
        // 截图器就绪但仍在加载：同样走准备（等到主界面）。
        assert_eq!(
            readiness_gate(
                Some("gameWrite"),
                Some("ensureGameReady"),
                &snapshot("i", true, false),
                "i"
            ),
            ReadinessGate::Prepare
        );
        // 已就绪：直接提交。
        assert_eq!(
            readiness_gate(
                Some("gameWrite"),
                Some("ensureGameReady"),
                &snapshot("i", true, true),
                "i"
            ),
            ReadinessGate::Proceed
        );
    }

    #[test]
    fn read_only_and_configuration_paths_never_trigger_preparation() {
        // 配置写/宿主命令不需要截图，即使声明了 preparation 也不会启动游戏。
        for effect in ["configurationWrite", "hostCommand"] {
            assert_eq!(
                readiness_gate(
                    Some(effect),
                    Some("ensureGameReady"),
                    &snapshot("i", false, false),
                    "i"
                ),
                ReadinessGate::Proceed
            );
        }
        // 只读能力本就不进入该提交通道；防御性地核对它们也不会被判 Prepare。
        assert_eq!(
            readiness_gate(
                Some("readOnly"),
                Some("ensureGameReady"),
                &snapshot("i", false, false),
                "i"
            ),
            ReadinessGate::Proceed
        );
    }

    #[test]
    fn instance_mismatch_is_always_rejected() {
        assert_eq!(
            readiness_gate(
                Some("gameWrite"),
                Some("ensureGameReady"),
                &snapshot("other", true, true),
                "i"
            ),
            ReadinessGate::InstanceMismatch
        );
    }
}

#[cfg(test)]
mod invoke_protocol_tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::runtime::host::catalog::Capability;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// 传输层协议静态测试：本地 HTTP 假服务器实现真实桥协议
    /// （写接口强制 requestId → 返回 Job → GET Job 到终态），
    /// 不加载 native/游戏；来源预检按测试缝跳过，生产路径不受影响。
    #[test]
    fn preparation_follows_the_real_write_protocol_through_invoke() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let ready = Arc::new(AtomicBool::new(false));
            let missing_request_id = Arc::new(AtomicUsize::new(0));
            let invokes_seen = Arc::new(AtomicUsize::new(0));
            {
                let ready = ready.clone();
                let missing_request_id = missing_request_id.clone();
                let invokes_seen = invokes_seen.clone();
                std::thread::spawn(move || {
                    for stream in listener.incoming() {
                        let Ok(mut stream) = stream else { break };
                        let mut head = Vec::new();
                        let mut buffer = [0u8; 8192];
                        loop {
                            let n = stream.read(&mut buffer).unwrap_or(0);
                            if n == 0 { break; }
                            head.extend_from_slice(&buffer[..n]);
                            let text = String::from_utf8_lossy(&head);
                            if let Some(position) = text.find("\r\n\r\n") {
                                let length = text
                                    .lines()
                                    .find_map(|line| {
                                        line.to_ascii_lowercase()
                                            .strip_prefix("content-length:")
                                            .map(|value| value.trim().to_string())
                                    })
                                    .and_then(|value| value.parse::<usize>().ok())
                                    .unwrap_or(0);
                                if head.len() >= position + 4 + length { break; }
                            }
                        }
                        let request = String::from_utf8_lossy(&head).into_owned();
                        let (method, path, body) = parse_request(&request);
                        let response =
                            route(&method, &path, &body, &ready, &missing_request_id, &invokes_seen);
                        let json = serde_json::to_string(&response.1).unwrap();
                        let head = format!(
                            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response.0, response.2, json.len()
                        );
                        let _ = stream.write_all(head.as_bytes());
                        let _ = stream.write_all(json.as_bytes());
                    }
                });
            }

            let base_url = format!("http://127.0.0.1:{port}");
            let bridge = Bridge::for_transport_tests(BridgeConfig {
                enabled: true,
                base_url: base_url.clone(),
                token: Some("test-token".into()),
                instance_id: None,
                timeout_ms: 5_000,
                host_install_path: None,
                launch_silently: true,
            })
            .unwrap();

            let database =
                std::env::temp_dir().join(format!("bridge-proto-{}.db", uuid::Uuid::new_v4()));
            let journal = Arc::new(Journal::open(&database).unwrap());
            let mut run = journal
                .create("跑一下兽怪暴徒", "conv-1", "key", 600, None)
                .unwrap();
            // 生产 invoke 在运行中的 Run 上执行；按真实状态机推进 Queued→Preflighting→Executing。
            journal.save(&mut run, RunState::Preflighting).unwrap();
            journal.save(&mut run, RunState::Executing).unwrap();

            let capability = Capability {
                id: "cap-run".into(),
                description: "运行配置组".into(),
                method_id: "bgi.run_script_groups".into(),
                catalog_version: "v1".into(),
                aliases: vec![],
                resource_fields: vec![],
                postconditions: vec![],
            };
            let mut catalog = crate::runtime::host::catalog::Catalog::default();
            catalog.capabilities.insert("cap-run".into(), capability.clone());

            let authorization_config: AppConfig = serde_json::from_value(serde_json::json!({
                "version": 4, "activeModel": "", "models": [],
                "agent": {"systemPrompt": ""},
                "bridge": {"enabled": true, "baseUrl": base_url, "token": "test-token"},
                "runtime": {"permissionMode": "fullAccess"},
                "storage": {"database": "unused.db"}
            }))
            .unwrap();
            let authorization = std::sync::RwLock::new(authorization_config);

            let policy = crate::runtime::policy::RuntimeConfig {
                permission_mode:
                    crate::runtime::operation::permissions::PermissionMode::FullAccess,
                ..Default::default()
            };

            let arguments = json!({"groupNames":["挖矿讨伐"]});
            let (_, resources) = catalog.resolve("cap-run", &arguments).unwrap();
            let cancel = CancellationToken::new();
            let result = bridge
                .invoke(
                    &journal,
                    &mut run,
                    Invocation {
                        authorization: &authorization,
                        call_id: "call-1",
                        binding: &capability,
                        arguments: &arguments,
                        resources,
                        catalog: &catalog,
                    },
                    &policy,
                    &cancel,
                )
                .await
                .expect("invoke must pass the old gate via the preparation job");

            assert_eq!(result["outcome"], "verifiedSucceeded");
            assert_eq!(
                result["evidence"]["result"]["accepted"], true,
                "main job evidence: {result}"
            );
            assert!(
                ready.load(Ordering::SeqCst),
                "preparation job must have reached terminal state"
            );
            assert_eq!(
                missing_request_id.load(Ordering::SeqCst),
                0,
                "every write invoke must carry requestId"
            );
            assert!(
                invokes_seen.load(Ordering::SeqCst) >= 2,
                "prep + main submissions both observed"
            );
            let attempts = journal.attempts(&run.id).unwrap();
            let prep = attempts
                .iter()
                .find(|a| a.call_id == "call-1#prep")
                .expect("preparation recorded as its own attempt");
            assert_eq!(prep.outcome, "verifiedSucceeded");
            assert_eq!(prep.job_id.as_deref(), Some("job-prep"));
            let _ = std::fs::remove_file(&database);
        });
    }

    fn parse_request(request: &str) -> (String, String, Value) {
        let request_line = request.lines().next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let body = request
            .split("\r\n\r\n")
            .nth(1)
            .filter(|text| !text.is_empty())
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or(Value::Null);
        (method, path, body)
    }

    fn now_rfc3339() -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap()
    }

    fn route(
        method: &str,
        path: &str,
        body: &Value,
        ready: &AtomicBool,
        missing_request_id: &AtomicUsize,
        invokes_seen: &AtomicUsize,
    ) -> (u16, Value, &'static str) {
        let state = |capture: bool| {
            json!({
                "instanceId": "test-instance",
                "snapshotId": format!("s-{}", now_rfc3339()),
                "observedAt": now_rfc3339(),
                "runtime": {
                    "captureReady": capture, "inMainUi": capture,
                    "windowActive": true, "gameHandle": 1
                }
            })
        };
        match (method, path) {
            ("GET", "/bridge/v1/info") => (
                200,
                json!({
                    "protocolVersion": "1", "instanceId": "test-instance",
                    "features": ["jobs", "idempotency"]
                }),
                "OK",
            ),
            ("GET", "/bridge/v1/catalog/bgi.run_script_groups") => (
                200,
                json!({
                    "methodId": "bgi.run_script_groups", "instanceId": "test-instance",
                    "summary": "run groups", "callable": true, "catalogVersion": "v1",
                    "effect": "gameWrite", "preparation": "ensureGameReady",
                    "inputSchema": {"type": "object"}
                }),
                "OK",
            ),
            ("GET", "/bridge/v1/state") => (200, state(ready.load(Ordering::SeqCst)), "OK"),
            ("POST", "/bridge/v1/invoke") => {
                invokes_seen.fetch_add(1, Ordering::SeqCst);
                // 真实桥协议：写接口没有 requestId 直接拒绝。
                if body["requestId"].as_str().is_none_or(str::is_empty) {
                    missing_request_id.fetch_add(1, Ordering::SeqCst);
                    return (
                        400,
                        json!({"code": "INVALID_ARGUMENT", "message": "requestId required"}),
                        "Bad Request",
                    );
                }
                match body["methodId"].as_str() {
                    Some("bgi.ensure_game_ready") => (200, json!({"jobId": "job-prep"}), "OK"),
                    _ => (200, json!({"jobId": "job-run"}), "OK"),
                }
            }
            ("GET", "/bridge/v1/jobs/job-prep") => {
                ready.store(true, Ordering::SeqCst);
                (
                    200,
                    json!({
                        "jobId": "job-prep", "state": "completed",
                        "verification": {"status": "succeeded"},
                        "result": {"prepared": true, "ready": true, "verificationScope": "readiness"}
                    }),
                    "OK",
                )
            }
            ("GET", "/bridge/v1/jobs/job-run") => (
                200,
                json!({
                    "jobId": "job-run", "state": "completed",
                    "verification": {"status": "succeeded"},
                    "result": {"accepted": true, "executionMode": "launch"}
                }),
                "OK",
            ),
            _ => (
                404,
                json!({"code": "NOT_FOUND", "message": path}),
                "Not Found",
            ),
        }
    }
}
