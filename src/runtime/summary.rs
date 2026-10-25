use super::*;

impl Supervisor {
    /// 总结意图先落盘；记忆评估和任务终结属于同一个可恢复阶段。
    pub(super) async fn summarize(
        &self,
        run: &mut Run,
        state: RunState,
        reply: Option<Message>,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        if !self.journal.prepare_summary(run, state, reply)? {
            return Ok(false);
        }
        self.resume_summary(run, cancel).await
    }

    pub(super) async fn resume_summary(
        &self,
        run: &mut Run,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if self.journal.has_inputs(&run.id)? {
            self.journal.discard_summary(&run.id)?;
            return Ok(false);
        }
        let summary = self
            .journal
            .summary_checkpoint(&run.id)?
            .ok_or_else(|| Error::Conflict("缺少总结检查点".into()))?;
        let mut source = summary.run;
        source.state = summary.state;
        if self.memory_enabled() && matches!(run.source, RunSource::Agent) {
            let report = self.memory.flush_revisions(run, true)?;
            if report["updated"] != 0 || report["conflicts"] != 0 {
                self.journal.emit(run, "memory.revised", report)?;
            }
            if self.enqueue_experience_learning(&source)? {
                let status = self.memory.learning_status(&run.id)?;
                let ready =
                    status.as_deref() == Some("pending") && self.memory.claim_learning(&run.id)?;
                let restored = status.as_deref() == Some("inflight")
                    && self.memory.learning_result(&run.id)?.is_some();
                let unsent = status.as_deref() == Some("inflight")
                    && !self
                        .journal
                        .summary_request_started(&run.id, summary.request_boundary)?;
                if ready || restored || unsent {
                    // 不受空闲调度限制，也不会因为别的会话到达而丢掉本次总结。
                    let result = self.learn_experience(&run.id, cancel, cancel).await;
                    if cancel.is_cancelled() {
                        return Err(Error::Cancelled);
                    }
                    match result {
                        Ok((count, input, output)) => {
                            self.memory.finish_learning(&run.id, true, input, output)?;
                            self.journal.emit(run, "memory.learned", json!({"count":count,"inputTokens":input,"outputTokens":output,"stage":"summary"}))?;
                        }
                        Err(Error::Cancelled) => return Err(Error::Cancelled),
                        Err(error) => {
                            self.memory.finish_learning(&run.id, false, 0, 0)?;
                            self.journal.emit(run, "memory.review.failed", json!({"error":error.user_message(),"stage":"summary","silent":true}))?;
                        }
                    }
                } else if status.as_deref() == Some("inflight") {
                    // 发送后、结果落盘前中断，不能确定是否计费；不自动重发。
                    self.memory.finish_learning(&run.id, false, 0, 0)?;
                    self.journal.emit(run, "memory.review.failed", json!({"reason":"interruptedUnknownResponse","stage":"summary","silent":true}))?;
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        self.journal.finish_summary(run)
    }
}
