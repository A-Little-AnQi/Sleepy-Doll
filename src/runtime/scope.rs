//! Explicit shortcut boundaries and mechanical no-progress limits.
//! Task interpretation and continuation belong to the main model and loaded skills.
use super::*;

impl Supervisor {
    /// A standalone skill invocation is a human request to save an entry, not
    /// permission to execute its bound action. Historical/tool text cannot grant it.
    pub(super) fn shortcut_only(&self, run: &Run) -> Result<bool> {
        if run.shortcut_configuration {
            return Ok(true);
        }
        if !matches!(run.source, RunSource::Agent)
            || self
                .config
                .read()
                .unwrap()
                .agent
                .disabled_skills
                .iter()
                .any(|name| name == "create-shortcut")
        {
            return Ok(false);
        }
        Ok(self.journal.goal(&run.id)?.is_some_and(|goal| {
            matches!(goal.inputs.first(), Some(input)
                if input.origin == contracts::InputOrigin::HumanMessage
                    && input.text.trim() == "$create-shortcut")
        }))
    }

    pub(super) fn check_progress(&self, run: &Run, call: &ToolCall) -> Result<()> {
        if !matches!(run.source, RunSource::Agent) {
            return Ok(());
        }
        let Some(goal) = self.journal.goal(&run.id)? else {
            return Ok(());
        };
        let receipts = self.journal.goal_receipts(&run.id, goal.revision)?;
        let limit = self.config.read().unwrap().runtime.max_stagnant_calls;
        if limit == 0 {
            return Ok(());
        }
        let signature = hash(&call.arguments);
        let matching = receipts.iter().rev().take(limit.max(2)).collect::<Vec<_>>();
        if matching.len() == limit.max(2)
            && matching
                .iter()
                .all(|r| r.tool == call.name && r.arguments_hash == signature)
            && matching.iter().all(|r| {
                r.effect == ToolEffect::ReadOnly || r.state == contracts::ReceiptState::Failed
            })
            && matching.windows(2).all(|pair| {
                contracts::progress_value(&pair[0].evidence)
                    == contracts::progress_value(&pair[1].evidence)
            })
        {
            return Err(Error::Tool("相同操作没有带来进展，未再次执行".into()));
        }
        Ok(())
    }
}
