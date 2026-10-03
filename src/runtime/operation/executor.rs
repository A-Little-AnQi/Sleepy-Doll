//! 确定性执行器：只解释通过验证的修订。
//!
//! 全程不调用语言模型；结果文案来自模板与结构化结果。恢复按已落库的尝试记录
//! 进行：核验成功的步骤复用证据，未确认的步骤保留锁并转入待核对。

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
    /// 落库的尝试结果转为步骤结果；未确认的结果归为 unknown。
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
    /// 该步骤是 launch 交接（宿主后台继续执行）还是同步完成。
    submitted: bool,
}

/// 运行期记账：调用次数、循环展开与期限，上限来自修订自带的 limits。
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
    /// 循环作用域键，逐层拼接；重启后同键命中已落库的尝试。
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

    /// 预检：检查依赖与契约版本，不做任何外部写入。
    pub async fn preflight(&self) -> Result<()> {
        let definitions = self.supervisor.tool_definitions();
        let mut missing: Vec<String> = Vec::new();
        for node in flatten(&self.revision.nodes) {
            let tool = match node {
                TaskNode::Tool(tool) => tool,
                TaskNode::Wait(wait) => match &wait.probe {
                    Some(probe) => probe,
                    None => continue,
                },
                _ => continue,
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
            return Err(Error::Conflict(
                crate::extension::providers::missing_plugin_issue(
                    &missing,
                    &self.supervisor.introduced_providers(),
                ),
            ));
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<RunState> {
        let nodes = self.revision.nodes.clone();
        let mut stopped: Option<String> = None;
        self.walk(&nodes, &mut stopped).await?;
        if self.revision.shortcut.is_some() {
            let (state, error, result) = shortcut_outcome(
                &self.reports,
                self.revision.nodes.len(),
                &self.revision.name,
            );
            self.run.error = error;
            self.run.result = Some(result);
            return Ok(state);
        }
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
                // 前一步未达要求，后续步骤跳过并记账。
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
                Ok(()) => {
                    if self.revision.shortcut.is_some()
                        && self
                            .reports
                            .last()
                            .is_some_and(|report| report.result != StepResult::Succeeded)
                    {
                        *stopped = Some(node.id().to_owned());
                    }
                }
                Err(error @ Error::Cancelled) => return Err(error),
                Err(error) => {
                    let message = error.user_message();
                    self.fail(node.id(), message.clone())?;
                    self.report(node.id(), node.title(), StepResult::Failed, message)?;
                    match failure_of(node) {
                        // 独立分支继续执行。
                        FailurePolicy::ContinueIndependent => continue,
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
            // 已有未成功的尝试：不再重放。
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
                    // launch 交接只代表宿主接了计划：卡片写「已提交」，不当
                    // 同步完成；其余步骤按各自核验结论表述。
                    let submitted = result == StepResult::Succeeded && launch_submitted(&output);
                    let detail = if submitted {
                        format!("已提交「{}」运行", node.title)
                    } else {
                        detail_of(result)
                    };
                    self.scope.record(&node.id, output);
                    return if submitted {
                        self.report_submitted(&node.id, &node.title, result, detail)
                    } else {
                        self.report(&node.id, &node.title, result, detail)
                    };
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
            // 空列表按合法完成处理。
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
            let nested = std::mem::replace(&mut self.scope, previous);
            self.scope.retain_outputs_from(&nested);
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
            let reached = node
                .until
                .as_ref()
                .is_some_and(|until| until.evaluate(&self.scope) == Truth::True);
            let nested = std::mem::replace(&mut self.scope, previous);
            self.scope.retain_outputs_from(&nested);
            if let Err(error) = result {
                self.loop_key = outer;
                return Err(error);
            }
            if stopped.is_some() {
                break;
            }
            if reached {
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
        // 期限在第一次进入时落库；重启后按原期限继续。
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
            // 条件一直不满足：按等待超时结束。
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

    /// 预检失败时供调用方写回运行状态。
    pub fn run_mut(&mut self) -> &mut Run {
        self.run
    }

    /// 是否已有结果未知的步骤。
    pub fn reports_have_unknown(&self) -> bool {
        self.reports
            .iter()
            .any(|report| report.result == StepResult::Unknown)
    }

    /// 已落库的尝试：核验成功的复用证据，其余不重放。
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
        self.push_report(id, title, result, detail, false)
    }

    /// launch 交接步骤的记账：与同步完成区分，收尾聚合据此写「已提交」。
    fn report_submitted(
        &mut self,
        id: &str,
        title: &str,
        result: StepResult,
        detail: String,
    ) -> Result<()> {
        self.push_report(id, title, result, detail, true)
    }

    fn push_report(
        &mut self,
        id: &str,
        title: &str,
        result: StepResult,
        detail: String,
        submitted: bool,
    ) -> Result<()> {
        self.reports.push(NodeReport {
            result,
            detail: detail.clone(),
            submitted,
        });
        self.supervisor.journal.emit(
            self.run,
            "step.finished",
            json!({"id":id,"title":title,"outcome":result.as_str(),"detail":detail,"submitted":submitted}),
        )
    }

    fn fail(&self, id: &str, message: String) -> Result<()> {
        self.supervisor
            .journal
            .emit(self.run, "step.failed", json!({"id":id,"error":message}))
    }

    /// 执行补偿步骤；失败按未知收场。
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

/// 是否为外部影响或停止状态未确认的冲突。
fn is_unconfirmed(error: &Error) -> bool {
    matches!(error, Error::Conflict(message) if message.contains("未确认"))
}

/// 快捷任务的收尾状态：任何未确认都进待核对，只有全部步骤成功才算完成；
/// 停链后未执行的步骤记为跳过，不能抵消已发生的失败。
fn shortcut_outcome(
    reports: &[NodeReport],
    total_nodes: usize,
    name: &str,
) -> (RunState, Option<String>, String) {
    // 只要有一步是 launch 交接，整体成功也只能表述为「已提交」：宿主还在
    // 后台执行，不能宣称业务已完成。
    let submitted = reports.iter().any(|report| report.submitted);
    let state = if reports
        .iter()
        .any(|report| report.result == StepResult::Unknown)
    {
        RunState::NeedsReview
    } else if reports.len() == total_nodes
        && reports
            .iter()
            .all(|report| report.result == StepResult::Succeeded)
    {
        RunState::Succeeded
    } else {
        RunState::Failed
    };
    let error = reports
        .iter()
        .find(|report| matches!(report.result, StepResult::Failed | StepResult::Unknown))
        .map(|report| report.detail.clone());
    let result = match state {
        RunState::Succeeded if submitted => format!("已提交「{name}」运行"),
        RunState::Succeeded => format!("已完成「{name}」"),
        RunState::NeedsReview => format!("「{name}」的运行结果需要核对"),
        _ => format!("未能运行「{name}」"),
    };
    (state, error, result)
}

/// 取工具返回的业务核验结论。
///
/// 桥接调用返回 `{jobId, outcome, evidence}` 包装：`outcome` 是宿主对整体
/// 执行的判定，`evidence.verification/status` 是任务级核验，
/// `evidence.result.verification.status` 可能是组合进度判定。规则：只有
/// 明确成功算成功，明确失败算失败，其余任何已存在的核验状态一律未知；
/// 失败优先于未知，未知优先于成功；没有任何核验元数据的普通只读结果仍按
/// 成功计。包装识别看 `jobId`，或「桥 outcome + evidence 字段」同时存在：
/// 证据丢失或不是对象时按未知处理，不被默认成功吞掉。恰好叫 outcome 的
/// 业务字段（如 `bgi.wait_ready` 的 `timeout`）没有包装字段，不参与判定。
fn verification_of(output: &Value) -> StepResult {
    let mut statuses: Vec<&str> = Vec::new();
    if let Some(status) = output
        .pointer("/verification/status")
        .and_then(Value::as_str)
    {
        statuses.push(normalize_status(status));
    }
    let envelope = output.get("jobId").is_some()
        || (output.get("outcome").is_some() && output.get("evidence").is_some());
    if envelope {
        if let Some(outcome) = output.get("outcome").and_then(Value::as_str) {
            statuses.push(normalize_status(outcome));
        }
        if let Some(evidence) = output.get("evidence").filter(|value| value.is_object()) {
            for pointer in ["/verification/status", "/result/verification/status"] {
                if let Some(status) = evidence.pointer(pointer).and_then(Value::as_str) {
                    statuses.push(normalize_status(status));
                }
            }
        } else {
            // 有包装元数据但证据丢失或不可读：不能默认成功。
            statuses.push("unknown");
        }
    }
    if statuses.is_empty() {
        return StepResult::Succeeded;
    }
    if statuses.contains(&"failed") {
        StepResult::Failed
    } else if statuses.contains(&"unknown") {
        StepResult::Unknown
    } else {
        StepResult::Succeeded
    }
}

/// 核验状态规范化：只有明确成功算成功，明确失败算失败，其余一律未知。
fn normalize_status(status: &str) -> &'static str {
    match status {
        "succeeded" | "verifiedSucceeded" => "succeeded",
        "failed" | "verifiedFailed" | "cancelled" => "failed",
        _ => "unknown",
    }
}

/// 桥结果是否为明确成功的仅启动交接（launch scope）。先过 `verification_of`
/// 聚合核验：任何一层 failed/unknown 都不算交接成功，不允许未知结果终结。
/// 新批量契约的核验在 `result.verification`；旧单组契约没有该对象，但 Job
/// 证据 `verification.status=succeeded` 加 `result.verified=true` 同样能证明
/// 交接，不因缺新增字段而拒绝。
pub(crate) fn launch_submitted(output: &Value) -> bool {
    if verification_of(output) != StepResult::Succeeded {
        return false;
    }
    let Some(result) = output
        .pointer("/evidence/result")
        .filter(|value| value.is_object())
    else {
        return false;
    };
    if result.get("accepted").and_then(Value::as_bool) != Some(true)
        || result.get("executionMode").and_then(Value::as_str) != Some("launch")
        || result.get("verificationScope").and_then(Value::as_str) != Some("launch")
    {
        return false;
    }
    result
        .pointer("/verification/status")
        .and_then(Value::as_str)
        == Some("succeeded")
        || (result.get("verified").and_then(Value::as_bool) == Some(true)
            && output
                .pointer("/evidence/verification/status")
                .and_then(Value::as_str)
                == Some("succeeded"))
}

fn failure_of(node: &TaskNode) -> FailurePolicy {
    match node {
        TaskNode::Tool(tool) => tool.on_failure.clone(),
        _ => FailurePolicy::Stop,
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
    // 同步步骤保持「已完成」；只有真实 launch 交接的步骤单独按提交计数，
    // 不把整批说成已提交。
    let submitted = reports.iter().filter(|report| report.submitted).count();
    let mut text = if submitted > 0 {
        format!("已完成 {succeeded} 个步骤，其中 {submitted} 个为后台提交")
    } else {
        format!("已完成 {succeeded} 个步骤")
    };
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

/// 编译期可达的全部节点。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn report(result: StepResult) -> NodeReport {
        NodeReport {
            result,
            detail: match result {
                StepResult::Failed => "工具报告核验失败".into(),
                StepResult::Unknown => "结果未确认".into(),
                _ => "已核验".into(),
            },
            submitted: false,
        }
    }

    #[test]
    fn verification_reads_bridge_envelope_and_nests() {
        // 成功包装：宿主判定成功且任务级核验成功。
        let ok = json!({"jobId":"j1","outcome":"verifiedSucceeded","evidence":{"verification":{"status":"succeeded"}}});
        assert_eq!(verification_of(&ok), StepResult::Succeeded);
        // 明确失败不被嵌套成功覆盖。
        let failed = json!({"jobId":"j2","outcome":"verifiedFailed","evidence":{"verification":{"status":"failed"},"result":{"verification":{"status":"succeeded"}}}});
        assert_eq!(verification_of(&failed), StepResult::Failed);
        // 组合进度判定失败优于外层未知。
        let progress_failed = json!({"jobId":"j3","outcome":"unknown","evidence":{"verification":{"status":"unknown"},"result":{"verification":{"status":"failed"}}}});
        assert_eq!(verification_of(&progress_failed), StepResult::Failed);
        // 旧单组 completion 未核验：整体未知，不许按成功收尾。
        let unverified = json!({"jobId":"j4","outcome":"unknown","evidence":{"verification":{"status":"unknown"},"verified":false}});
        assert_eq!(verification_of(&unverified), StepResult::Unknown);
        // cancelled 是明确终止，不是成功。
        let cancelled = json!({"jobId":"j5","outcome":"cancelled","evidence":{"verification":{"status":"succeeded"}}});
        assert_eq!(verification_of(&cancelled), StepResult::Failed);
        // 未识别的核验状态（running、notSubmitted、garbage）一律未知，
        // 未知优先于成功。
        assert_eq!(
            verification_of(&json!({"verification":{"status":"running"}})),
            StepResult::Unknown
        );
        assert_eq!(
            verification_of(&json!({"verification":{"status":"notSubmitted"}})),
            StepResult::Unknown
        );
        assert_eq!(
            verification_of(&json!({"verification":{"status":"garbage"}})),
            StepResult::Unknown
        );
        assert_eq!(
            verification_of(
                &json!({"jobId":"j6","outcome":"verifiedSucceeded","evidence":{"verification":{"status":"running"}}})
            ),
            StepResult::Unknown
        );
        assert_eq!(
            verification_of(
                &json!({"jobId":"j7","outcome":"notSubmitted","evidence":{"verification":{"status":"succeeded"}}})
            ),
            StepResult::Unknown
        );
        // 包装在但证据丢失或不是对象：按未知，不被默认成功吞掉。
        assert_eq!(
            verification_of(&json!({"jobId":"j8","outcome":"verifiedSucceeded","evidence":"lost"})),
            StepResult::Unknown
        );
        assert_eq!(
            verification_of(&json!({"jobId":"j9","outcome":"verifiedSucceeded"})),
            StepResult::Unknown
        );
        // 证据缺失时明确失败仍然优先。
        assert_eq!(
            verification_of(&json!({"jobId":"j10","outcome":"verifiedFailed","evidence":null})),
            StepResult::Failed
        );
        // 普通只读结果没有核验元数据：保留默认成功。
        assert_eq!(
            verification_of(&json!({"path":"a.txt","text":"hi"})),
            StepResult::Succeeded
        );
        // 业务字段恰好叫 outcome 但没有 jobId/evidence 包装：不当核验结论。
        assert_eq!(
            verification_of(&json!({"outcome":"timeout"})),
            StepResult::Succeeded
        );
        // 顶层 verification 仍然生效。
        assert_eq!(
            verification_of(&json!({"verification":{"status":"succeeded"}})),
            StepResult::Succeeded
        );
        assert_eq!(
            verification_of(&json!({"verification":{"status":"failed"}})),
            StepResult::Failed
        );
    }

    /// 仅验证收尾结果聚合；步骤是否真的没有执行由
    /// bgi-bridge/dev/task-workflow-check.py 的真实驱动集成用例负责。
    #[test]
    fn shortcut_outcome_aggregates_failure_unknown_and_incomplete() {
        // 第二步失败后第三步不执行（记为跳过）：整体失败，错误取失败步骤。
        let (state, error, _) = shortcut_outcome(
            &[
                report(StepResult::Succeeded),
                report(StepResult::Failed),
                report(StepResult::Skipped),
            ],
            3,
            "挖矿讨伐",
        );
        assert_eq!(state, RunState::Failed);
        assert_eq!(error.as_deref(), Some("工具报告核验失败"));
        // 未确认的步骤进入待核对，不能按成功收尾。
        let (state, error, _) = shortcut_outcome(
            &[
                report(StepResult::Succeeded),
                report(StepResult::Unknown),
                report(StepResult::Skipped),
            ],
            3,
            "挖矿讨伐",
        );
        assert_eq!(state, RunState::NeedsReview);
        assert_eq!(error.as_deref(), Some("结果未确认"));
        // 全部成功且没有缺漏节点才算完成。
        let (state, _, _) = shortcut_outcome(
            &[report(StepResult::Succeeded), report(StepResult::Succeeded)],
            2,
            "t",
        );
        assert_eq!(state, RunState::Succeeded);
        // 有步骤根本没跑到（不是跳过记账）：不算完成。
        let (state, _, _) = shortcut_outcome(&[report(StepResult::Succeeded)], 2, "t");
        assert_eq!(state, RunState::Failed);
    }

    fn launch_output(outcome: &str, result: Value) -> Value {
        json!({"jobId":"job-1","outcome":outcome,"evidence":{"result":result}})
    }

    #[test]
    fn launch_submitted_reads_batch_contract() {
        // 新批量契约：launch 核验在 result.verification。
        let output = launch_output(
            "verifiedSucceeded",
            json!({"accepted":true,"executionMode":"launch","verificationScope":"launch","verification":{"status":"succeeded"}}),
        );
        assert!(launch_submitted(&output));
    }

    #[test]
    fn launch_submitted_reads_legacy_single_group() {
        // 旧单组契约没有 result.verification；Job 级核验成功 + result.verified
        // 同样证明交接。
        let mut output = launch_output(
            "verifiedSucceeded",
            json!({"accepted":true,"executed":true,"executionMode":"launch","verificationScope":"launch","verified":true}),
        );
        output["evidence"]["verification"] = json!({"status":"succeeded"});
        assert!(launch_submitted(&output));
        // 缺 Job 级核验：不能算交接成功。
        let unverifiable = json!({"jobId":"job-2","outcome":"unknown","evidence":{"result":json!({"accepted":true,"executionMode":"launch","verificationScope":"launch","verified":true})}});
        assert!(!launch_submitted(&unverifiable));
    }

    #[test]
    fn launch_submitted_rejects_unknown_and_failed() {
        // 外层核验未确认：不许终结。
        let mut output = launch_output(
            "unknown",
            json!({"accepted":true,"executionMode":"launch","verificationScope":"launch","verification":{"status":"succeeded"}}),
        );
        output["evidence"]["verification"] = json!({"status":"unknown"});
        assert!(!launch_submitted(&output));
        // 任何一层明确失败：不许终结。
        let failed = json!({"jobId":"job-3","outcome":"verifiedFailed","evidence":{"result":json!({"accepted":true,"executionMode":"launch","verificationScope":"launch","verification":{"status":"failed"}})}});
        assert!(!launch_submitted(&failed));
        // executionMode 不是 launch（completion 显式等待）：不走交接终结。
        let completion = launch_output(
            "verifiedSucceeded",
            json!({"accepted":true,"executionMode":"completion","verificationScope":"schedulerProgress","verification":{"status":"failed"}}),
        );
        assert!(!launch_submitted(&completion));
    }
}
