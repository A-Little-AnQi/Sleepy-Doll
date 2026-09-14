//! 确定性执行器：只解释通过验证的修订。
//!
//! 依赖图里没有模型网关 —— 预检、执行、等待、核验、失败分支、取消与重启恢复
//! 全程不调用语言模型；结果文案来自模板与结构化结果。恢复靠已落库的尝试记录：
//! 已核验成功的步骤直接复用证据，不重新提交外部写入；结果未知的步骤保留锁并
//! 转入待核对，绝不换一个新请求键重做。

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    error::{Error, Result},
    model::ToolCall,
    runtime::{
        Supervisor,
        host::bridge::Bridge,
        policy::RuntimeConfig,
        types::{Run, RunState, unix_now},
    },
};

use super::task::{
    FailurePolicy, ForEachNode, RepeatNode, Scope, TaskNode, ToolNode, Truth, WaitNode,
    WorkflowRevision, render_template, resolve_arguments,
};

/// 单个步骤的执行结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Succeeded,
    Failed,
    Skipped,
    Unknown,
}

impl StepResult {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "verifiedSucceeded",
            Self::Failed => "verifiedFailed",
            Self::Skipped => "skipped",
            Self::Unknown => "unknown",
        }
    }
    /// 落库的尝试结果 → 步骤结果。未知的结果不能被当成成功或失败。
    fn from_attempt(outcome: &str) -> Self {
        match outcome {
            "verifiedSucceeded" | "completed" => Self::Succeeded,
            "skipped" => Self::Skipped,
            "unknown" | "running" | "submitting" | "prepared" => Self::Unknown,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
struct NodeReport {
    result: StepResult,
    detail: String,
}

/// 运行期记账。上限来自修订自带的 limits，超限即显式失败。
struct Budget {
    attempts: u32,
    expansions: u32,
    deadline: i64,
    max_attempts: u32,
    max_expansions: u32,
}

impl Budget {
    fn spend_attempt(&mut self) -> Result<()> {
        if self.attempts >= self.max_attempts {
            return Err(Error::Conflict("工具调用次数已达任务上限".into()));
        }
        self.attempts += 1;
        Ok(())
    }
    fn spend_expansion(&mut self, count: u32) -> Result<()> {
        if self.expansions.saturating_add(count) > self.max_expansions {
            return Err(Error::Conflict("循环展开次数已达任务上限".into()));
        }
        self.expansions += count;
        Ok(())
    }
    fn check_time(&self) -> Result<()> {
        if unix_now() > self.deadline {
            return Err(Error::Conflict("任务超过时限".into()));
        }
        Ok(())
    }
}

pub struct TaskExecutor<'a> {
    supervisor: &'a Supervisor,
    bridge: &'a Bridge,
    policy: &'a RuntimeConfig,
    cancel: &'a CancellationToken,
    revision: &'a WorkflowRevision,
    run: &'a mut Run,
    reports: Vec<NodeReport>,
    scope: Scope,
    exposed: HashSet<String>,
    budget: Budget,
    /// 循环作用域键，逐层拼接。重启后同键直接命中已落库的尝试。
    loop_key: String,
}

impl<'a> TaskExecutor<'a> {
    pub fn new(
        supervisor: &'a Supervisor,
        bridge: &'a Bridge,
        policy: &'a RuntimeConfig,
        cancel: &'a CancellationToken,
        revision: &'a WorkflowRevision,
        run: &'a mut Run,
    ) -> Self {
        let deadline = run.deadline.min(unix_now() + revision.limits.max_seconds);
        Self {
            supervisor,
            bridge,
            policy,
            cancel,
            revision,
            run,
            reports: Vec::new(),
            scope: Scope::default(),
            exposed: HashSet::new(),
            budget: Budget {
                attempts: 0,
                expansions: 0,
                deadline,
                max_attempts: revision.limits.max_tool_attempts,
                max_expansions: revision.limits.max_loop_expansions,
            },
            loop_key: String::new(),
        }
    }

    /// 预检只读检查依赖与契约版本，不提交任何外部写入。
    pub async fn preflight(&self) -> Result<()> {
        let definitions = self.supervisor.tool_definitions();
        let mut missing: Vec<String> = Vec::new();
        for node in flatten(&self.revision.nodes) {
            let TaskNode::Tool(tool) = node else {
                continue;
            };
            let Some(name) = tool.tool.as_deref() else {
                return Err(Error::Conflict(format!(
                    "步骤「{}」没有绑定工具",
                    tool.title
                )));
            };
            let Some(definition) = definitions.iter().find(|entry| entry.name == name) else {
                if !missing.iter().any(|entry| entry == name) {
                    missing.push(name.to_owned());
                }
                continue;
            };
            if let (Some(bound), Some(current)) =
                (&tool.provider_version, &definition.provider_version)
                && bound != current
            {
                return Err(Error::Conflict(format!(
                    "步骤「{}」依赖的工具契约已变化，需要重新验证任务",
                    tool.title
                )));
            }
            if let Some(execution) = &tool.execution
                && execution.effect != definition.execution.effect
            {
                return Err(Error::Conflict(format!(
                    "步骤「{}」记录的执行效果与当前工具不一致",
                    tool.title
                )));
            }
        }
        if !missing.is_empty() {
            return Err(Error::Conflict(format!(
                "缺少依赖工具：{}",
                missing.join("、")
            )));
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<RunState> {
        let nodes = self.revision.nodes.clone();
        let mut stopped: Option<String> = None;
        self.walk(&nodes, &mut stopped).await?;
        let tally = |result: StepResult| {
            self.reports
                .iter()
                .filter(|report| report.result == result)
                .count()
        };
        let (failed, unknown, succeeded, skipped) = (
            tally(StepResult::Failed),
            tally(StepResult::Unknown),
            tally(StepResult::Succeeded),
            tally(StepResult::Skipped),
        );
        self.run.result = Some(summary(&self.reports, succeeded, failed, skipped, unknown));
        Ok(if unknown > 0 {
            RunState::NeedsReview
        } else if failed > 0 && succeeded > 0 {
            RunState::Partial
        } else if failed > 0 && succeeded == 0 {
            RunState::Failed
        } else {
            RunState::Succeeded
        })
    }

    async fn walk(&mut self, nodes: &[TaskNode], stopped: &mut Option<String>) -> Result<()> {
        for node in nodes {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            if stopped.is_some() {
                // 前一步没有达到要求时，依赖它的后续步骤一律跳过并如实记账。
                self.report(
                    node.id(),
                    node.title(),
                    StepResult::Skipped,
                    "前面的步骤没有达到要求".into(),
                )?;
                continue;
            }
            self.budget.check_time()?;
            match self.execute(node).await {
                Ok(()) => {}
                Err(error @ Error::Cancelled) => return Err(error),
                Err(error) => {
                    let message = error.user_message();
                    self.fail(node.id(), message.clone())?;
                    self.report(node.id(), node.title(), StepResult::Failed, message)?;
                    match failure_of(node) {
                        // 独立的分支继续跑；整体按部分完成收场。
                        FailurePolicy::ContinueIndependent => continue,
                        // 补偿是新的一次操作，可能同样失败；如实记录，不假造回滚。
                        FailurePolicy::Compensate {
                            tool: Some(tool), ..
                        } if !tool.trim().is_empty() => {
                            self.compensate(&tool).await?;
                            *stopped = Some(node.id().to_owned());
                        }
                        _ => *stopped = Some(node.id().to_owned()),
                    }
                }
            }
        }
        Ok(())
    }

    async fn execute(&mut self, node: &TaskNode) -> Result<()> {
        self.supervisor.journal.emit(
            self.run,
            "step.started",
            json!({"id":node.id(),"title":node.title(),"kind":node.kind_label()}),
        )?;
        match node {
            TaskNode::Tool(tool) => self.tool(tool).await,
            TaskNode::Sequence(sequence) => {
                let mut stopped = None;
                Box::pin(self.walk(&sequence.nodes, &mut stopped)).await
            }
            TaskNode::Condition(condition) => {
                let truth = condition.condition.evaluate(&self.scope);
                let branch = match truth {
                    Truth::True => &condition.then,
                    Truth::False => &condition.otherwise,
                    Truth::Unknown => &condition.unknown,
                };
                let label = match truth {
                    Truth::True => "条件成立",
                    Truth::False => "条件不成立",
                    Truth::Unknown => "缺少字段，按未确定分支处理",
                };
                self.report(node.id(), node.title(), StepResult::Succeeded, label.into())?;
                let mut stopped = None;
                Box::pin(self.walk(branch, &mut stopped)).await
            }
            TaskNode::ForEach(each) => self.for_each(each).await,
            TaskNode::Repeat(repeat) => self.repeat(repeat).await,
            TaskNode::Wait(wait) => self.wait(wait).await,
            TaskNode::Result(result) => {
                let (rendered, missing) = render_template(&result.template, &self.scope);
                let values = result
                    .outputs
                    .iter()
                    .filter_map(|reference| reference.resolve(&self.scope))
                    .collect::<Vec<_>>();
                let mut output = json!({"text":rendered,"values":values});
                if !missing.is_empty() {
                    output["missing"] = json!(missing);
                }
                self.scope.record(node.id(), output);
                self.report(
                    node.id(),
                    node.title(),
                    StepResult::Succeeded,
                    format!("结果：{rendered}"),
                )
            }
        }
    }

    async fn tool(&mut self, node: &ToolNode) -> Result<()> {
        let name = node
            .tool
            .clone()
            .ok_or_else(|| Error::Conflict("步骤没有绑定工具".into()))?;
        let arguments = match resolve_arguments(&node.arguments, &self.scope) {
            Ok(arguments) => arguments,
            Err(missing) => {
                return Err(Error::Conflict(format!("步骤「{}」{missing}", node.title)));
            }
        };
        let key = self.loop_key.clone();
        if let Some((result, output)) = self.replay(&node.id, &key)? {
            if result == StepResult::Succeeded {
                self.scope.record(&node.id, output);
                return self.report(
                    &node.id,
                    &node.title,
                    StepResult::Succeeded,
                    "使用本次运行已核验的结果".into(),
                );
            }
            // 已有未成功的尝试：不再重放，交给上层按未知/失败收场。
            return Err(match result {
                StepResult::Unknown => {
                    Error::Conflict("该步骤先前的结果未确认，已保留执行锁等待核对".into())
                }
                _ => Error::Conflict("该步骤先前未成功".into()),
            });
        }
        let attempts = match &node.on_failure {
            FailurePolicy::BoundedRetry { max_attempts, .. } => (*max_attempts).max(1),
            _ => 1,
        };
        let mut last: Option<Error> = None;
        for attempt in 1..=attempts {
            self.budget.spend_attempt()?;
            match self
                .invoke(&node.id, &name, arguments.clone(), &key, attempt)
                .await
            {
                Ok(output) => {
                    let result = verification_of(&output);
                    self.scope.record(&node.id, output);
                    return self.report(&node.id, &node.title, result, detail_of(result));
                }
                Err(error @ Error::Cancelled) => return Err(error),
                // 外部影响未知：不换请求键重做，也不释放锁。
                Err(error) if is_unconfirmed(&error) => return Err(error),
                Err(error) => {
                    last = Some(error);
                    if attempt < attempts
                        && let FailurePolicy::BoundedRetry { backoff_ms, .. } = &node.on_failure
                    {
                        self.backoff(*backoff_ms).await?;
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| Error::Tool("工具调用失败".into())))
    }

    async fn for_each(&mut self, node: &ForEachNode) -> Result<()> {
        let items = node
            .items
            .resolve(&self.scope)
            .ok_or_else(|| Error::Conflict(format!("步骤「{}」取不到要遍历的列表", node.title)))?;
        let Value::Array(items) = items else {
            return Err(Error::Conflict(format!(
                "步骤「{}」要遍历的不是列表",
                node.title
            )));
        };
        if items.is_empty() {
            // 空列表合法完成，并说明没有目标。
            return self.report(
                &node.id,
                &node.title,
                StepResult::Succeeded,
                "列表为空，没有需要处理的目标".into(),
            );
        }
        if items.len() > node.max_items {
            return Err(Error::Conflict(format!(
                "步骤「{}」有 {} 项，超过上限 {}",
                node.title,
                items.len(),
                node.max_items
            )));
        }
        self.budget.spend_expansion(items.len() as u32)?;
        let outer = self.loop_key.clone();
        for (index, item) in items.into_iter().enumerate() {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.budget.check_time()?;
            let key = item
                .get(&node.item_key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| index.to_string());
            self.loop_key = format!("{outer}/{key}");
            let scoped = self.scope.with_item(item, index as u32 + 1);
            let previous = std::mem::replace(&mut self.scope, scoped);
            let mut stopped = None;
            let result = Box::pin(self.walk(&node.nodes, &mut stopped)).await;
            // 循环变量是局部作用域，退出体后必须还原。
            self.scope = previous;
            if let Err(error) = result {
                self.loop_key = outer;
                return Err(error);
            }
        }
        self.loop_key = outer;
        self.report(
            &node.id,
            &node.title,
            StepResult::Succeeded,
            "已逐项处理".into(),
        )
    }

    async fn repeat(&mut self, node: &RepeatNode) -> Result<()> {
        let mut iteration: u32 = 0;
        let limit = node
            .count
            .unwrap_or(node.max_iterations)
            .min(node.max_iterations);
        self.budget.spend_expansion(limit)?;
        let outer = self.loop_key.clone();
        let mut stopped: Option<String> = None;
        while iteration < limit {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            self.budget.check_time()?;
            iteration += 1;
            self.loop_key = format!("{outer}#{iteration}");
            let scoped = self.scope.with_item(Value::Null, iteration);
            let previous = std::mem::replace(&mut self.scope, scoped);
            let result = Box::pin(self.walk(&node.nodes, &mut stopped)).await;
            self.scope = previous;
            if let Err(error) = result {
                self.loop_key = outer;
                return Err(error);
            }
            if stopped.is_some() {
                break;
            }
            if let Some(until) = &node.until
                && until.evaluate(&self.scope) == Truth::True
            {
                break;
            }
        }
        self.loop_key = outer;
        self.report(
            &node.id,
            &node.title,
            StepResult::Succeeded,
            format!("已执行 {iteration} 次"),
        )
    }

    async fn wait(&mut self, node: &WaitNode) -> Result<()> {
        let key = self.loop_key.clone();
        let attempt_id = format!("wait-{}", node.id);
        // 期限在第一次进入时落库：重启后按原期限继续，不重新计时。
        let deadline = match self.replay(&attempt_id, &key)? {
            Some((StepResult::Succeeded, _)) => return Ok(()),
            Some((_, evidence)) => evidence
                .get("deadline")
                .and_then(Value::as_i64)
                .unwrap_or_else(unix_now),
            None => {
                let seconds = node
                    .seconds
                    .or(node.timeout_seconds)
                    .unwrap_or(node.check_seconds);
                let deadline = unix_now() + seconds as i64;
                let attempt = self.supervisor.journal.prepare(
                    self.run,
                    &attempt_id,
                    json!({"stepId":node.id,"iterationKey":key,"kind":"wait","deadline":deadline}),
                    "-",
                )?;
                let mut attempt = attempt;
                attempt.evidence = json!({"deadline":deadline});
                self.supervisor.journal.attempt(&attempt)?;
                deadline
            }
        };
        loop {
            if self.cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let now = unix_now();
            if now >= deadline {
                break;
            }
            if let Some(until) = &node.until {
                let truth = match &node.probe {
                    Some(probe) => {
                        self.budget.spend_attempt()?;
                        let name = probe.tool.clone().ok_or_else(|| {
                            Error::Conflict("条件等待缺少可执行的检查步骤".into())
                        })?;
                        let arguments = resolve_arguments(&probe.arguments, &self.scope)
                            .map_err(Error::Conflict)?;
                        match self.invoke(&probe.id, &name, arguments, &key, 1).await {
                            Ok(output) => {
                                self.scope.record(&probe.id, output);
                                until.evaluate(&self.scope)
                            }
                            Err(error @ Error::Cancelled) => return Err(error),
                            Err(_) => Truth::Unknown,
                        }
                    }
                    None => until.evaluate(&self.scope),
                };
                if truth == Truth::True {
                    self.close_wait(&attempt_id)?;
                    return self.report(
                        &node.id,
                        &node.title,
                        StepResult::Succeeded,
                        "等待的条件已满足".into(),
                    );
                }
            }
            let remaining = (deadline - now).max(1) as u64;
            self.sleep(node.check_seconds.min(remaining)).await?;
        }
        if node.until.is_some() && node.seconds.is_none() {
            // 条件一直不满足：有限轮询后给出等待超时，不无限循环，也不去问模型。
            return Err(Error::Conflict("等待的条件在期限内没有满足".into()));
        }
        self.close_wait(&attempt_id)?;
        self.report(
            &node.id,
            &node.title,
            StepResult::Succeeded,
            "等待结束".into(),
        )
    }

    fn close_wait(&self, attempt_id: &str) -> Result<()> {
        if let Some(mut attempt) = self
            .supervisor
            .journal
            .attempt_by_call(&self.run.id, attempt_id)?
        {
            attempt.outcome = "verifiedSucceeded".into();
            self.supervisor.journal.attempt(&attempt)?;
        }
        Ok(())
    }

    /// 供调用方在预检失败时写回状态，避免执行器与运行对象被重复借用。
    pub fn run_mut(&mut self) -> &mut Run {
        self.run
    }

    /// 中止时是否已有结果未知的步骤：决定收场是待核对还是失败。
    pub fn reports_have_unknown(&self) -> bool {
        self.reports
            .iter()
            .any(|report| report.result == StepResult::Unknown)
    }

    /// 已落库的尝试：核验成功复用证据，其余不重放。
    fn replay(&self, node_id: &str, key: &str) -> Result<Option<(StepResult, Value)>> {
        for attempt in self.supervisor.journal.attempts(&self.run.id)? {
            if attempt.request["stepId"] != json!(node_id)
                || attempt.request["iterationKey"] != json!(key)
            {
                continue;
            }
            let result = StepResult::from_attempt(&attempt.outcome);
            return Ok(Some((result, attempt.evidence.clone())));
        }
        Ok(None)
    }

    async fn invoke(
        &mut self,
        node_id: &str,
        name: &str,
        arguments: Value,
        key: &str,
        attempt: u32,
    ) -> Result<Value> {
        // 运行记账要反映真实发生的调用，否则「这次跑了多少步」只能靠猜。
        self.run.tool_calls += 1;
        let call = ToolCall {
            id: format!("task-{}-{}-{}-{}", self.run.id, node_id, key, attempt),
            name: name.to_owned(),
            arguments,
        };
        let supervisor = self.supervisor;
        let bridge = self.bridge;
        let policy = self.policy;
        let cancel = self.cancel;
        supervisor
            .tool(self.run, &call, bridge, policy, cancel, &mut self.exposed)
            .await
    }

    fn report(&mut self, id: &str, title: &str, result: StepResult, detail: String) -> Result<()> {
        self.reports.push(NodeReport {
            result,
            detail: detail.clone(),
        });
        self.supervisor.journal.emit(
            self.run,
            "step.finished",
            json!({"id":id,"title":title,"outcome":result.as_str(),"detail":detail}),
        )
    }

    fn fail(&self, id: &str, message: String) -> Result<()> {
        self.supervisor
            .journal
            .emit(self.run, "step.failed", json!({"id":id,"error":message}))
    }

    /// 补偿是一次新的操作，也可能失败；失败按未知收场，不宣称已恢复。
    async fn compensate(&mut self, tool: &str) -> Result<()> {
        self.supervisor.journal.emit(
            self.run,
            "step.started",
            json!({"id":format!("compensate:{tool}"),"title":"补偿","kind":"补偿"}),
        )?;
        let key = format!("{}/compensate", self.loop_key);
        let name = tool.to_owned();
        self.budget.spend_attempt()?;
        match self
            .invoke(&format!("compensate:{tool}"), &name, json!({}), &key, 1)
            .await
        {
            Ok(_) => self.report(
                &format!("compensate:{tool}"),
                "补偿",
                StepResult::Succeeded,
                "补偿操作已完成".into(),
            ),
            Err(Error::Cancelled) => Err(Error::Cancelled),
            Err(error) => self.report(
                &format!("compensate:{tool}"),
                "补偿",
                StepResult::Unknown,
                format!("补偿未完成：{}", error.user_message()),
            ),
        }
    }

    async fn sleep(&self, seconds: u64) -> Result<()> {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(seconds.max(1))) => Ok(()),
            _ = self.cancel.cancelled() => Err(Error::Cancelled),
        }
    }

    async fn backoff(&self, ms: u64) -> Result<()> {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(ms)) => Ok(()),
            _ = self.cancel.cancelled() => Err(Error::Cancelled),
        }
    }
}

/// 只有「外部影响或停止状态未确认」这一种冲突不重试。
fn is_unconfirmed(error: &Error) -> bool {
    matches!(error, Error::Conflict(message) if message.contains("未确认"))
}

fn failure_of(node: &TaskNode) -> FailurePolicy {
    match node {
        TaskNode::Tool(tool) => tool.on_failure.clone(),
        _ => FailurePolicy::Stop,
    }
}

/// 工具返回带业务核验结论时以它为准；没有结论的只读结果按成功计。
fn verification_of(output: &Value) -> StepResult {
    match output
        .pointer("/verification/status")
        .and_then(Value::as_str)
    {
        Some("succeeded") => StepResult::Succeeded,
        Some("failed") => StepResult::Failed,
        Some(_) => StepResult::Unknown,
        None => StepResult::Succeeded,
    }
}

fn detail_of(result: StepResult) -> String {
    match result {
        StepResult::Succeeded => "已核验".into(),
        StepResult::Failed => "工具报告核验失败".into(),
        StepResult::Unknown => "结果未确认".into(),
        StepResult::Skipped => "已跳过".into(),
    }
}

fn summary(
    reports: &[NodeReport],
    succeeded: usize,
    failed: usize,
    skipped: usize,
    unknown: usize,
) -> String {
    let mut text = format!("已完成 {succeeded} 个步骤");
    if failed > 0 {
        text.push_str(&format!("，{failed} 个失败"));
    }
    if skipped > 0 {
        text.push_str(&format!("，{skipped} 个跳过"));
    }
    if unknown > 0 {
        text.push_str(&format!("，{unknown} 个结果待核对"));
    }
    if let Some(report) = reports
        .iter()
        .rev()
        .find(|report| report.detail.starts_with("结果："))
    {
        text.push('\n');
        text.push_str(&report.detail);
    }
    text
}

/// 编译期可达的全部节点，用于预检与依赖盘点。
pub fn flatten(nodes: &[TaskNode]) -> Vec<&TaskNode> {
    let mut flat = Vec::new();
    collect(nodes, &mut flat);
    flat
}

fn collect<'a>(nodes: &'a [TaskNode], flat: &mut Vec<&'a TaskNode>) {
    for node in nodes {
        flat.push(node);
        match node {
            TaskNode::Sequence(node) => collect(&node.nodes, flat),
            TaskNode::Condition(node) => {
                collect(&node.then, flat);
                collect(&node.otherwise, flat);
                collect(&node.unknown, flat);
            }
            TaskNode::ForEach(node) => collect(&node.nodes, flat),
            TaskNode::Repeat(node) => collect(&node.nodes, flat),
            TaskNode::Tool(_) | TaskNode::Wait(_) | TaskNode::Result(_) => {}
        }
    }
}

#[allow(dead_code)]
fn unused(_: &HashMap<String, Value>) {}
