use crate::error::{Error, Result};
use crate::model::ToolCall;
use crate::runtime::types::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
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
    pub pinned: bool,
    pub archived: bool,
    /// 会话绑定的模型；为空或指向已删除配置时回落到默认模型。
    pub model_id: Option<String>,
    pub task_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationMessage {
    #[serde(flatten)]
    pub message: crate::model::Message,
    pub created_at: String,
    pub run_id: Option<String>,
    pub stream_boundary: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextActivity {
    pub id: u64,
    pub run_id: String,
    pub state: &'static str,
}

/// 同一请求再次打开（崩溃重放）时的结果：open 保持原状不发新事件，
/// answered 保留已存答案，superseded 保持结束。身份是 (run_id, request_id)。
#[derive(Debug)]
pub enum QuestionReopen {
    Opened,
    Open,
    Answered(Value),
    Superseded,
}

#[cfg(test)]
mod context_activity_tests {
    use super::*;

    #[test]
    fn interrupt_submission_rolls_back_when_model_binding_fails() {
        let journal = deletion_journal();
        journal
            .connection
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER reject_binding BEFORE UPDATE OF model_id ON conversations
             BEGIN SELECT RAISE(ABORT, 'binding unavailable'); END;",
            )
            .unwrap();
        assert!(
            journal
                .create("new", "chat", "new-key", 300, Some("model"))
                .is_err()
        );
        let db = journal.connection.lock().unwrap();
        for table in [
            "runtime_runs",
            "messages",
            "conversations",
            "runtime_events",
        ] {
            let count: i64 = db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0, "failed submission left rows in {table}");
        }
    }

    #[test]
    fn interrupt_history_pairs_missing_calls_without_changing_chat_records() {
        let journal = deletion_journal();
        let mut old = journal.create("old", "chat", "old-key", 300, None).unwrap();
        let mut assistant = crate::runtime::context::message(crate::model::Role::Assistant, "");
        assistant.tool_calls = vec![
            ToolCall {
                id: "finished".into(),
                name: "read".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "interrupted".into(),
                name: "write".into(),
                arguments: json!({}),
            },
        ];
        journal.append_message(&old, &assistant).unwrap();
        let mut result = crate::runtime::context::message(crate::model::Role::Tool, "real result");
        result.tool_call_id = Some("finished".into());
        journal.append_message(&old, &result).unwrap();
        journal.save(&mut old, RunState::Cancelled).unwrap();
        let new = journal
            .create("new", "chat", "new-key", 300, Some("model"))
            .unwrap();
        let stored = journal.conversation_messages("chat").unwrap();
        let history = journal.history(&new).unwrap();
        assert_eq!(history.len(), 5); // user, assistant, two tool replies, new user
        let synthetic = history
            .iter()
            .find(|m| m.tool_call_id.as_deref() == Some("interrupted"))
            .unwrap();
        let payload: Value = serde_json::from_str(&synthetic.content).unwrap();
        assert_eq!(payload["ok"], false);
        assert_eq!(payload["executionStatus"], "unknown");
        assert_eq!(history.last().unwrap().content, "new");
        assert_eq!(
            history
                .iter()
                .find(|m| m.tool_call_id.as_deref() == Some("finished"))
                .unwrap(),
            &result
        );
        assert_eq!(
            journal.conversation_messages("chat").unwrap().len(),
            stored.len()
        );
        assert_eq!(
            journal.conversation("chat").unwrap().model_id.as_deref(),
            Some("model")
        );
    }

    fn deletion_journal() -> Journal {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE runtime_runs(id TEXT PRIMARY KEY,conversation_id TEXT,client_key TEXT UNIQUE,state TEXT,revision INTEGER,payload TEXT);
            CREATE TABLE runtime_events(sequence INTEGER PRIMARY KEY AUTOINCREMENT,conversation_id TEXT,run_id TEXT,kind TEXT,data TEXT);
            CREATE TABLE runtime_attempts(id TEXT,run_id TEXT,call_id TEXT,payload TEXT);
            CREATE TABLE runtime_approvals(id TEXT,run_id TEXT,payload TEXT);
            CREATE TABLE runtime_plans(run_id TEXT,revision INTEGER,payload TEXT);
            CREATE TABLE runtime_inputs(id INTEGER PRIMARY KEY,run_id TEXT,kind TEXT,content TEXT,consumed INTEGER);
            CREATE TABLE runtime_artifacts(id TEXT,run_id TEXT,content TEXT);
            CREATE TABLE runtime_leases(instance_id TEXT,attempt_id TEXT);").unwrap();
        crate::runtime::store::migrations::migrate(&mut connection).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE runtime_message_owners(message_id INTEGER,run_id TEXT);
            CREATE TABLE runtime_input_keys(run_id TEXT,client_key TEXT,kind TEXT,content TEXT);",
            )
            .unwrap();
        Journal {
            connection: Mutex::new(connection),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    #[test]
    fn permanent_deletion_removes_related_rows_and_preserves_other_chats_and_tasks() {
        let journal = deletion_journal();
        let mut run = journal
            .create("private prompt", "deleted", "deleted-key", 300, None)
            .unwrap();
        journal.save(&mut run, RunState::Cancelled).unwrap();
        let mut other = journal
            .create("preserved prompt", "kept", "kept-key", 300, None)
            .unwrap();
        journal.save(&mut other, RunState::Cancelled).unwrap();
        {
            let db = journal.connection.lock().unwrap();
            db.execute(
                "INSERT INTO runtime_attempts VALUES('attempt',?1,'call','private')",
                [&run.id],
            )
            .unwrap();
            db.execute(
                "INSERT INTO runtime_approvals VALUES('approval',?1,'private')",
                [&run.id],
            )
            .unwrap();
            db.execute(
                "INSERT INTO runtime_inputs VALUES(1,?1,'input','private',0)",
                [&run.id],
            )
            .unwrap();
            db.execute(
                "INSERT INTO runtime_plans VALUES(?1,1,'private')",
                [&run.id],
            )
            .unwrap();
            db.execute(
                "INSERT INTO runtime_artifacts VALUES('artifact',?1,'private')",
                [&run.id],
            )
            .unwrap();
            db.execute("INSERT INTO task_definitions VALUES('task','task','saved independently','deleted',1,NULL,0,0,'{}','now')",[]).unwrap();
            db.execute("INSERT INTO runtime_operations VALUES('operation',?1,'fixture','done',1,'{}','now','now')",[&run.id]).unwrap();
            db.execute("INSERT INTO runtime_evidence VALUES('evidence','operation','fixture','private','now')",[]).unwrap();
            db.execute("INSERT INTO runtime_operation_events(operation_id,kind,payload,created_at) VALUES('operation','done','private','now')",[]).unwrap();
            db.execute(
                "INSERT INTO runtime_artifact_refs VALUES('artifact','run',?1,'result','now')",
                [&run.id],
            )
            .unwrap();
            db.execute("INSERT INTO runtime_artifact_refs VALUES('shared','resourceSnapshot','snapshot','content','now')",[]).unwrap();
            db.execute("INSERT INTO runtime_notifications VALUES('notification','run','{\"target\":{\"conversationId\":\"deleted\"}}','now',NULL)",[]).unwrap();
        }
        let result = journal.delete_conversation("deleted").unwrap();
        assert_eq!(result["permanent"], true);
        assert_eq!(result["tasksKept"], 1);
        assert_eq!(
            journal.conversation_messages("kept").unwrap()[0]
                .message
                .content,
            "preserved prompt"
        );
        assert_eq!(journal.get(&other.id).unwrap().id, other.id);
        let db = journal.connection.lock().unwrap();
        for table in [
            "runtime_attempts",
            "runtime_approvals",
            "runtime_inputs",
            "runtime_plans",
            "runtime_artifacts",
            "runtime_operations",
            "runtime_evidence",
            "runtime_operation_events",
            "runtime_notifications",
        ] {
            let count: i64 = db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} retained deleted chat data");
        }
        let events: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM runtime_events WHERE conversation_id='deleted'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let checkpoints: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM runtime_checkpoints WHERE run_id=?1",
                [&run.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((events, checkpoints), (0, 0));
        let shared: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM runtime_artifact_refs WHERE owner_kind='resourceSnapshot'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(shared, 1);
    }

    #[test]
    fn leased_execution_prevents_partial_conversation_deletion() {
        let journal = deletion_journal();
        let mut run = journal
            .create("preserve on failure", "chat", "key", 300, None)
            .unwrap();
        journal.save(&mut run, RunState::Cancelled).unwrap();
        {
            let db = journal.connection.lock().unwrap();
            db.execute(
                "INSERT INTO runtime_attempts VALUES('attempt',?1,'call','private')",
                [&run.id],
            )
            .unwrap();
            db.execute("INSERT INTO runtime_leases VALUES('host','attempt')", [])
                .unwrap();
        }
        assert!(journal.delete_conversation("chat").is_err());
        assert_eq!(journal.conversation_messages("chat").unwrap().len(), 1);
        assert!(journal.get(&run.id).is_ok());
    }

    #[test]
    fn snapshot_pairs_compactions_and_marks_interrupted_records() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE runtime_runs(id TEXT,state TEXT);
            CREATE TABLE runtime_events(sequence INTEGER,conversation_id TEXT,run_id TEXT,kind TEXT,data TEXT);
            INSERT INTO runtime_runs VALUES ('active','\"deciding\"'),('ended','\"failed\"');
            INSERT INTO runtime_events VALUES
              (1,'chat','active','context.compaction.started','{}'),
              (2,'chat','active','context.compacted','{\"mode\":\"summary\"}'),
              (3,'chat','active','context.compacted','{\"clearedResults\":1}'),
              (4,'chat','active','context.compaction.started','{}'),
              (5,'chat','ended','context.compaction.started','{}'),
              (6,'other','active','context.compaction.started','{}');").unwrap();
        let journal = Journal {
            connection: Mutex::new(connection),
            notify: Arc::new(tokio::sync::Notify::new()),
        };
        let (activities, boundary) = journal.context_activities("chat").unwrap();
        assert_eq!(boundary, 5);
        assert_eq!(
            activities
                .iter()
                .map(|activity| (activity.id, activity.state))
                .collect::<Vec<_>>(),
            vec![(1, "completed"), (4, "running"), (5, "failed")]
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GroupLayout {
    #[serde(default)]
    pub groups: Vec<ConversationGroup>,
    #[serde(default)]
    pub membership: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub order: Vec<String>,
}

/// 会话列表的筛选与分页。默认不含已归档会话。
#[derive(Debug, Clone, Default)]
pub struct ConversationQuery {
    pub search: Option<String>,
    pub include_archived: bool,
    pub offset: usize,
    pub limit: usize,
}

impl ConversationQuery {
    pub fn normalized(&self) -> Self {
        Self {
            search: self
                .search
                .as_ref()
                .map(|value| value.trim().to_lowercase())
                .filter(|value| !value.is_empty()),
            include_archived: self.include_archived,
            offset: self.offset,
            limit: self.limit.clamp(1, 200),
        }
    }
}

pub struct Journal {
    connection: Mutex<Connection>,
    notify: Arc<tokio::sync::Notify>,
}
// 以下事务都要写：读之前先取写入预留。延迟的 WAL 读事务在另一连接提交后无法升级，
// SQLITE_BUSY_SNAPSHOT 不受 busy_timeout 约束。
impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        // 这里也要设 WAL：运行时另有自己的连接。
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
        // 问题请求以 (run_id, request_id) 为身份：requestId 用的是 tool call.id，
        // 只在任务内唯一，跨任务会重复。旧开发库是单列主键，搬完数据再替换。
        let question_table_sql: Option<String> = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='runtime_question_requests'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(sql) = question_table_sql {
            if sql.contains("PRIMARY KEY(run_id,id)") {
                // 已是新形状。
            } else {
                connection.execute_batch(
                    "ALTER TABLE runtime_question_requests RENAME TO runtime_question_requests_legacy;
                     CREATE TABLE runtime_question_requests(id TEXT NOT NULL, run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE, conversation_id TEXT NOT NULL, status TEXT NOT NULL, payload TEXT NOT NULL, answers TEXT, answer_key TEXT, created_at TEXT NOT NULL, answered_at TEXT, PRIMARY KEY(run_id,id));
                     INSERT INTO runtime_question_requests(id,run_id,conversation_id,status,payload,answers,answer_key,created_at,answered_at) SELECT id,run_id,conversation_id,status,payload,answers,answer_key,created_at,answered_at FROM runtime_question_requests_legacy;
                     DROP TABLE runtime_question_requests_legacy;",
                )?;
            }
        } else {
            connection.execute_batch(
                "CREATE TABLE runtime_question_requests(id TEXT NOT NULL, run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE, conversation_id TEXT NOT NULL, status TEXT NOT NULL, payload TEXT NOT NULL, answers TEXT, answer_key TEXT, created_at TEXT NOT NULL, answered_at TEXT, PRIMARY KEY(run_id,id));",
            )?;
        }
        connection.execute_batch(
            "CREATE INDEX IF NOT EXISTS runtime_question_requests_run ON runtime_question_requests(run_id,status);
             CREATE INDEX IF NOT EXISTS runtime_question_requests_conversation ON runtime_question_requests(conversation_id,status);",
        )?;
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

    /// 调度器与 IPC 等待方订阅这里，不轮询 SQLite。
    pub fn notifier(&self) -> Arc<tokio::sync::Notify> {
        self.notify.clone()
    }

    fn touch(&self) {
        self.notify.notify_waiters();
        self.notify.notify_one();
    }

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
        model_id: Option<&str>,
    ) -> Result<Run> {
        self.create_configured(
            prompt,
            conversation,
            key,
            duration,
            model_id,
            false,
            None,
            None,
        )
    }

    // 参数是创建一条配置运行所需的最小完整集合；引入结构体只是搬家不降复杂度，
    // 且会连带动 call site，维持现有 API。
    #[allow(clippy::too_many_arguments)]
    pub fn create_configured(
        &self,
        prompt: &str,
        conversation: &str,
        key: &str,
        duration: i64,
        model_id: Option<&str>,
        shortcut_configuration: bool,
        shortcut_target: Option<&str>,
        shortcut_reference: Option<&str>,
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
            if run.prompt != prompt
                || run.conversation_id != conversation
                || run.shortcut_configuration != shortcut_configuration
                || run.shortcut_target.as_deref() != shortcut_target
                || run.shortcut_reference.as_deref() != shortcut_reference
            {
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
            shortcut_configuration,
            shortcut_target: shortcut_target.map(str::to_owned),
            shortcut_reference: shortcut_reference.map(str::to_owned),
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
            context_tokens: 0,
            context_window: 0,
            context_compacted: false,
            cache_read_tokens: 0,
            prompt_cache_hit_tokens: 0,
            prompt_cache_hit: false,
            prompt_cache_reason: None,
            prompt_cache_snapshot: None,
            message_boundary: 0,
            result: None,
            error: None,
            source: RunSource::Agent,
            model_id: model_id.map(str::to_owned),
            discovered: Vec::new(),
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
        if let Some(model_id) = model_id {
            tx.execute(
                "UPDATE conversations SET model_id=?1 WHERE id=?2",
                params![model_id, conversation],
            )?;
        }
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
    /// clientKey 幂等预检（只读）：同 key 已有 run 时返回它，prompt/会话不一致
    /// 视为冲突。打断式重发必须在取消旧运行之前先走这里，重试不能取消任何 run。
    pub fn run_by_client_key(
        &self,
        key: &str,
        prompt: &str,
        conversation: &str,
    ) -> Result<Option<Run>> {
        let payload: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM runtime_runs WHERE client_key=?1",
                [key],
                |r| r.get(0),
            )
            .optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let run: Run = serde_json::from_str(&payload)?;
        if run.prompt != prompt || run.conversation_id != conversation {
            return Err(Error::Conflict(
                "submission key reused with different input".into(),
            ));
        }
        Ok(Some(run))
    }
    pub fn list(&self) -> Result<Vec<Run>> {
        self.query_runs("SELECT payload FROM runtime_runs ORDER BY rowid DESC LIMIT 100")
    }
    pub fn conversation_runs(&self, id: &str) -> Result<Vec<Run>> {
        let db = self.connection.lock().unwrap();
        let mut query = db.prepare("SELECT payload FROM runtime_runs WHERE conversation_id=?1")?;
        let rows = query
            .query_map([id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .collect()
    }
    pub fn pending(&self) -> Result<Vec<Run>> {
        self.query_runs("SELECT payload FROM runtime_runs WHERE state NOT IN ('\"answered\"','\"succeeded\"','\"partial\"','\"failed\"','\"cancelled\"','\"needsReview\"') ORDER BY rowid")
    }
    /// 同一会话上一条已经派发过的模型请求。普通聊天的每条用户消息会创建一个
    /// 新 Run，但 Prompt Cache 的连续性属于 Conversation，不能跟着 Run 清零。
    pub fn latest_prompt_cache_seed(
        &self,
        conversation_id: &str,
        exclude_run_id: &str,
    ) -> Result<Option<(PromptCacheSnapshot, Vec<String>)>> {
        let db = self.connection.lock().unwrap();
        let mut query = db.prepare(
            "SELECT payload FROM runtime_runs \
             WHERE conversation_id=?1 AND id<>?2 ORDER BY rowid DESC",
        )?;
        let rows = query
            .query_map(params![conversation_id, exclude_run_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for payload in rows {
            let previous: Run = serde_json::from_str(&payload)?;
            if let Some(snapshot) = previous.prompt_cache_snapshot {
                return Ok(Some((snapshot, previous.discovered)));
            }
        }
        Ok(None)
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
    /// 游标是否已经落在保留窗口之前。
    pub fn cursor_expired(&self, conversation: &str, after: u64) -> Result<bool> {
        if after == 0 {
            return Ok(false);
        }
        let earliest: Option<i64> = self.connection.lock().unwrap().query_row(
            "SELECT MIN(sequence) FROM runtime_events WHERE conversation_id=?1",
            [conversation],
            |row| row.get(0),
        )?;
        Ok(earliest.is_some_and(|earliest| after < earliest as u64 - 1))
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
    pub fn attempt_by_call(&self, run: &str, call_id: &str) -> Result<Option<Attempt>> {
        let payload: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM runtime_attempts WHERE run_id=?1 AND call_id=?2",
                params![run, call_id],
                |row| row.get(0),
            )
            .optional()?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .transpose()
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

    /// 记录待回答的结构化问题，并在同一事务里发出 question 事件与取代旧请求。
    /// 同一 run 同时最多一个 open 请求；已结束的请求不得重开。
    pub fn open_question_request(
        &self,
        run: &Run,
        request_id: &str,
        request: &Value,
        event_data: &Value,
    ) -> Result<QuestionReopen> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<(String, String, Option<String>)> = tx
            .query_row(
                "SELECT status,payload,answers FROM runtime_question_requests WHERE run_id=?1 AND id=?2",
                params![run.id, request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((status, saved_payload, saved_answers)) = existing {
            // 同一 request id 只能对应同一组题目，换题目必须换 call。
            let same_questions = serde_json::from_str::<Value>(&saved_payload)
                .map(|saved| saved["questions"] == request["questions"])
                .unwrap_or(false);
            if !same_questions {
                return Err(Error::Conflict("同一请求标识的题目不一致".into()));
            }
            return match status.as_str() {
                // open 重放：幂等，不再发待回答事件。
                "open" => {
                    tx.commit()?;
                    Ok(QuestionReopen::Open)
                }
                // answered 重放：保留答案，绝不重开。
                "answered" => {
                    tx.commit()?;
                    let answers = saved_answers
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()?
                        .unwrap_or(Value::Null);
                    Ok(QuestionReopen::Answered(answers))
                }
                // superseded 重放：保持结束，交回主循环按用户新输入继续。
                _ => {
                    tx.commit()?;
                    Ok(QuestionReopen::Superseded)
                }
            };
        }
        let stale: Vec<String> = {
            let mut q = tx.prepare(
                "SELECT id FROM runtime_question_requests WHERE run_id=?1 AND status='open' AND id<>?2",
            )?;
            q.query_map(params![run.id, request_id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &stale {
            tx.execute(
                "UPDATE runtime_question_requests SET status='superseded',answered_at=?2 WHERE run_id=?3 AND id=?1 AND status='open'",
                params![id, now(), run.id],
            )?;
            Self::insert_event(
                &tx,
                run,
                "question.superseded",
                &json!({"requestId":id,"reason":"replaced"}),
            )?;
        }
        tx.execute(
            "INSERT INTO runtime_question_requests(id,run_id,conversation_id,status,payload,created_at) VALUES(?1,?2,?3,'open',?4,?5)",
            params![request_id, run.id, run.conversation_id, request.to_string(), now()],
        )?;
        Self::insert_event(&tx, run, "question", event_data)?;
        tx.commit()?;
        self.touch();
        Ok(QuestionReopen::Opened)
    }

    /// 专用答复通道：先按请求题目归一答案，再做幂等比较；幂等合法重试优先于
    /// 状态校验。答案、状态与事件同事务落库。返回 true 表示新记录了答案。
    pub fn answer_question_request(
        &self,
        run_id: &str,
        request_id: &str,
        answers: &Value,
        key: Option<&str>,
    ) -> Result<bool> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // clientKey 绑定到 run 内的一次请求：换请求或换答案都拒绝，不跨请求串。
        if let Some(key) = key {
            let keyed: Option<String> = tx
                .query_row(
                    "SELECT id FROM runtime_question_requests WHERE run_id=?1 AND answer_key=?2",
                    params![run_id, key],
                    |r| r.get(0),
                )
                .optional()?;
            if keyed.as_deref() != Some(request_id) && keyed.is_some() {
                return Err(Error::Conflict("同一请求标识不能用于不同的问题".into()));
            }
        }
        let row: Option<(String, Option<String>, String)> = tx
            .query_row(
                "SELECT status,answers,payload FROM runtime_question_requests WHERE run_id=?1 AND id=?2",
                params![run_id, request_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((status, saved_answers, saved_payload)) = row else {
            return Err(Error::Config("问题不存在或不属于该任务".into()));
        };
        // 先按题目校验并归一：空白、未知 id、非法类型一律拒绝，之后才比较幂等。
        let questions: Vec<crate::runtime::questions::Question> = serde_json::from_value(
            serde_json::from_str::<Value>(&saved_payload)?["questions"].clone(),
        )?;
        let normalized = crate::runtime::questions::validate_answers(&questions, answers)?;
        if status == "answered" {
            let saved: Value = saved_answers
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or(Value::Null);
            return if saved == normalized {
                Ok(false)
            } else {
                Err(Error::Conflict("已回答的问题不能更改答案".into()))
            };
        }
        if status != "open" {
            return Err(Error::Conflict("该问题已失效，不能回答".into()));
        }
        // 归属与状态校验放在幂等之后：已成功请求的合法重试不能被拒绝。
        let payload: String = tx.query_row(
            "SELECT payload FROM runtime_runs WHERE id=?1",
            [run_id],
            |r| r.get(0),
        )?;
        let current: Run = serde_json::from_str(&payload)?;
        if current.state != RunState::AwaitingUser {
            return Err(Error::Conflict("当前不在等待回答".into()));
        }
        tx.execute(
            "UPDATE runtime_question_requests SET status='answered',answers=?2,answer_key=?3,answered_at=?4 WHERE run_id=?5 AND id=?1 AND status='open'",
            params![request_id, normalized.to_string(), key, now(), run_id],
        )?;
        Self::insert_event(
            &tx,
            &current,
            "question.answered",
            &json!({"requestId":request_id}),
        )?;
        tx.commit()?;
        self.touch();
        Ok(true)
    }

    /// 把 run 当前 open 的问题标记为 superseded（新补充输入接管时调用）。
    pub fn supersede_question_requests(&self, run: &Run, reason: &str) -> Result<Vec<String>> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stale: Vec<String> = {
            let mut q = tx.prepare(
                "SELECT id FROM runtime_question_requests WHERE run_id=?1 AND status='open'",
            )?;
            q.query_map(params![run.id], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &stale {
            tx.execute(
                "UPDATE runtime_question_requests SET status='superseded',answered_at=?2 WHERE run_id=?3 AND id=?1 AND status='open'",
                params![id, now(), run.id],
            )?;
            Self::insert_event(
                &tx,
                run,
                "question.superseded",
                &json!({"requestId":id,"reason":reason}),
            )?;
        }
        tx.commit()?;
        self.touch();
        Ok(stale)
    }

    /// 查询某次请求的当前状态与已存答案，user.ask 等待循环消费。
    pub fn question_request_state(
        &self,
        run_id: &str,
        request_id: &str,
    ) -> Result<Option<(String, Option<String>)>> {
        let db = self.connection.lock().unwrap();
        let row: Option<(String, Option<String>)> = db
            .query_row(
                "SELECT status,answers FROM runtime_question_requests WHERE id=?1 AND run_id=?2",
                params![request_id, run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    /// 会话当前真正 open 的问题请求，conversation.get 快照给前端，
    /// 不依赖可能已被裁剪的旧事件重放。
    /// 只算仍处于 awaitingUser 的运行：结束/取消运行留下的 open 记录不得进快照。
    pub fn open_question_requests(&self, conversation: &str) -> Result<Vec<Value>> {
        let db = self.connection.lock().unwrap();
        let mut q = db.prepare(
            "SELECT q.id,q.run_id,q.payload,q.created_at FROM runtime_question_requests q \
             JOIN runtime_runs r ON r.id=q.run_id \
             WHERE q.conversation_id=?1 AND q.status='open' AND r.state='\"awaitingUser\"' \
             ORDER BY q.rowid",
        )?;
        let rows = q
            .query_map([conversation], |r| {
                Ok((
                    r.get::<_, String>(0),
                    r.get::<_, String>(1),
                    r.get::<_, String>(2),
                    r.get::<_, String>(3),
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|row| {
                let (id, run_id, payload, created_at) = row;
                let (id, run_id, payload, created_at) = (id?, run_id?, payload?, created_at?);
                let payload: Value = serde_json::from_str(&payload)?;
                Ok(json!({
                    "requestId":id,
                    "runId":run_id,
                    "questions":payload["questions"],
                    "createdAt":created_at,
                }))
            })
            .collect()
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
        self.conversations_matching(&ConversationQuery {
            limit: 200,
            include_archived: true,
            ..ConversationQuery::default()
        })
    }

    /// 最近活动的会话在前。
    pub fn conversations_matching(
        &self,
        query: &ConversationQuery,
    ) -> Result<Vec<ConversationSummary>> {
        let query = query.normalized();
        let connection = self.connection.lock().unwrap();
        let mut statement = connection.prepare(
            "SELECT c.id,c.title,c.created_at,c.updated_at,c.pinned,c.archived_at,c.model_id,
                    (SELECT COUNT(*) FROM task_definitions t
                     WHERE t.source_conversation_id=c.id AND t.deleted=0)
             FROM conversations c
             WHERE (?1=1 OR c.archived_at IS NULL)
               AND c.id NOT LIKE 'task-%'
               AND c.id NOT LIKE 'shortcut-config-%'
               AND (?2 IS NULL OR instr(lower(c.title),?2)>0)
             ORDER BY c.updated_at DESC
             LIMIT ?3 OFFSET ?4",
        )?;
        Ok(statement
            .query_map(
                params![
                    query.include_archived as i32,
                    query.search,
                    query.limit as i64,
                    query.offset as i64
                ],
                |row| {
                    Ok(ConversationSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                        pinned: row.get::<_, i64>(4)? != 0,
                        archived: row.get::<_, Option<String>>(5)?.is_some(),
                        model_id: row.get(6)?,
                        task_count: row.get::<_, i64>(7)? as usize,
                    })
                },
            )?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn conversation(&self, id: &str) -> Result<ConversationSummary> {
        self.conversations()?
            .into_iter()
            .find(|conversation| conversation.id == id)
            .ok_or_else(|| Error::Config("会话不存在".into()))
    }

    pub fn rename_conversation(&self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > 120 {
            return Err(Error::Config("会话标题应为 1 到 120 个字符".into()));
        }
        let updated = self.connection.lock().unwrap().execute(
            "UPDATE conversations SET title=?1,updated_at=?2 WHERE id=?3",
            params![title, now(), id],
        )?;
        if updated == 0 {
            return Err(Error::Config("会话不存在".into()));
        }
        self.touch();
        Ok(())
    }

    pub fn set_conversation_pinned(&self, id: &str, pinned: bool) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE conversations SET pinned=?1 WHERE id=?2",
            params![pinned as i32, id],
        )?;
        Ok(())
    }

    pub fn set_conversation_model(&self, id: &str, model_id: Option<&str>) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE conversations SET model_id=?1 WHERE id=?2",
            params![model_id, id],
        )?;
        Ok(())
    }

    /// 未绑定或绑定配置已失效的会话，改用当前的默认模型。
    pub fn rebind_models(&self, valid: &[String], default: &str) -> Result<()> {
        let connection = self.connection.lock().unwrap();
        connection.execute(
            "UPDATE conversations SET model_id=?1 WHERE model_id IS NULL OR trim(model_id)=''",
            [default],
        )?;
        let stale: Vec<String> = {
            let mut query = connection.prepare(
                "SELECT DISTINCT model_id FROM conversations WHERE model_id IS NOT NULL",
            )?;
            query
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in stale {
            if !valid.iter().any(|known| known == &id) {
                connection.execute(
                    "UPDATE conversations SET model_id=?1 WHERE model_id=?2",
                    params![default, id],
                )?;
            }
        }
        Ok(())
    }

    /// 归档不删除消息，也不影响来源快捷任务。
    pub fn set_conversation_archived(&self, id: &str, archived: bool) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE conversations SET archived_at=?1 WHERE id=?2",
            params![if archived { Some(now()) } else { None }, id],
        )?;
        self.touch();
        Ok(())
    }

    pub fn conversation_groups(&self) -> Result<GroupLayout> {
        let connection = self.connection.lock().unwrap();
        let mut statement = connection
            .prepare("SELECT id,name,collapsed FROM conversation_groups ORDER BY position, id")?;
        let groups = statement
            .query_map([], |row| {
                Ok(ConversationGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    collapsed: row.get::<_, i64>(2)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut members = connection.prepare(
            "SELECT id, group_id FROM conversations WHERE group_id IS NOT NULL AND trim(group_id) != ''",
        )?;
        let membership = members
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|(_, group_id)| groups.iter().any(|group| group.id == *group_id))
            .collect();
        Ok(GroupLayout {
            groups,
            membership,
            order: Vec::new(),
        })
    }

    pub fn save_conversation_groups(&self, layout: &GroupLayout) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute("DELETE FROM conversation_groups", [])?;
        for (position, group) in layout.groups.iter().enumerate() {
            let name = group.name.trim();
            if name.is_empty() || group.id.trim().is_empty() {
                continue;
            }
            tx.execute(
                "INSERT INTO conversation_groups(id,name,position,collapsed) VALUES(?1,?2,?3,?4)",
                params![group.id, name, position as i64, group.collapsed as i32],
            )?;
        }
        tx.execute("UPDATE conversations SET group_id=NULL", [])?;
        for (conversation_id, group_id) in &layout.membership {
            if layout.groups.iter().any(|group| group.id == *group_id) {
                tx.execute(
                    "UPDATE conversations SET group_id=?1 WHERE id=?2",
                    params![group_id, conversation_id],
                )?;
            }
        }
        tx.commit()?;
        drop(db);
        self.touch();
        Ok(())
    }

    /// 删除会话：只删这个会话的消息与运行记录，独立保存的快捷任务保留。
    pub fn delete_conversation(&self, id: &str) -> Result<Value> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let messages: i64 = tx.query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id=?1",
            [id],
            |row| row.get(0),
        )?;
        let runs: i64 = tx.query_row(
            "SELECT COUNT(*) FROM runtime_runs WHERE conversation_id=?1",
            [id],
            |row| row.get(0),
        )?;
        let tasks: i64 = tx.query_row(
            "SELECT COUNT(*) FROM task_definitions WHERE source_conversation_id=?1 AND deleted=0",
            [id],
            |row| row.get(0),
        )?;
        let leased: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_leases WHERE attempt_id IN
            (SELECT id FROM runtime_attempts WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)))
            OR EXISTS(SELECT 1 FROM runtime_resource_leases WHERE operation_id IN
            (SELECT id FROM runtime_operations WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)))", [id], |row| row.get(0))?;
        if leased {
            return Err(Error::Conflict(
                "这个对话仍有未结束的执行，请先停止执行再删除".into(),
            ));
        }
        tx.execute("DELETE FROM runtime_artifact_refs WHERE (owner_kind='run' AND owner_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1))
            OR (owner_kind='operation' AND owner_id IN (SELECT id FROM runtime_operations WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)))", [id])?;
        tx.execute("DELETE FROM runtime_evidence WHERE operation_id IN (SELECT id FROM runtime_operations WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1))",[id])?;
        tx.execute("DELETE FROM runtime_operation_events WHERE operation_id IN (SELECT id FROM runtime_operations WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1))",[id])?;
        tx.execute("DELETE FROM runtime_operations WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)",[id])?;
        tx.execute("DELETE FROM runtime_notifications WHERE json_extract(payload,'$.target.conversationId')=?1 OR json_extract(payload,'$.target.runId') IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)",[id])?;
        tx.execute("DELETE FROM runtime_metrics WHERE json_extract(payload,'$.labels.conversationId')=?1 OR json_extract(payload,'$.labels.runId') IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)",[id])?;
        for table in [
            "runtime_attempts",
            "runtime_approvals",
            "runtime_plans",
            "runtime_inputs",
            "runtime_input_keys",
            "runtime_artifacts",
            "runtime_checkpoints",
            "runtime_workflow_runs",
            "task_runs",
        ] {
            tx.execute(&format!("DELETE FROM {table} WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)"),[id])?;
        }
        tx.execute("DELETE FROM runtime_events WHERE conversation_id=?1", [id])?;
        tx.execute("DELETE FROM runtime_preferences WHERE scope=?1", [id])?;
        tx.execute(
            "DELETE FROM runtime_message_owners WHERE run_id IN (SELECT id FROM runtime_runs WHERE conversation_id=?1)",
            [id],
        )?;
        tx.execute("DELETE FROM runtime_runs WHERE conversation_id=?1", [id])?;
        tx.execute("DELETE FROM messages WHERE conversation_id=?1", [id])?;
        tx.execute("DELETE FROM tool_calls WHERE conversation_id=?1", [id])?;
        tx.execute("DELETE FROM conversations WHERE id=?1", [id])?;
        tx.commit()?;
        drop(db);
        self.touch();
        Ok(json!({
            "deleted":true,
            "messages":messages,
            "runs":runs,
            "tasksKept":tasks,
            "permanent":true,
        }))
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
        let (summary, entries) = self.history_window(run)?;
        let mut messages = summary
            .as_ref()
            .map(|summary| vec![crate::runtime::context::summary_message(&summary.content)])
            .unwrap_or_default();
        messages.extend(entries.into_iter().map(|entry| entry.message));
        Ok(messages)
    }

    pub fn history_window(
        &self,
        run: &Run,
    ) -> Result<(
        Option<crate::runtime::context::StoredSummary>,
        Vec<crate::runtime::context::HistoryEntry>,
    )> {
        let db = self.connection.lock().unwrap();
        let summary = db
            .query_row(
                "SELECT through_sort_key,through_message_id,summary
                 FROM conversation_compactions
                 WHERE conversation_id=?1 AND through_sort_key<=?2",
                params![run.conversation_id, run.message_boundary],
                |row| {
                    Ok(crate::runtime::context::StoredSummary {
                        through_sort_key: row.get(0)?,
                        through_message_id: row.get(1)?,
                        content: row.get(2)?,
                    })
                },
            )
            .optional()?;
        let lower_sort = summary
            .as_ref()
            .map_or(i64::MIN, |value| value.through_sort_key);
        let lower_id = summary
            .as_ref()
            .map_or(i64::MIN, |value| value.through_message_id);
        let mut query = db.prepare(
            "SELECT COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id),
                    m.id,m.role,m.content,m.tool_call_id,m.tool_calls_json,m.reasoning_json
             FROM messages m
             LEFT JOIN runtime_message_owners o ON o.message_id=m.id
             LEFT JOIN runtime_runs r ON r.id=o.run_id
             WHERE m.conversation_id=?1
               AND COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id)<=?2
               AND (COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id)>?3
                    OR (COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id)=?3 AND m.id>?4))
             ORDER BY COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id),m.id",
        )?;
        let rows = query
            .query_map(
                params![
                    run.conversation_id,
                    run.message_boundary,
                    lower_sort,
                    lower_id
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let entries = rows
            .into_iter()
            .map(
                |(sort_key, id, role, content, tool_call_id, calls, reasoning)| {
                    Ok(crate::runtime::context::HistoryEntry {
                        sort_key,
                        id,
                        message: crate::model::Message {
                            role: serde_json::from_value(json!(role))?,
                            content,
                            tool_call_id,
                            tool_calls: serde_json::from_str(calls.as_deref().unwrap_or("[]"))?,
                            reasoning: serde_json::from_str(
                                reasoning.as_deref().unwrap_or("null"),
                            )?,
                        },
                    })
                },
            )
            .collect::<Result<Vec<_>>>()?;
        // Only the model projection closes missing tool replies from ended turns.
        // Keep stored chat/audit messages and real tool results unchanged.
        let mut ended_query = db.prepare(
            "SELECT json_extract(payload,'$.messageBoundary') FROM runtime_runs
             WHERE conversation_id=?1 AND state IN ('\"cancelled\"','\"failed\"','\"needsReview\"')",
        )?;
        let ended = ended_query
            .query_map([&run.conversation_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()?;
        let mut projected = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            projected.push(entry.clone());
            if !ended.contains(&entry.sort_key) || entry.message.tool_calls.is_empty() {
                continue;
            }
            // Results belonging to this assistant call must precede the next
            // assistant/user message; already recorded results remain authoritative.
            let replies = entries[index + 1..]
                .iter()
                .take_while(|next| next.message.role == crate::model::Role::Tool)
                .filter_map(|next| next.message.tool_call_id.as_deref())
                .collect::<std::collections::HashSet<_>>();
            // Append missing replies after the existing contiguous tool messages.
            // Inserting before them is also protocol-valid and preserves their order.
            for call in &entry.message.tool_calls {
                if replies.contains(call.id.as_str()) {
                    continue;
                }
                let mut reply = entry.clone();
                reply.message = crate::model::Message {
                    role: crate::model::Role::Tool,
                    content: json!({"ok":false,"executionStatus":"unknown",
                        "error":"上一轮已结束，未取得这个调用的最终结果。不能据此判断操作是否执行，也不要直接重复写入；需要时先读取当前状态。"}).to_string(),
                    tool_call_id: Some(call.id.clone()),
                    tool_calls: vec![],
                    reasoning: None,
                };
                projected.push(reply);
            }
        }
        Ok((summary, projected))
    }

    pub fn save_compaction(
        &self,
        run: &Run,
        plan: &crate::runtime::context::CompactionPlan,
        summary: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "INSERT INTO conversation_compactions(
                 conversation_id,run_id,through_sort_key,through_message_id,summary,created_at
             ) VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(conversation_id) DO UPDATE SET
                 run_id=excluded.run_id,
                 through_sort_key=excluded.through_sort_key,
                 through_message_id=excluded.through_message_id,
                 summary=excluded.summary,
                 created_at=excluded.created_at
             WHERE excluded.through_sort_key>conversation_compactions.through_sort_key
                OR (excluded.through_sort_key=conversation_compactions.through_sort_key
                    AND excluded.through_message_id>conversation_compactions.through_message_id)",
            params![
                run.conversation_id,
                run.id,
                plan.through_sort_key,
                plan.through_message_id,
                summary,
                now()
            ],
        )?;
        if changed == 1 {
            Self::insert_event(
                &tx,
                run,
                "context.compacted",
                &json!({
                    "mode":"summary",
                    "throughMessageId":plan.through_message_id,
                    "inputTokens":input_tokens,
                    "outputTokens":output_tokens,
                }),
            )?;
        }
        tx.commit()?;
        self.touch();
        Ok(())
    }
    /// Load compaction records before the first paint; event replay must not reopen old process panels.
    pub fn context_activities(&self, conversation: &str) -> Result<(Vec<ContextActivity>, u64)> {
        let db = self.connection.lock().unwrap();
        let mut query = db.prepare("SELECT e.sequence,e.run_id,e.kind,r.state FROM runtime_events e
            LEFT JOIN runtime_runs r ON r.id=e.run_id WHERE e.conversation_id=?1 AND
            (e.kind='context.compaction.started' OR (e.kind='context.compacted' AND json_extract(e.data,'$.mode')='summary'))
            ORDER BY e.sequence")?;
        let rows = query.query_map([conversation], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut activities: Vec<ContextActivity> = vec![];
        let mut ended = std::collections::HashSet::new();
        let mut boundary = 0;
        for row in rows {
            let (id, run_id, kind, state) = row?;
            boundary = id;
            if state
                .as_deref()
                .and_then(|state| serde_json::from_str::<RunState>(state).ok())
                .is_some_and(|state| state.terminal() || state == RunState::Blocked)
            {
                ended.insert(run_id.clone());
            }
            if kind == "context.compaction.started" {
                activities.push(ContextActivity {
                    id,
                    run_id,
                    state: "running",
                });
            } else if let Some(activity) = activities
                .iter_mut()
                .rev()
                .find(|activity| activity.run_id == run_id && activity.state == "running")
            {
                activity.state = "completed";
            } else {
                activities.push(ContextActivity {
                    id,
                    run_id,
                    state: "completed",
                });
            }
        }
        for activity in &mut activities {
            if activity.state == "running" && ended.contains(&activity.run_id) {
                activity.state = "failed";
            }
        }
        Ok((activities, boundary))
    }

    pub fn conversation_messages(&self, conversation: &str) -> Result<Vec<ConversationMessage>> {
        let db = self.connection.lock().unwrap();
        // Legacy rows have no stored boundary. A completed event always follows
        // persistence, so it safely covers prior deltas without guessing text.
        let mut query = db.prepare(
            "SELECT m.role,m.content,m.tool_call_id,m.tool_calls_json,m.reasoning_json,m.created_at,o.run_id,
                    COALESCE(m.stream_boundary,completed.boundary)
             FROM messages m
             LEFT JOIN runtime_message_owners o ON o.message_id=m.id
             LEFT JOIN runtime_runs r ON r.id=o.run_id
             LEFT JOIN (SELECT run_id,MAX(sequence) AS boundary FROM runtime_events
                        WHERE conversation_id=?1 AND kind='assistant.completed' GROUP BY run_id) completed
                    ON completed.run_id=o.run_id
             WHERE m.conversation_id=?1
               AND (r.state IS NULL OR r.state!='\"queued\"')
             ORDER BY COALESCE(json_extract(r.payload,'$.messageBoundary'),m.id),m.id",
        )?;
        let rows = query
            .query_map([conversation], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<u64>>(7)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(
                |(
                    role,
                    content,
                    tool_call_id,
                    calls,
                    reasoning,
                    created_at,
                    run_id,
                    stream_boundary,
                )| {
                    Ok(ConversationMessage {
                        message: crate::model::Message {
                            role: serde_json::from_value(json!(role))?,
                            content,
                            tool_call_id,
                            tool_calls: serde_json::from_str(calls.as_deref().unwrap_or("[]"))?,
                            reasoning: serde_json::from_str(
                                reasoning.as_deref().unwrap_or("null"),
                            )?,
                        },
                        created_at,
                        run_id,
                        stream_boundary,
                    })
                },
            )
            .collect()
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
        let message_id = tx.last_insert_rowid();
        if matches!(message.role, crate::model::Role::Assistant) {
            tx.execute(
                "UPDATE messages SET stream_boundary=(SELECT COALESCE(MAX(sequence),0) FROM runtime_events WHERE run_id=?1 AND kind='assistant.delta') WHERE id=?2",
                params![run.id, message_id],
            )?;
        }
        tx.execute(
            "INSERT INTO runtime_message_owners VALUES(?1,?2)",
            params![message_id, run.id],
        )?;
        tx.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now(), run.conversation_id],
        )?;
        tx.commit()?;
        Ok(())
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
            discovered: run.discovered.clone(),
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
            None,
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
        task_id: &str,
        name: &str,
        revision: u64,
        key: &str,
        duration: i64,
    ) -> Result<Run> {
        let conversation = format!("task-{task_id}");
        let mut run = self.create(
            &format!("运行快捷任务：{name}"),
            &conversation,
            key,
            duration,
            None,
        )?;
        run.source = RunSource::SavedWorkflow {
            workflow_id: task_id.to_owned(),
            workflow_revision: revision,
        };
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_runs SET payload=?1 WHERE id=?2",
            params![serde_json::to_string(&run)?, run.id],
        )?;
        Ok(run)
    }
}

#[cfg(test)]
mod question_request_tests {
    use super::*;
    use std::path::PathBuf;

    fn journal(name: &str) -> (Journal, PathBuf) {
        // 统一放本任务登记的 target/.tmp/shortcut-runtime 根内。
        let dir = PathBuf::from("target/.tmp/shortcut-runtime/question-journal-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.db"));
        // Journal::open 首次建表会写 VACUUM 备份，按前缀一并清掉上次残留。
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with(name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        (Journal::open(&path).unwrap(), path)
    }

    fn run_to_awaiting(journal: &Journal, id: &str, conversation: &str) -> Run {
        let mut run = journal
            .create(
                "测试目标",
                conversation,
                format!("key-{id}").as_str(),
                3600,
                None,
            )
            .unwrap();
        for next in [
            RunState::Preflighting,
            RunState::Deciding,
            RunState::AwaitingUser,
        ] {
            journal.save(&mut run, next).unwrap();
        }
        run
    }

    fn request(id: &str) -> Value {
        json!({"requestId":id,"questions":[
            {"id":"mode","header":"运行方式","question":"怎么跑？","options":[{"label":"启动","description":"立即运行"},{"label":"检查"}]}
        ]})
    }

    fn answers() -> Value {
        json!({"mode":{"answers":["启动"]}})
    }

    fn event_count(journal: &Journal, kind: &str) -> i64 {
        journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM runtime_events WHERE kind=?1",
                [kind],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn user_message_count(journal: &Journal) -> i64 {
        journal
            .connection
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM messages WHERE role='user'", [], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn status(journal: &Journal, run_id: &str, request_id: &str) -> String {
        journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM runtime_question_requests WHERE run_id=?1 AND id=?2",
                params![run_id, request_id],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn same_call_id_coexists_across_runs() {
        let (journal, path) = journal("cross-run");
        let a = run_to_awaiting(&journal, "run-a", "c-a");
        let b = run_to_awaiting(&journal, "run-b", "c-b");
        // 真实 tool call.id 可跨任务重复：同 id 并存，互不串。
        journal
            .open_question_request(&a, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        journal
            .open_question_request(&b, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        assert!(
            journal
                .answer_question_request(&a.id, "call-1", &answers(), Some("client-1"))
                .unwrap()
        );
        assert_eq!(status(&journal, &a.id, "call-1"), "answered");
        // b 的同名请求不受影响，仍 open；clientKey 按 run 作用域，同名 key 在 b 内重新绑定。
        assert!(
            journal
                .answer_question_request(
                    &b.id,
                    "call-1",
                    &json!({"mode":{"answers":["检查"]}}),
                    Some("client-1")
                )
                .unwrap()
        );
        assert_eq!(status(&journal, &b.id, "call-1"), "answered");
        // b 内同 key 换答案拒绝。
        assert!(
            journal
                .answer_question_request(&b.id, "call-1", &answers(), Some("client-1"))
                .is_err()
        );
        // clientKey 在 run 内绑定到具体请求：a 的 key 再用于 a 的另一个请求要拒绝。
        journal
            .open_question_request(&a, "call-2", &request("call-2"), &json!({"question":"q"}))
            .unwrap();
        assert!(
            journal
                .answer_question_request(&a.id, "call-2", &answers(), Some("client-1"))
                .is_err()
        );
        // 其他 run / 未知请求不能答。
        assert!(
            journal
                .answer_question_request(&b.id, "call-2", &answers(), None)
                .is_err()
        );
        assert!(
            journal
                .answer_question_request(&a.id, "call-none", &answers(), None)
                .is_err()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn answer_records_without_user_message_and_retries_stay_idempotent() {
        let (journal, path) = journal("answer-roundtrip");
        let run = run_to_awaiting(&journal, "run-a", "c-a");
        journal
            .open_question_request(
                &run,
                "call-1",
                &request("call-1"),
                &json!({"question":"怎么跑？","request":request("call-1")}),
            )
            .unwrap();
        // 刷新重放：question 事件带结构化 request。
        let stored: String = journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT data FROM runtime_events WHERE kind='question' AND run_id=?1",
                [&run.id],
                |r| r.get(0),
            )
            .unwrap();
        let stored: Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(stored["request"]["requestId"], json!("call-1"));
        assert_eq!(stored["request"]["questions"][0]["id"], json!("mode"));
        assert_eq!(stored["question"], json!("怎么跑？"));

        let before = user_message_count(&journal);
        assert!(
            journal
                .answer_question_request(&run.id, "call-1", &answers(), Some("client-1"))
                .unwrap()
        );
        // 答复不新增 user 聊天消息，状态与事件同事务落库。
        assert_eq!(user_message_count(&journal), before);
        assert_eq!(status(&journal, &run.id, "call-1"), "answered");
        assert_eq!(event_count(&journal, "question.answered"), 1);

        // 归一后幂等：首答带空白，合法重试不带空白也命中，不冲突。
        assert!(
            !journal
                .answer_question_request(
                    &run.id,
                    "call-1",
                    &json!({"mode":{"answers":[" 启动 "]}}),
                    Some("client-1")
                )
                .unwrap()
        );
        assert_eq!(event_count(&journal, "question.answered"), 1);
        // 无 key 的同答案重试同样幂等成功。
        assert!(
            !journal
                .answer_question_request(&run.id, "call-1", &answers(), None)
                .unwrap()
        );
        // 同 key 不同答案拒绝。
        assert!(
            journal
                .answer_question_request(
                    &run.id,
                    "call-1",
                    &json!({"mode":{"answers":["检查"]}}),
                    Some("client-1")
                )
                .is_err()
        );
        // 已成功请求的合法重试不能被 terminal 状态挡住。
        let mut run = run;
        journal.finish(&mut run, RunState::Answered).unwrap();
        assert!(
            !journal
                .answer_question_request(&run.id, "call-1", &answers(), Some("client-2"))
                .unwrap()
        );
        // 已回答后改答案拒绝；非法类型与未知 id 一律拒绝。
        assert!(
            journal
                .answer_question_request(
                    &run.id,
                    "call-1",
                    &json!({"mode":{"answers":["检查"]}}),
                    Some("client-2")
                )
                .is_err()
        );
        for invalid in [
            json!({"mode":{"answers":["启动"]},"unknown":{"answers":["x"]}}),
            json!({"mode":{"answers":[true]}}),
            json!({"mode":"启动"}),
            json!({}),
        ] {
            assert!(
                journal
                    .answer_question_request(&run.id, "call-1", &invalid, None)
                    .is_err()
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reopen_preserves_state_and_never_reopens_answered() {
        let (journal, path) = journal("reopen");
        let run = run_to_awaiting(&journal, "run-r", "c-r");
        journal
            .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        // open 重放：幂等，不重复发 question 事件。
        assert!(matches!(
            journal
                .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
                .unwrap(),
            QuestionReopen::Open
        ));
        assert_eq!(event_count(&journal, "question"), 1);
        // 同 id 换题目拒绝。
        let other =
            json!({"requestId":"call-1","questions":[{"id":"else","header":"h","question":"q"}]});
        assert!(
            journal
                .open_question_request(&run, "call-1", &other, &json!({"question":"q"}))
                .is_err()
        );
        journal
            .answer_question_request(&run.id, "call-1", &answers(), Some("client-1"))
            .unwrap();
        // answered 重放：保留答案，不重开、不再发事件。
        match journal
            .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap()
        {
            QuestionReopen::Answered(answers) => {
                assert_eq!(answers["mode"]["answers"][0], json!("启动"))
            }
            other => panic!("应保留答案，实际 {other:?}"),
        }
        assert_eq!(status(&journal, &run.id, "call-1"), "answered");
        assert_eq!(event_count(&journal, "question"), 1);
        // superseded 重放：保持结束，不再打开。
        journal
            .open_question_request(&run, "call-2", &request("call-2"), &json!({"question":"q"}))
            .unwrap();
        assert!(matches!(
            journal
                .open_question_request(&run, "call-2", &request("call-2"), &json!({"question":"q"}))
                .unwrap(),
            QuestionReopen::Open
        ));
        journal
            .supersede_question_requests(&run, "supplemented")
            .unwrap();
        assert!(matches!(
            journal
                .open_question_request(&run, "call-2", &request("call-2"), &json!({"question":"q"}))
                .unwrap(),
            QuestionReopen::Superseded
        ));
        assert_eq!(status(&journal, &run.id, "call-2"), "superseded");
        assert!(
            journal
                .answer_question_request(&run.id, "call-2", &answers(), None)
                .is_err()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn supplement_supersedes_and_stays_unconsumed_for_main_loop() {
        let (journal, path) = journal("supplement");
        let run = run_to_awaiting(&journal, "run-b", "c-b");
        journal
            .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        // 真实新补充到达（run.input 不区分等待状态）。
        journal
            .input_once(&run.id, "supplement", "算了，直接跑默认", Some("s-1"))
            .unwrap();
        assert_eq!(
            journal
                .supersede_question_requests(&run, "supplemented")
                .unwrap(),
            vec!["call-1".to_string()]
        );
        assert_eq!(status(&journal, &run.id, "call-1"), "superseded");
        assert!(
            journal
                .answer_question_request(&run.id, "call-1", &answers(), Some("client-1"))
                .is_err()
        );
        // 补充未被 user.ask 消费：留给主循环按 Role::User 取用。
        assert!(journal.has_inputs(&run.id).unwrap());
        assert_eq!(
            journal.drain_inputs(&run.id).unwrap(),
            vec!["算了，直接跑默认".to_string()]
        );
        // 答复优先完成后同时到来的补充也保留：回答不动 inputs。
        journal
            .open_question_request(&run, "call-2", &request("call-2"), &json!({"question":"q"}))
            .unwrap();
        journal
            .input_once(&run.id, "supplement", "顺便加一条", Some("s-2"))
            .unwrap();
        journal
            .answer_question_request(&run.id, "call-2", &answers(), Some("client-2"))
            .unwrap();
        assert!(journal.has_inputs(&run.id).unwrap());
        assert_eq!(
            journal.drain_inputs(&run.id).unwrap(),
            vec!["顺便加一条".to_string()]
        );
        // 同文新请求可再次回答。
        journal
            .open_question_request(&run, "call-3", &request("call-3"), &json!({"question":"q"}))
            .unwrap();
        assert!(
            journal
                .answer_question_request(&run.id, "call-3", &answers(), Some("client-1"))
                .unwrap()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn awaiting_state_required_and_answers_validated() {
        let (journal, path) = journal("ownership");
        let run = run_to_awaiting(&journal, "run-c", "c-c");
        journal
            .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        // 不在 AwaitingUser 时不能答。
        let mut run = run;
        journal.save(&mut run, RunState::Deciding).unwrap();
        assert!(
            journal
                .answer_question_request(&run.id, "call-1", &answers(), None)
                .is_err()
        );
        journal.save(&mut run, RunState::Executing).unwrap();
        journal.save(&mut run, RunState::AwaitingUser).unwrap();
        for invalid in [
            json!({}),
            json!({"mode":{"answers":["启动"]},"unknown":{"answers":["x"]}}),
            json!({"mode":{"answers":[" "]}}),
            json!({"mode":{"answers":[]}}),
        ] {
            assert!(
                journal
                    .answer_question_request(&run.id, "call-1", &invalid, None)
                    .is_err()
            );
        }
        // 回到等待后同一 open 请求仍可答。
        assert!(
            journal
                .answer_question_request(&run.id, "call-1", &answers(), None)
                .unwrap()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn opening_new_request_replaces_still_open_one() {
        let (journal, path) = journal("replace");
        let run = run_to_awaiting(&journal, "run-e", "c-e");
        journal
            .open_question_request(&run, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        journal
            .open_question_request(&run, "call-2", &request("call-2"), &json!({"question":"q"}))
            .unwrap();
        assert_eq!(status(&journal, &run.id, "call-1"), "superseded");
        assert_eq!(status(&journal, &run.id, "call-2"), "open");
        assert!(
            journal
                .answer_question_request(&run.id, "call-1", &answers(), None)
                .is_err()
        );
        assert!(
            journal
                .answer_question_request(&run.id, "call-2", &answers(), None)
                .unwrap()
        );
        assert_eq!(event_count(&journal, "question.superseded"), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_single_pk_table_migrates_with_data() {
        let dir = PathBuf::from("target/.tmp/shortcut-runtime/question-journal-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("legacy-migrate.db");
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        // 先用正常 open 建全库，再把问题请求表降级回旧形状（单列主键）。
        let legacy_run_id;
        {
            let bootstrap = Journal::open(&path).unwrap();
            let mut run = bootstrap
                .create("旧目标", "c-old", "key-old", 3600, None)
                .unwrap();
            legacy_run_id = run.id.clone();
            for next in [
                RunState::Preflighting,
                RunState::Deciding,
                RunState::AwaitingUser,
            ] {
                bootstrap.save(&mut run, next).unwrap();
            }
            bootstrap
                .connection
                .lock()
                .unwrap()
                .execute_batch(
                    "DROP INDEX IF EXISTS runtime_question_requests_conversation;
                     DROP INDEX IF EXISTS runtime_question_requests_run;
                     DROP TABLE runtime_question_requests;
                     CREATE TABLE runtime_question_requests(id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runtime_runs(id) ON DELETE CASCADE, conversation_id TEXT NOT NULL, status TEXT NOT NULL, payload TEXT NOT NULL, answers TEXT, answer_key TEXT, created_at TEXT NOT NULL, answered_at TEXT);",
                )
                .unwrap();
            bootstrap
                .connection
                .lock()
                .unwrap()
                .execute(
                    "INSERT INTO runtime_question_requests(id,run_id,conversation_id,status,payload,created_at) VALUES('call-old',?1,'c-old','answered','{\"requestId\":\"call-old\",\"questions\":[{\"id\":\"mode\",\"header\":\"h\",\"question\":\"q\"}]}','2026-01-01T00:00:00Z')",
                    params![run.id],
                )
                .unwrap();
        }
        // 打开时迁移：数据保留，且新库允许同 id 存在于不同 run。
        let journal = Journal::open(&path).unwrap();
        let (status, payload): (String, String) = journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT status,payload FROM runtime_question_requests WHERE run_id=?1 AND id='call-old'",
                [&legacy_run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "answered");
        // 已回答的旧数据原样保留，不因迁移被重开或清空。
        let payload: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["questions"][0]["id"], json!("mode"));
        let count: i64 = journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM runtime_question_requests WHERE id='call-old'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn conversation_snapshot_lists_open_requests() {
        let (journal, path) = journal("snapshot");
        let a = run_to_awaiting(&journal, "run-s1", "c-s");
        let b = run_to_awaiting(&journal, "run-s2", "c-s");
        journal
            .open_question_request(&a, "call-1", &request("call-1"), &json!({"question":"q"}))
            .unwrap();
        journal
            .open_question_request(&b, "call-2", &request("call-2"), &json!({"question":"q"}))
            .unwrap();
        let open = journal.open_question_requests("c-s").unwrap();
        assert_eq!(open.len(), 2);
        assert_eq!(open[0]["requestId"], json!("call-1"));
        assert_eq!(open[0]["runId"], json!(a.id));
        assert_eq!(open[0]["questions"][0]["id"], json!("mode"));
        assert!(open[0]["createdAt"].as_str().is_some());
        // 已回答的从快照消失。
        journal
            .answer_question_request(&a.id, "call-1", &answers(), None)
            .unwrap();
        let open = journal.open_question_requests("c-s").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["requestId"], json!("call-2"));
        // 其他会话不受影响。
        assert!(
            journal
                .open_question_requests("c-other")
                .unwrap()
                .is_empty()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cancelled_run_hides_open_requests_from_snapshot() {
        let (journal, path) = journal("cancelq");
        let mut a = run_to_awaiting(&journal, "run-x1", "c-x");
        let b = run_to_awaiting(&journal, "run-x2", "c-x");
        journal
            .open_question_request(&a, "call-x1", &request("call-x1"), &json!({"question":"q"}))
            .unwrap();
        journal
            .open_question_request(&b, "call-x2", &request("call-x2"), &json!({"question":"q"}))
            .unwrap();
        assert_eq!(journal.open_question_requests("c-x").unwrap().len(), 2);
        // run a 走取消路径：cancelling 起快照就不再返回它的 open 请求。
        journal.save(&mut a, RunState::Cancelling).unwrap();
        let open = journal.open_question_requests("c-x").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["requestId"], json!("call-x2"));
        journal.finish(&mut a, RunState::Cancelled).unwrap();
        // 终态后依旧不复活；仍 awaitingUser 的 run b 不受影响。
        let open = journal.open_question_requests("c-x").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0]["requestId"], json!("call-x2"));
        // 旧记录保留历史，不因快照过滤被改写或删除。
        let status: String = journal
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM runtime_question_requests WHERE run_id=?1 AND id='call-x1'",
                [&a.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "open");
        let _ = std::fs::remove_file(&path);
    }
}
