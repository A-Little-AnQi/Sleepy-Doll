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

/// Bridge 观测可以作为执行依据的最大年龄（秒）。
const OBSERVATION_MAX_AGE_SEC: i64 = 15;

#[derive(Clone)]
pub struct Bridge {
    config: BridgeConfig,
    client: reqwest::Client,
    /// Job 事件流专用：用空闲超时，不用总超时。
    stream_client: reqwest::Client,
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
        if plan.is_none()
            && attempts
                .iter()
                .any(|a| a.request["capabilityId"] == binding.id && a.request["arguments"] == *args)
        {
            return Err(Error::Tool(
                "该动作已有执行记录，需要核对结果，不能直接重复提交".into(),
            ));
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
        let decision = crate::runtime::operation::permissions::PermissionEngine::decide(
            policy.permission_mode,
            &current_permission,
            &policy.trust_grants,
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
        let snapshot = self.get("/bridge/v1/state", cancel).await?;
        let requires_capture = !matches!(
            descriptor["effect"].as_str(),
            Some("configurationWrite" | "hostCommand")
        );
        if snapshot["instanceId"] != instance
            || (requires_capture && snapshot["runtime"]["captureReady"] != true)
        {
            return Err(Error::Tool(
                "游戏尚未就绪：截图器未启动或游戏窗口未打开。先调用 bgi.start_game 启动原神，用 bgi.get_status 等到 ready=true，再重试本次调用；不要把启动这一步交回用户。"
                    .into(),
            ));
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
            if fresh["instanceId"] != instance
                || (requires_capture && fresh["runtime"]["captureReady"] != true)
            {
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
        let wire = json!({"requestId":a.id,"instanceId":instance,"catalogVersion":version,"methodId":id,"arguments":args,"execution":a.request["execution"]});
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
