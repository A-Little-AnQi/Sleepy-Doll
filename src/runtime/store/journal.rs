use crate::error::{Error, Result};
use crate::model::ToolCall;
use crate::runtime::types::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct Journal {
    connection: Mutex<Connection>,
    notify: Arc<tokio::sync::Notify>,
}
// Every transaction below writes. Acquire the writer reservation before reading:
// a deferred WAL read transaction cannot upgrade after another connection commits,
// and SQLITE_BUSY_SNAPSHOT bypasses busy_timeout (notably when two chats run).
impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        // WAL must be set here as well: the runtime keeps its own connection which
        // otherwise degrades to the rollback journal under concurrent access.
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON;",
        )?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='runtime_runs')",
            [],
            |r| r.get(0),
        )?;
        if !exists {
            let backup = path.with_extension(format!("pre-runtime-{}.db", unix_now()));
            connection.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()])?;
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE runtime_runs(id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, client_key TEXT UNIQUE NOT NULL, state TEXT NOT NULL, revision INTEGER NOT NULL, payload TEXT NOT NULL);
                CREATE TABLE runtime_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, run_id TEXT NOT NULL, kind TEXT NOT NULL, data TEXT NOT NULL);
                CREATE INDEX runtime_events_conversation ON runtime_events(conversation_id,sequence);
                CREATE TABLE runtime_attempts(id TEXT PRIMARY KEY,run_id TEXT NOT NULL,call_id TEXT NOT NULL,payload TEXT NOT NULL, UNIQUE(run_id,call_id));
                CREATE TABLE runtime_approvals(id TEXT PRIMARY KEY,run_id TEXT NOT NULL,payload TEXT NOT NULL);
                CREATE TABLE runtime_plans(run_id TEXT NOT NULL,revision INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(run_id,revision));
                CREATE TABLE runtime_inputs(id INTEGER PRIMARY KEY AUTOINCREMENT,run_id TEXT NOT NULL,kind TEXT NOT NULL,content TEXT NOT NULL,consumed INTEGER NOT NULL DEFAULT 0);
                CREATE TABLE runtime_artifacts(id TEXT PRIMARY KEY,run_id TEXT NOT NULL,content TEXT NOT NULL);
                CREATE TABLE runtime_leases(instance_id TEXT PRIMARY KEY,attempt_id TEXT NOT NULL UNIQUE);
                COMMIT;")?;
        }
        connection.execute_batch("CREATE TABLE IF NOT EXISTS runtime_message_owners(message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,run_id TEXT NOT NULL REFERENCES runtime_runs(id));")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS saved_strategies(id TEXT PRIMARY KEY,name TEXT NOT NULL,source_run_id TEXT NOT NULL,payload TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL,last_run_id TEXT);
             CREATE UNIQUE INDEX IF NOT EXISTS saved_strategy_source ON saved_strategies(source_run_id);",
        )?;
        crate::runtime::store::migrations::migrate(&mut connection)?;
        connection.execute_batch("CREATE TABLE IF NOT EXISTS runtime_input_keys(run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE,client_key TEXT NOT NULL,kind TEXT NOT NULL,content TEXT NOT NULL,PRIMARY KEY(run_id,client_key));")?;
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS runtime_events_run ON runtime_events(run_id,sequence);",
        )?;
        let journal = Self {
            connection: Mutex::new(connection),
            notify: Arc::new(tokio::sync::Notify::new()),
        };
        journal.prune()?;
        Ok(journal)
    }

    /// Scheduler and IPC waiters subscribe to this instead of polling SQLite.
    pub fn notifier(&self) -> Arc<tokio::sync::Notify> {
        self.notify.clone()
    }

    fn touch(&self) {
        self.notify.notify_waiters();
        self.notify.notify_one();
    }

    /// Stream frames are the only high-volume events, and their text is already
    /// stored with the assistant message, so they are retired first. The wider
    /// cap only guards against unbounded growth; recent evidence is always kept.
    pub fn prune(&self) -> Result<()> {
        let db = self.connection.lock().unwrap();
        db.execute(
            "DELETE FROM runtime_events WHERE kind='assistant.delta' AND sequence <= (SELECT COALESCE(MAX(sequence),0)-2000 FROM runtime_events)",
            [],
        )?;
        db.execute(
            "DELETE FROM runtime_events WHERE sequence <= (SELECT COALESCE(MAX(sequence),0)-200000 FROM runtime_events)",
            [],
        )?;
        Ok(())
    }

    pub fn create(
        &self,
        prompt: &str,
        conversation: &str,
        key: &str,
        duration: i64,
    ) -> Result<Run> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT payload FROM runtime_runs WHERE client_key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(payload) = previous {
            let run: Run = serde_json::from_str(&payload)?;
            if run.prompt != prompt || run.conversation_id != conversation {
                return Err(Error::Conflict(
                    "submission key reused with different input".into(),
                ));
            }
            return Ok(run);
        }
        let pending: u64 = tx.query_row(
            "SELECT count(*) FROM runtime_runs WHERE state='\"queued\"'",
            [],
            |r| r.get(0),
        )?;
        if pending >= 64 {
            return Err(Error::Conflict("run queue is full".into()));
        }
        let stamp = now();
        let mut run = Run {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.into(),
            prompt: prompt.into(),
            state: RunState::Queued,
            revision: 0,
            created_at: stamp.clone(),
            updated_at: stamp.clone(),
            deadline: unix_now() + duration,
            decisions: 0,
            tool_calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            usage_estimated: false,
            message_boundary: 0,
            result: None,
            error: None,
            source: RunSource::Agent,
        };
        tx.execute("INSERT OR IGNORE INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",params![conversation,prompt.chars().take(120).collect::<String>(),stamp])?;
        tx.execute(
            "INSERT INTO runtime_runs VALUES(?1,?2,?3,?4,0,?5)",
            params![
                run.id,
                conversation,
                key,
                serde_json::to_string(&run.state)?,
                serde_json::to_string(&run)?
            ],
        )?;
        tx.execute(
            "INSERT INTO messages(conversation_id,role,content,created_at) VALUES(?1,'user',?2,?3)",
            params![conversation, prompt, stamp],
        )?;
        run.message_boundary = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO runtime_message_owners VALUES(?1,?2)",
            params![run.message_boundary, run.id],
        )?;
        tx.execute(
            "UPDATE runtime_runs SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&run)?, run.id],
        )?;
        tx.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![stamp, conversation],
        )?;
        Self::insert_event(&tx, &run, "run.created", &public_run(&run))?;
        Self::insert_checkpoint(&tx, &run)?;
        tx.commit()?;
        self.touch();
        Ok(run)
    }
    fn insert_event(db: &Connection, run: &Run, kind: &str, data: &Value) -> Result<()> {
        db.execute(
            "INSERT INTO runtime_events(conversation_id,run_id,kind,data) VALUES(?1,?2,?3,?4)",
            params![run.conversation_id, run.id, kind, data.to_string()],
        )?;
        Ok(())
    }
    pub fn get(&self, id: &str) -> Result<Run> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_runs WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }
    pub fn reopen(&self, id: &str) -> Result<Run> {
        let mut run = self.get(id)?;
        if run.state != RunState::NeedsReview {
            return Err(Error::Conflict("只有待核对运行可以恢复".into()));
        }
        let revision = run.revision;
        run.revision += 1;
        run.state = RunState::Recovering;
        run.error = None;
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute(
            "UPDATE runtime_runs SET state=?1,revision=?2,payload=?3 WHERE id=?4 AND revision=?5",
            params![
                serde_json::to_string(&run.state)?,
                run.revision,
                serde_json::to_string(&run)?,
                id,
                revision
            ],
        )? != 1
        {
            return Err(Error::Conflict("运行状态已更新".into()));
        }
        Self::insert_event(&tx, &run, "run.changed", &public_run(&run))?;
        Self::insert_checkpoint(&tx, &run)?;
        tx.commit()?;
        self.touch();
        Ok(run)
    }
    pub fn list(&self) -> Result<Vec<Run>> {
        self.query_runs("SELECT payload FROM runtime_runs ORDER BY rowid DESC LIMIT 100")
    }
    pub fn pending(&self) -> Result<Vec<Run>> {
        self.query_runs("SELECT payload FROM runtime_runs WHERE state NOT IN ('\"answered\"','\"succeeded\"','\"partial\"','\"failed\"','\"cancelled\"','\"needsReview\"') ORDER BY rowid")
    }
    fn query_runs(&self, sql: &str) -> Result<Vec<Run>> {
        let db = self.connection.lock().unwrap();
        let mut q = db.prepare(sql)?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .collect()
    }
    pub fn save(&self, run: &mut Run, next: RunState) -> Result<()> {
        if run.state != next && !run.state.permits(next) {
            return Err(Error::Conflict(format!(
                "invalid transition {:?} -> {:?}",
                run.state, next
            )));
        }
        let mut candidate = run.clone();
        candidate.state = next;
        candidate.revision += 1;
        candidate.updated_at = now();
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if tx.execute(
            "UPDATE runtime_runs SET state=?1,revision=?2,payload=?3 WHERE id=?4 AND revision=?5",
            params![
                serde_json::to_string(&next)?,
                candidate.revision,
                serde_json::to_string(&candidate)?,
                run.id,
                run.revision
            ],
        )? != 1
        {
            return Err(Error::Conflict("stale run revision".into()));
        }
        Self::insert_event(&tx, &candidate, "run.changed", &public_run(&candidate))?;
        Self::insert_checkpoint(&tx, &candidate)?;
        tx.commit()?;
        *run = candidate;
        self.touch();
        Ok(())
    }
    pub fn emit(&self, run: &Run, kind: &str, data: Value) -> Result<()> {
        Self::insert_event(&self.connection.lock().unwrap(), run, kind, &data)?;
        self.touch();
        Ok(())
    }
    pub fn events(&self, conversation: &str, after: u64) -> Result<Vec<Event>> {
        let db = self.connection.lock().unwrap();
        let mut q=db.prepare("SELECT sequence,run_id,kind,data FROM runtime_events WHERE conversation_id=?1 AND sequence>?2 ORDER BY sequence LIMIT 256")?;
        let rows = q
            .query_map(params![conversation, after], |r| {
                Ok((
                    r.get::<_, u64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(sequence, run_id, kind, data)| {
                Ok(Event {
                    sequence,
                    conversation_id: conversation.into(),
                    run_id,
                    kind,
                    data: serde_json::from_str(&data)?,
                })
            })
            .collect()
    }
    pub fn step_outcomes(&self, run_id: &str) -> Result<std::collections::HashMap<String, String>> {
        let db = self.connection.lock().unwrap();
        let mut statement = db.prepare(
            "SELECT data FROM runtime_events WHERE run_id=?1 AND kind='step.finished' ORDER BY sequence",
        )?;
        let rows = statement
            .query_map([run_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut outcomes = std::collections::HashMap::new();
        for payload in rows {
            let value: Value = serde_json::from_str(&payload)?;
            if let (Some(id), Some(outcome)) = (value["id"].as_str(), value["outcome"].as_str()) {
                outcomes.insert(id.into(), outcome.into());
            }
        }
        Ok(outcomes)
    }
    pub fn prepare(
        &self,
        run: &Run,
        call_id: &str,
        request: Value,
        instance: &str,
    ) -> Result<Attempt> {
        let attempt = Attempt {
            id: uuid::Uuid::new_v4().to_string(),
            run_id: run.id.clone(),
            call_id: call_id.into(),
            request_hash: hash(&request),
            request,
            instance_id: instance.into(),
            job_id: None,
            outcome: "prepared".into(),
            evidence: Value::Null,
        };
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_attempts VALUES(?1,?2,?3,?4)",
            params![
                attempt.id,
                run.id,
                call_id,
                serde_json::to_string(&attempt)?
            ],
        )?;
        Ok(attempt)
    }
    pub fn attempt(&self, a: &Attempt) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_attempts SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(a)?, a.id],
        )?;
        Ok(())
    }
    pub fn attempts(&self, run: &str) -> Result<Vec<Attempt>> {
        let db = self.connection.lock().unwrap();
        let mut q =
            db.prepare("SELECT payload FROM runtime_attempts WHERE run_id=?1 ORDER BY rowid")?;
        let rows = q
            .query_map([run], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .collect()
    }
    pub fn acquire(&self, a: &Attempt) -> Result<bool> {
        Ok(self.connection.lock().unwrap().execute(
            "INSERT OR IGNORE INTO runtime_leases VALUES(?1,?2)",
            params![a.instance_id, a.id],
        )? == 1)
    }
    pub fn release(&self, a: &Attempt) -> Result<()> {
        self.connection
            .lock()
            .unwrap()
            .execute("DELETE FROM runtime_leases WHERE attempt_id=?1", [&a.id])?;
        Ok(())
    }
    pub fn approval(&self, a: &Approval) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_approvals VALUES(?1,?2,?3)",
            params![a.id, a.run_id, serde_json::to_string(a)?],
        )?;
        Ok(())
    }
    pub fn decide(&self, id: &str, approved: bool) -> Result<Approval> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let s: String = tx.query_row(
            "SELECT payload FROM runtime_approvals WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let mut a: Approval = serde_json::from_str(&s)?;
        let run_state: String = tx.query_row(
            "SELECT state FROM runtime_runs WHERE id=?1",
            [&a.run_id],
            |r| r.get(0),
        )?;
        if serde_json::from_str::<RunState>(&run_state)? != RunState::AwaitingApproval {
            return Err(Error::Conflict("此运行已经不再等待授权".into()));
        }
        if a.decision.is_some() || a.expires_at < unix_now() {
            return Err(Error::Conflict(
                "approval expired or already answered".into(),
            ));
        }
        a.decision = Some(approved);
        tx.execute(
            "UPDATE runtime_approvals SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&a)?, id],
        )?;
        tx.commit()?;
        self.touch();
        Ok(a)
    }
    pub fn approval_result(&self, id: &str) -> Result<Approval> {
        let s: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_approvals WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&s)?)
    }
    pub fn input(&self, run: &str, kind: &str, content: &str) -> Result<()> {
        self.input_once(run, kind, content, None)
    }

    pub fn input_once(
        &self,
        run: &str,
        kind: &str,
        content: &str,
        key: Option<&str>,
    ) -> Result<()> {
        if content.trim().is_empty() || content.len() > 128 * 1024 {
            return Err(Error::Config("补充内容为空或过长".into()));
        }
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(key) = key {
            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT kind,content FROM runtime_input_keys WHERE run_id=?1 AND client_key=?2",
                    params![run, key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((saved_kind, saved_content)) = existing {
                return if saved_kind == kind && saved_content == content {
                    Ok(())
                } else {
                    Err(Error::Conflict("同一请求标识不能提交不同的补充内容".into()))
                };
            }
        }
        let payload: String =
            tx.query_row("SELECT payload FROM runtime_runs WHERE id=?1", [run], |r| {
                r.get(0)
            })?;
        let current: Run = serde_json::from_str(&payload)?;
        if current.state.terminal() {
            return Err(Error::Conflict("run already finished".into()));
        }
        tx.execute(
            "INSERT INTO runtime_inputs(run_id,kind,content) VALUES(?1,?2,?3)",
            params![run, kind, content],
        )?;
        tx.execute(
            "INSERT INTO messages(conversation_id,role,content,created_at) VALUES(?1,'user',?2,?3)",
            params![current.conversation_id, content, now()],
        )?;
        tx.execute(
            "INSERT INTO runtime_message_owners VALUES(?1,?2)",
            params![tx.last_insert_rowid(), run],
        )?;
        Self::insert_event(&tx, &current, "input.received", &json!({"content":content}))?;
        if let Some(key) = key {
            tx.execute("INSERT INTO runtime_input_keys(run_id,client_key,kind,content) VALUES(?1,?2,?3,?4)", params![run,key,kind,content])?;
        }
        tx.commit()?;
        self.touch();
        Ok(())
    }
    pub fn finish(&self, run: &mut Run, next: RunState) -> Result<bool> {
        if run.state.terminal() || !next.terminal() {
            return Err(Error::Conflict("invalid finish transition".into()));
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
        let mut candidate = run.clone();
        candidate.state = next;
        candidate.revision += 1;
        candidate.updated_at = now();
        if tx.execute(
            "UPDATE runtime_runs SET state=?1,revision=?2,payload=?3 WHERE id=?4 AND revision=?5",
            params![
                serde_json::to_string(&next)?,
                candidate.revision,
                serde_json::to_string(&candidate)?,
                run.id,
                run.revision
            ],
        )? != 1
        {
            return Err(Error::Conflict("stale finish revision".into()));
        }
        Self::insert_event(&tx, &candidate, "run.changed", &public_run(&candidate))?;
        Self::insert_checkpoint(&tx, &candidate)?;
        tx.commit()?;
        *run = candidate;
        self.touch();
        Ok(true)
    }
    pub fn conversations(&self) -> Result<Vec<ConversationSummary>> {
        let connection = self.connection.lock().unwrap();
        let mut query = connection.prepare("SELECT id,title,created_at,updated_at FROM conversations ORDER BY updated_at DESC LIMIT 50")?;
        Ok(query
            .query_map([], |row| {
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn record_tool(
        &self,
        conversation_id: &str,
        call: &ToolCall,
        result: &Value,
    ) -> Result<()> {
        let connection = self.connection.lock().unwrap();
        connection.execute(
            "INSERT INTO tool_calls (conversation_id,call_id,tool_name,arguments_json,result_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                conversation_id,
                call.id,
                call.name,
                call.arguments.to_string(),
                result.to_string(),
                now()
            ],
        )?;
        Ok(())
    }

    pub fn history(&self, run: &Run) -> Result<Vec<crate::model::Message>> {
        self.messages(&run.conversation_id, run.message_boundary, true)
    }
    pub fn conversation_messages(&self, conversation: &str) -> Result<Vec<crate::model::Message>> {
        self.messages(conversation, i64::MAX, false)
    }
    pub fn fork_conversation(&self, conversation: &str, title: Option<&str>) -> Result<String> {
        let id = format!("conversation-{}", uuid::Uuid::new_v4());
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let source_title: String = tx.query_row(
            "SELECT title FROM conversations WHERE id=?1",
            [conversation],
            |row| row.get(0),
        )?;
        let stamp = now();
        let target_title = title
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} · 副本", source_title));
        tx.execute(
            "INSERT INTO conversations(id,title,created_at,updated_at) VALUES(?1,?2,?3,?3)",
            params![id, target_title, stamp],
        )?;
        tx.execute(
            "INSERT INTO messages(conversation_id,role,content,tool_call_id,tool_calls_json,reasoning_json,created_at)
             SELECT ?1,m.role,m.content,m.tool_call_id,m.tool_calls_json,m.reasoning_json,m.created_at
             FROM messages m
             LEFT JOIN runtime_message_owners o ON o.message_id=m.id
             LEFT JOIN runtime_runs r ON r.id=o.run_id
             WHERE m.conversation_id=?2 AND (r.id IS NULL OR r.state IN ('\"answered\"','\"succeeded\"','\"partial\"','\"failed\"','\"cancelled\"','\"needsReview\"'))
             ORDER BY m.id",
            params![id, conversation],
        )?;
        tx.commit()?;
        Ok(id)
    }
    pub fn append_message(&self, run: &Run, message: &crate::model::Message) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO messages(conversation_id,role,content,tool_call_id,tool_calls_json,reasoning_json,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![run.conversation_id,serde_json::to_value(message.role)?.as_str().unwrap(),message.content,message.tool_call_id,serde_json::to_string(&message.tool_calls)?,serde_json::to_string(&message.reasoning)?,now()])?;
        tx.execute(
            "INSERT INTO runtime_message_owners VALUES(?1,?2)",
            params![tx.last_insert_rowid(), run.id],
        )?;
        tx.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now(), run.conversation_id],
        )?;
        tx.commit()?;
        Ok(())
    }
    fn messages(
        &self,
        conversation: &str,
        boundary: i64,
        include_queued: bool,
    ) -> Result<Vec<crate::model::Message>> {
        let db = self.connection.lock().unwrap();
        let mut q=db.prepare("SELECT m.role,m.content,m.tool_call_id,m.tool_calls_json,m.reasoning_json FROM messages m LEFT JOIN runtime_message_owners o ON o.message_id=m.id LEFT JOIN runtime_runs r ON r.id=o.run_id WHERE m.conversation_id=?1 AND COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id)<=?2 AND (?3 OR r.state IS NULL OR r.state!='\"queued\"') ORDER BY COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id),m.id")?;
        let rows = q
            .query_map(params![conversation, boundary, include_queued], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(role, content, tool_call_id, calls, reasoning)| {
                Ok(crate::model::Message {
                    role: serde_json::from_value(json!(role))?,
                    content,
                    tool_call_id,
                    tool_calls: serde_json::from_str(calls.as_deref().unwrap_or("[]"))?,
                    // 列可空：`null` 与 NULL 都还原成 None。
                    reasoning: serde_json::from_str(reasoning.as_deref().unwrap_or("null"))?,
                })
            })
            .collect()
    }
    pub fn drain_inputs(&self, run: &str) -> Result<Vec<String>> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let rows = {
            let mut q = tx.prepare(
                "SELECT content FROM runtime_inputs WHERE run_id=?1 AND consumed=0 ORDER BY id",
            )?;
            q.query_map([run], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute(
            "UPDATE runtime_inputs SET consumed=1 WHERE run_id=?1",
            [run],
        )?;
        tx.commit()?;
        Ok(rows)
    }
    pub fn has_inputs(&self, run: &str) -> Result<bool> {
        Ok(self.connection.lock().unwrap().query_row(
            "SELECT EXISTS(SELECT 1 FROM runtime_inputs WHERE run_id=?1 AND consumed=0)",
            [run],
            |r| r.get(0),
        )?)
    }
    pub fn completed_read_steps(
        &self,
        run: &str,
        plan: &PlanRevision,
    ) -> Result<std::collections::HashSet<String>> {
        let db = self.connection.lock().unwrap();
        Self::completed_read_steps_in(&db, run, plan)
    }
    fn completed_read_steps_in(
        db: &Connection,
        run: &str,
        plan: &PlanRevision,
    ) -> Result<std::collections::HashSet<String>> {
        let mut query =
            db.prepare("SELECT data FROM runtime_events WHERE run_id=?1 AND kind='step.finished'")?;
        let events = query
            .query_map([run], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut completed = std::collections::HashSet::new();
        for event in events {
            let event: Value = serde_json::from_str(&event)?;
            if event["planRevision"] != plan.revision || event["outcome"] != "completed" {
                continue;
            }
            if let Some(step) = plan.steps.iter().find(|step| event["id"] == step.id)
                && step.execution.as_ref().is_some_and(|execution| {
                    execution.effect == crate::extension::ToolEffect::ReadOnly
                })
            {
                completed.insert(step.id.clone());
            }
        }
        Ok(completed)
    }
    pub fn artifact(&self, run: &Run, content: &str) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.lock().unwrap().execute(
            "INSERT INTO runtime_artifacts VALUES(?1,?2,?3)",
            params![id, run.id, content],
        )?;
        Ok(id)
    }
    pub fn read_artifact(&self, run: &str, id: &str) -> Result<String> {
        Ok(self.connection.lock().unwrap().query_row(
            "SELECT content FROM runtime_artifacts WHERE id=?1 AND run_id=?2",
            params![id, run],
            |r| r.get(0),
        )?)
    }
    pub fn plan(&self, run: &str) -> Result<Option<PlanRevision>> {
        let s: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM runtime_plans WHERE run_id=?1 ORDER BY revision DESC LIMIT 1",
                [run],
                |r| r.get(0),
            )
            .optional()?;
        s.map(|s| serde_json::from_str(&s).map_err(Error::from))
            .transpose()
    }
    pub fn save_plan(&self, run: &Run, plan: &PlanRevision) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO runtime_plans VALUES(?1,?2,?3)",
            params![run.id, plan.revision, serde_json::to_string(plan)?],
        )?;
        Self::insert_event(&tx, run, "plan.changed", &json!(plan))?;
        Self::insert_checkpoint(&tx, run)?;
        tx.commit()?;
        self.touch();
        Ok(())
    }

    pub fn checkpoint(&self, run_id: &str) -> Result<RunCheckpoint> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM runtime_checkpoints WHERE run_id=?1 ORDER BY revision DESC LIMIT 1",
            [run_id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    fn insert_checkpoint(db: &Connection, run: &Run) -> Result<()> {
        let plan: Option<PlanRevision> = db
            .query_row(
                "SELECT payload FROM runtime_plans WHERE run_id=?1 ORDER BY revision DESC LIMIT 1",
                [&run.id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|payload| serde_json::from_str(&payload))
            .transpose()?;
        let mut query =
            db.prepare("SELECT payload FROM runtime_attempts WHERE run_id=?1 ORDER BY rowid")?;
        let attempts = query
            .query_map([&run.id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .map(|payload| serde_json::from_str::<Attempt>(&payload))
            .collect::<serde_json::Result<Vec<_>>>()?;
        let pending_approval = db
            .query_row(
                "SELECT payload FROM runtime_approvals WHERE run_id=?1 ORDER BY rowid DESC LIMIT 1",
                [&run.id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .and_then(|payload| serde_json::from_str::<Approval>(&payload).ok())
            .filter(|approval| approval.decision.is_none() && approval.expires_at >= unix_now())
            .map(|approval| approval.id);
        let completed_reads = plan
            .as_ref()
            .map(|plan| Self::completed_read_steps_in(db, &run.id, plan))
            .transpose()?
            .unwrap_or_default();
        let completed_steps = plan
            .as_ref()
            .map(|plan| {
                plan.steps
                    .iter()
                    .filter(|step| {
                        completed_reads.contains(&step.id)
                            || attempts.iter().any(|attempt| {
                                attempt.request["stepId"] == step.id
                                    && attempt.outcome == "verifiedSucceeded"
                            })
                    })
                    .map(|step| step.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let pending_step = plan.as_ref().and_then(|plan| {
            plan.steps
                .iter()
                .find(|step| !completed_steps.contains(&step.id))
                .map(|step| step.id.clone())
        });
        let checkpoint = RunCheckpoint {
            run_id: run.id.clone(),
            goal: run.prompt.clone(),
            run_state: run.state,
            run_revision: run.revision,
            plan_revision: plan.as_ref().map(|plan| plan.revision),
            completed_steps,
            pending_step,
            evidence_refs: attempts
                .iter()
                .filter(|attempt| !attempt.evidence.is_null())
                .map(|attempt| attempt.id.clone())
                .collect(),
            unresolved_attempts: attempts
                .iter()
                .filter(|attempt| {
                    matches!(
                        attempt.outcome.as_str(),
                        "prepared" | "submitting" | "running" | "unknown" | "completed"
                    )
                })
                .map(|attempt| attempt.id.clone())
                .collect(),
            pending_approval,
            selected_resources: attempts
                .iter()
                .filter_map(|attempt| attempt.request.get("resources").cloned())
                .collect(),
            bridge_instances: attempts
                .iter()
                .map(|attempt| attempt.instance_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect(),
            catalog_versions: attempts
                .iter()
                .filter_map(|attempt| {
                    attempt.request["catalogVersion"]
                        .as_str()
                        .map(str::to_owned)
                })
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect(),
            created_at: now(),
        };
        db.execute(
            "INSERT OR REPLACE INTO runtime_checkpoints(run_id,revision,payload,created_at) VALUES(?1,?2,?3,?4)",
            params![run.id, run.revision, serde_json::to_string(&checkpoint)?, checkpoint.created_at],
        )?;
        Ok(())
    }

    pub fn create_strategy(&self, run_id: &str, name: &str) -> Result<SavedStrategy> {
        let run = self.get(run_id)?;
        if run.state != RunState::Succeeded {
            return Err(Error::Conflict("只有已验证成功的运行可以保存为策略".into()));
        }
        let plan = self
            .plan(run_id)?
            .ok_or_else(|| Error::Conflict("该运行没有可保存的执行计划".into()))?;
        if name.trim().is_empty() || name.chars().count() > 80 {
            return Err(Error::Config("策略名称应为 1 到 80 个字符".into()));
        }
        if self
            .strategies()?
            .iter()
            .any(|strategy| strategy.source_run_id == run_id)
        {
            return Err(Error::Conflict("该运行已经保存到运行库".into()));
        }
        let stamp = now();
        let strategy = SavedStrategy {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.trim().into(),
            source_run_id: run_id.into(),
            plan,
            created_at: stamp.clone(),
            updated_at: stamp.clone(),
            last_run_id: None,
        };
        self.connection.lock().unwrap().execute(
            "INSERT INTO saved_strategies(id,name,source_run_id,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",
            params![strategy.id, strategy.name, strategy.source_run_id, serde_json::to_string(&strategy)?, stamp],
        )?;
        Ok(strategy)
    }

    pub fn strategies(&self) -> Result<Vec<SavedStrategy>> {
        let db = self.connection.lock().unwrap();
        let mut q = db.prepare("SELECT payload FROM saved_strategies ORDER BY updated_at DESC")?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|s| serde_json::from_str(&s).map_err(Error::from))
            .collect()
    }

    pub fn strategy(&self, id: &str) -> Result<SavedStrategy> {
        let payload: String = self.connection.lock().unwrap().query_row(
            "SELECT payload FROM saved_strategies WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn mark_strategy_run(&self, strategy: &mut SavedStrategy, run_id: &str) -> Result<()> {
        strategy.last_run_id = Some(run_id.into());
        strategy.updated_at = now();
        self.connection.lock().unwrap().execute(
            "UPDATE saved_strategies SET payload=?1,updated_at=?2,last_run_id=?3 WHERE id=?4",
            params![
                serde_json::to_string(strategy)?,
                strategy.updated_at,
                run_id,
                strategy.id
            ],
        )?;
        Ok(())
    }

    pub fn create_manual_run(
        &self,
        strategy: &SavedStrategy,
        key: &str,
        duration: i64,
    ) -> Result<Run> {
        let conversation = format!("strategy-{}", strategy.id);
        let mut run = self.create(
            &format!("运行策略：{}", strategy.name),
            &conversation,
            key,
            duration,
        )?;
        run.source = RunSource::SavedStrategy {
            strategy_id: strategy.id.clone(),
        };
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_runs SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&run)?, run.id],
        )?;
        self.save_plan(&run, &strategy.plan)?;
        Ok(run)
    }

    pub fn create_workflow_run(
        &self,
        workflow: &crate::runtime::operation::workflow::Workflow,
        key: &str,
        duration: i64,
    ) -> Result<Run> {
        let conversation = format!("workflow-{}", workflow.id);
        let mut run = self.create(
            &format!("运行流程：{}", workflow.name),
            &conversation,
            key,
            duration,
        )?;
        run.source = RunSource::SavedWorkflow {
            workflow_id: workflow.id.clone(),
            workflow_revision: workflow.revision,
        };
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_runs SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&run)?, run.id],
        )?;
        Ok(run)
    }
}
