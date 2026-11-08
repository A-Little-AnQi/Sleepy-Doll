use super::{
    Scheduler,
    model::{BoundEntry, EntryResult, Occurrence},
};
use crate::{
    Result,
    runtime::{
        Supervisor,
        types::{Run, RunState},
    },
};

pub(super) fn entry_result(
    supervisor: &Supervisor,
    entry: &BoundEntry,
    run: &Run,
    timed_out: bool,
    stopping: bool,
) -> Result<EntryResult> {
    let unresolved = supervisor.journal.attempts(&run.id)?.iter().any(|attempt| {
        matches!(
            attempt.outcome.as_str(),
            "unknown" | "running" | "submitting"
        )
    });
    let terminal = run.state.terminal() || run.state == RunState::Blocked;
    let state = if timed_out {
        "timeout"
    } else if !terminal {
        "cancelling"
    } else if unresolved || run.state == RunState::NeedsReview {
        "unknown"
    } else {
        match run.state {
            RunState::Succeeded => "succeeded",
            RunState::Partial => "partial",
            RunState::Cancelled => "cancelled",
            _ => "failed",
        }
    };
    let stopping = stopping || run.state == RunState::Cancelled;
    Ok(EntryResult {
        entry_id: entry.entry_id.clone(),
        title: entry.title.clone(),
        state: state.into(),
        run_id: Some(run.id.clone()),
        error: if state == "failed" || state == "partial" || (state == "unknown" && !stopping) {
            run.error.clone()
        } else {
            None
        },
        stop_state: stopping.then(|| {
            if !terminal {
                "requested"
            } else if unresolved || run.state == RunState::NeedsReview {
                "unknown"
            } else {
                "confirmed"
            }
            .into()
        }),
    })
}

pub(super) fn occurrence_state(value: &Occurrence, stopping: bool) -> &'static str {
    if value
        .results
        .iter()
        .any(|entry| entry.state == "cancelling")
    {
        return "cancelling";
    }
    if value.results.iter().any(|entry| entry.state == "unknown") {
        return "unknown";
    }
    let complete = value.results.len() == value.schedule.entries.len()
        && value.results.iter().all(|entry| {
            matches!(
                entry.state.as_str(),
                "succeeded" | "partial" | "failed" | "timeout"
            )
        });
    if stopping && !complete {
        return "cancelled";
    }
    if !value.results.is_empty() && value.results.iter().all(|entry| entry.state == "cancelled") {
        return "cancelled";
    }
    if !value.results.is_empty() && value.results.iter().all(|entry| entry.state == "succeeded") {
        "succeeded"
    } else if value
        .results
        .iter()
        .any(|entry| matches!(entry.state.as_str(), "succeeded" | "partial"))
    {
        "partial"
    } else if value.results.iter().any(|entry| entry.state == "timeout") {
        "timeout"
    } else {
        "failed"
    }
}

impl Scheduler {
    fn project_history(&self, value: &mut Occurrence) -> Result<()> {
        let stopping = self.store.cancelled(&value.id)?
            || matches!(value.state.as_str(), "cancelled" | "cancelling");
        let supervisor = self.supervisor()?;
        let mut projected = false;
        for result in &mut value.results {
            let concerning = result.stop_state.is_some()
                || matches!(
                    result.state.as_str(),
                    "cancelled" | "cancelling" | "timeout"
                );
            let Some(id) = &result.run_id else {
                continue;
            };
            let run = match supervisor.journal.get(id) {
                Ok(run) => run,
                Err(_) if concerning => {
                    result.state = "unknown".into();
                    result.stop_state = Some("unknown".into());
                    result.error = None;
                    projected = true;
                    continue;
                }
                Err(_) => continue,
            };
            if !concerning
                && !stopping
                && !matches!(run.state, RunState::Cancelled | RunState::Cancelling)
            {
                continue;
            }
            let Some(entry) = value
                .schedule
                .entries
                .iter()
                .find(|entry| entry.entry_id == result.entry_id)
            else {
                continue;
            };
            *result = entry_result(&supervisor, entry, &run, result.state == "timeout", true)?;
            projected = true;
        }
        if projected {
            value.state = occurrence_state(value, stopping).into();
        }
        Ok(())
    }

    pub fn history(&self, id: Option<&str>, limit: usize) -> Result<Vec<Occurrence>> {
        let mut values = self.store.history(id, limit)?;
        for value in &mut values {
            self.project_history(value)?;
        }
        Ok(values)
    }

    pub(super) fn reconcile_stopping(&self) -> Result<()> {
        for mut value in self.store.stopping()? {
            let original_state = value.state.clone();
            let original_results = value.results.clone();
            self.project_history(&mut value)?;
            if value.state != original_state || value.results != original_results {
                self.store.update_occurrence(&value)?;
            }
        }
        Ok(())
    }
}
