//! 有界并行按完成顺序落盘，空闲执行位立即补充下一项。
use super::*;
use futures_util::{StreamExt, stream};

impl Supervisor {
    pub(super) async fn parallel_tools(
        &self,
        run: &mut Run,
        calls: &[ToolCall],
        bridge: &host::bridge::Bridge,
        cancel: &CancellationToken,
        exposed: &mut HashSet<String>,
        history: &mut Vec<Message>,
    ) -> Result<()> {
        self.journal.save(run, RunState::Executing)?;
        let policy = self.config.read().unwrap().runtime.clone();
        let work = calls
            .iter()
            .map(|call| {
                let mut local_run = run.clone();
                let mut local_exposed = exposed.clone();
                let policy = &policy;
                let limit = self
                    .definition(&call.name, exposed)
                    .map(|d| d.execution.max_result_chars)
                    .unwrap_or(12_000);
                async move {
                    if cancel.is_cancelled() {
                        return (call, Err(Error::Cancelled), local_exposed, limit, false);
                    }
                    let result = match self.journal.emit(&local_run, "tool.started", json!(call)) {
                        Ok(()) => {
                            self.tool(
                                &mut local_run,
                                call,
                                bridge,
                                policy,
                                cancel,
                                &mut local_exposed,
                            )
                            .await
                        }
                        Err(error) => Err(error),
                    };
                    (call, result, local_exposed, limit, true)
                }
            })
            .collect::<Vec<_>>();
        let mut pending = stream::iter(work).buffer_unordered(4);
        while let Some((call, result, discovered, limit, started)) = pending.next().await {
            run.tool_calls += usize::from(started);
            exposed.extend(discovered);
            self.record_result(run, call, result, limit, history)?;
            self.journal.save(run, RunState::Executing)?;
        }
        Ok(())
    }
}
