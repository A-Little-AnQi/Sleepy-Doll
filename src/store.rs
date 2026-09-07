use std::{fs, path::Path, sync::Mutex};

use rusqlite::{Connection, params};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    error::{Error, Result},
    model::{Message, Role, ToolCall},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub conversation_id: String,
    pub prompt: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub struct Store {
    connection: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        connection.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS conversations (id TEXT PRIMARY KEY, title TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE, role TEXT NOT NULL, content TEXT NOT NULL, tool_call_id TEXT, tool_calls_json TEXT, created_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS messages_by_conversation ON messages(conversation_id, id);
            CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE, prompt TEXT NOT NULL, state TEXT NOT NULL, result TEXT, error TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS tasks_by_update ON tasks(updated_at DESC);
            CREATE TABLE IF NOT EXISTS tool_calls (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE, call_id TEXT NOT NULL, tool_name TEXT NOT NULL, arguments_json TEXT NOT NULL, result_json TEXT NOT NULL, created_at TEXT NOT NULL);
        "#)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn ensure_conversation(&self, id: &str, title: &str) -> Result<()> {
        let now = now();
        let connection = self.connection.lock().expect("store mutex poisoned");
        connection.execute("INSERT OR IGNORE INTO conversations (id,title,created_at,updated_at) VALUES (?1,?2,?3,?3)", params![id, truncate(title, 120), now])?;
        Ok(())
    }

    pub fn conversations(&self) -> Result<Vec<ConversationSummary>> {
        let connection = self.connection.lock().expect("store mutex poisoned");
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

    pub fn messages(&self, conversation_id: &str) -> Result<Vec<Message>> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut query = connection.prepare("SELECT role,content,tool_call_id,tool_calls_json FROM messages WHERE conversation_id=?1 ORDER BY id")?;
        Ok(query
            .query_map([conversation_id], |row| {
                let role: String = row.get(0)?;
                let calls: Option<String> = row.get(3)?;
                Ok(Message {
                    role: parse_role(&role),
                    content: row.get(1)?,
                    tool_call_id: row.get(2)?,
                    tool_calls: calls
                        .and_then(|value| serde_json::from_str::<Vec<ToolCall>>(&value).ok())
                        .unwrap_or_default(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn append_message(&self, conversation_id: &str, message: &Message) -> Result<()> {
        let now = now();
        let connection = self.connection.lock().expect("store mutex poisoned");
        connection.execute("INSERT INTO messages (conversation_id,role,content,tool_call_id,tool_calls_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![conversation_id, role(message.role), message.content, message.tool_call_id, if message.tool_calls.is_empty() { None } else { Some(serde_json::to_string(&message.tool_calls)?) }, now])?;
        connection.execute(
            "UPDATE conversations SET updated_at=?1 WHERE id=?2",
            params![now, conversation_id],
        )?;
        Ok(())
    }

    pub fn record_tool(
        &self,
        conversation_id: &str,
        call: &ToolCall,
        result: &serde_json::Value,
    ) -> Result<()> {
        self.connection.lock().expect("store mutex poisoned").execute("INSERT INTO tool_calls (conversation_id,call_id,tool_name,arguments_json,result_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)", params![conversation_id, call.id, call.name, call.arguments.to_string(), result.to_string(), now()])?;
        Ok(())
    }

    pub fn create_task(&self, prompt: &str, conversation_id: Option<String>) -> Result<TaskRecord> {
        let conversation_id = conversation_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        self.ensure_conversation(&conversation_id, prompt)?;
        let id = Uuid::new_v4().to_string();
        let now = now();
        self.connection.lock().expect("store mutex poisoned").execute("INSERT INTO tasks (id,conversation_id,prompt,state,created_at,updated_at) VALUES (?1,?2,?3,'queued',?4,?4)", params![id, conversation_id, prompt, now])?;
        self.task(&id)?.ok_or_else(|| Error::Conflict("task disappeared after insert".into()))
    }

    pub fn task(&self, id: &str) -> Result<Option<TaskRecord>> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut query = connection.prepare("SELECT id,conversation_id,prompt,state,result,error,created_at,updated_at FROM tasks WHERE id=?1")?;
        let mut rows = query.query([id])?;
        Ok(rows
            .next()?
            .map(|row| -> Result<TaskRecord> {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    prompt: row.get(2)?,
                    state: row.get(3)?,
                    result: row.get(4)?,
                    error: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })
            .transpose()?)
    }

    pub fn tasks(&self) -> Result<Vec<TaskRecord>> {
        let connection = self.connection.lock().expect("store mutex poisoned");
        let mut query = connection.prepare("SELECT id,conversation_id,prompt,state,result,error,created_at,updated_at FROM tasks ORDER BY updated_at DESC LIMIT 50")?;
        Ok(query
            .query_map([], |row| {
                Ok(TaskRecord {
                    id: row.get(0)?,
                    conversation_id: row.get(1)?,
                    prompt: row.get(2)?,
                    state: row.get(3)?,
                    result: row.get(4)?,
                    error: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn update_task(
        &self,
        id: &str,
        state: &str,
        result: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.connection
            .lock()
            .expect("store mutex poisoned")
            .execute(
                "UPDATE tasks SET state=?1,result=?2,error=?3,updated_at=?4 WHERE id=?5",
                params![state, result, error, now(), id],
            )?;
        Ok(())
    }
}

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}
fn truncate(value: &str, length: usize) -> String {
    value.chars().take(length).collect()
}
fn role(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}
fn parse_role(value: &str) -> Role {
    match value {
        "system" => Role::System,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}
