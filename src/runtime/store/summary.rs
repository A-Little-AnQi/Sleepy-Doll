use super::*;

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SummaryCheckpoint {
    pub state: RunState,
    pub run: Run,
    pub reply: Option<crate::model::Message>,
    pub request_boundary: i64,
}

impl Journal {
    pub(crate) fn summary_checkpoint(&self, run: &str) -> Result<Option<SummaryCheckpoint>> {
        let payload: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM runtime_summaries WHERE run_id=?1",
                [run],
                |r| r.get(0),
            )
            .optional()?;
        payload
            .map(|text| serde_json::from_str(&text).map_err(Into::into))
            .transpose()
    }

    pub(crate) fn prepare_summary(
        &self,
        run: &Run,
        state: RunState,
        reply: Option<crate::model::Message>,
    ) -> Result<bool> {
        if run.state.terminal() || !state.terminal() {
            return Err(Error::Conflict("invalid summary transition".into()));
        }
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let pending: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM runtime_inputs WHERE run_id=?1 AND consumed=0)",
            [&run.id],
            |r| r.get(0),
        )?;
        if pending {
            return Ok(false);
        }
        let current: u64 = tx.query_row(
            "SELECT revision FROM runtime_runs WHERE id=?1",
            [&run.id],
            |r| r.get(0),
        )?;
        if current != run.revision {
            return Err(Error::Conflict("stale summary revision".into()));
        }
        let checkpoint = SummaryCheckpoint {
            state,
            run: run.clone(),
            reply,
            request_boundary: tx.query_row("SELECT COALESCE(MAX(rowid),0) FROM runtime_model_requests WHERE run_id=?1 AND purpose='memory'", [&run.id], |r| r.get(0))?,
        };
        tx.execute("INSERT INTO runtime_summaries(run_id,payload) VALUES(?1,?2) ON CONFLICT(run_id) DO UPDATE SET payload=excluded.payload", params![run.id,serde_json::to_string(&checkpoint)?])?;
        Self::insert_event(&tx, run, "summary.prepared", &json!({"state":state}))?;
        tx.commit()?;
        self.touch();
        Ok(true)
    }

    pub(crate) fn finish_summary(&self, run: &mut Run) -> Result<bool> {
        let summary = self
            .summary_checkpoint(&run.id)?
            .ok_or_else(|| Error::Conflict("缺少总结检查点".into()))?;
        let (state, revision) = (run.state, run.revision);
        *run = summary.run;
        run.state = state;
        run.revision = revision;
        let finished = self.finish_with_reply(run, summary.state, summary.reply.as_ref())?;
        if !finished {
            self.discard_summary(&run.id)?;
        }
        Ok(finished)
    }

    pub(crate) fn summary_request_started(&self, run: &str, boundary: i64) -> Result<bool> {
        Ok(self.connection.lock().unwrap().query_row("SELECT EXISTS(SELECT 1 FROM runtime_model_requests WHERE run_id=?1 AND purpose='memory' AND rowid>?2)", params![run,boundary], |r| r.get(0))?)
    }

    pub(crate) fn discard_summary(&self, run: &str) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute("DELETE FROM runtime_summaries WHERE run_id=?1", [run])? > 0 {
            tx.execute("DELETE FROM runtime_memory_results WHERE run_id=?1", [run])?;
            tx.execute("DELETE FROM runtime_memory_learning WHERE run_id=?1", [run])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_summary_terminal_and_reply_rollback_together_and_recover_after_reopen() {
        let root = crate::test_root("memory-summary-transaction");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("history.db");
        let journal = Journal::open(&path).unwrap();
        let mut run = journal
            .create("read", "chat", "summary", 300, None)
            .unwrap();
        run.result = Some("已读取。".into());
        let reply = crate::runtime::context::message(crate::model::Role::Assistant, "已读取。");
        journal
            .prepare_summary(&run, RunState::Answered, Some(reply))
            .unwrap();
        journal.connection.lock().unwrap().execute_batch("CREATE TRIGGER fail_summary BEFORE INSERT ON runtime_events WHEN new.kind='run.changed' BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(journal.finish_summary(&mut run).is_err());
        assert_eq!(journal.get(&run.id).unwrap().state, RunState::Queued);
        assert_eq!(journal.run_history(&run).unwrap().len(), 1);
        assert!(journal.summary_checkpoint(&run.id).unwrap().is_some());
        journal
            .connection
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_summary")
            .unwrap();
        drop(journal);
        let reopened = Journal::open(&path).unwrap();
        assert!(reopened.finish_summary(&mut run).unwrap());
        assert_eq!(reopened.get(&run.id).unwrap().state, RunState::Answered);
        assert_eq!(
            reopened
                .run_history(&run)
                .unwrap()
                .iter()
                .filter(|m| m.content == "已读取。")
                .count(),
            1
        );
        assert!(reopened.summary_checkpoint(&run.id).unwrap().is_none());
    }
}
