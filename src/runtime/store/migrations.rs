use rusqlite::Connection;

use crate::error::Result;

pub const LATEST_SCHEMA_VERSION: i64 = 13;

pub fn migrate(connection: &mut Connection) -> Result<()> {
    connection.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;
         CREATE TABLE IF NOT EXISTS schema_migrations(
             version INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         );",
    )?;
    let mut current: i64 = connection.query_row(
        "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        let tx = connection.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            [version],
        )?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        current = *version;
    }
    Ok(())
}

const MIGRATIONS: &[(i64, &str)] = &[
    (
        3,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_operations(
            id TEXT PRIMARY KEY,
            run_id TEXT,
            provider_id TEXT NOT NULL,
            state TEXT NOT NULL,
            revision INTEGER NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS runtime_operations_by_update ON runtime_operations(updated_at DESC);
        CREATE TABLE IF NOT EXISTS runtime_operation_events(
            sequence INTEGER PRIMARY KEY AUTOINCREMENT,
            operation_id TEXT NOT NULL REFERENCES runtime_operations(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runtime_resources(
            id TEXT PRIMARY KEY,
            provider_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            version TEXT NOT NULL,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runtime_snapshots(
            id TEXT PRIMARY KEY,
            resource_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runtime_evidence(
            id TEXT PRIMARY KEY,
            operation_id TEXT,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runtime_resource_leases(
            scope TEXT PRIMARY KEY,
            operation_id TEXT NOT NULL UNIQUE,
            acquired_at TEXT NOT NULL
        );
    "#,
    ),
    (
        4,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_preferences(
            key TEXT NOT NULL,
            scope TEXT NOT NULL,
            payload TEXT NOT NULL,
            PRIMARY KEY(key,scope)
        );
        CREATE TABLE IF NOT EXISTS runtime_metrics(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            recorded_at TEXT NOT NULL,
            payload TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS runtime_metrics_by_name_time ON runtime_metrics(name,recorded_at DESC);
        CREATE TABLE IF NOT EXISTS runtime_diagnostics(
            id TEXT PRIMARY KEY,
            provider_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS runtime_hooks(
            id TEXT PRIMARY KEY,
            event TEXT NOT NULL,
            payload TEXT NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1
        );
    "#,
    ),
    (
        5,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_checkpoints(
            run_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(run_id,revision)
        );
        CREATE INDEX IF NOT EXISTS runtime_checkpoints_latest ON runtime_checkpoints(run_id,revision DESC);
    "#,
    ),
    (
        6,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_trust_grants(
            id TEXT PRIMARY KEY,
            payload TEXT NOT NULL,
            expires_at INTEGER,
            revoked_at TEXT
        );
        CREATE TABLE IF NOT EXISTS runtime_workflows(
            id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(id,revision)
        );
        CREATE TABLE IF NOT EXISTS runtime_workflow_runs(
            workflow_id TEXT NOT NULL,
            workflow_revision INTEGER NOT NULL,
            run_id TEXT NOT NULL UNIQUE,
            created_at TEXT NOT NULL
        );
    "#,
    ),
    (
        7,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_notifications(
            id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            read_at TEXT
        );
        CREATE INDEX IF NOT EXISTS runtime_notifications_unread ON runtime_notifications(read_at,created_at DESC);
    "#,
    ),
    (
        8,
        r#"
        CREATE TABLE IF NOT EXISTS runtime_artifact_refs(
            artifact_id TEXT NOT NULL,
            owner_kind TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            purpose TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(artifact_id,owner_kind,owner_id,purpose)
        );
        CREATE INDEX IF NOT EXISTS runtime_artifact_refs_owner ON runtime_artifact_refs(owner_kind,owner_id);
    "#,
    ),
    // Conversation storage used to live in a separate module that had to be
    // opened before the journal, because the journal reads and writes these
    // tables (and declares a foreign key to `messages`) but did not own them.
    // Owning them here removes that ordering requirement. `tasks` and its index
    // are dropped: no code path ever read them back.
    (
        9,
        r#"
        CREATE TABLE IF NOT EXISTS conversations(
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS messages(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            tool_call_id TEXT,
            tool_calls_json TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS messages_by_conversation ON messages(conversation_id, id);
        CREATE TABLE IF NOT EXISTS tool_calls(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
            call_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            arguments_json TEXT NOT NULL,
            result_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        DROP TABLE IF EXISTS tasks;
    "#,
    ),
    // 模型返回的推理载荷必须原样回传，否则 Anthropic、Gemini、Responses 会在
    // 下一轮拒绝请求。它随消息一起持久化，所以单独一列。
    //
    // 不要把这列加进迁移 9 的建表语句：那样新库建表时已带该列，迁移 10 再
    // ALTER 会以 `duplicate column name` 失败。
    (
        10,
        r#"
        ALTER TABLE messages ADD COLUMN reasoning_json TEXT;
    "#,
    ),
    // 快捷任务：定义与不可变修订分表。发布只改定义的指针，修订本体的行一旦写入
    // 就不再更新，这样正在运行的旧版本始终有完整快照可读。
    (
        11,
        r#"
        CREATE TABLE IF NOT EXISTS task_definitions(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL,
            source_conversation_id TEXT,
            published_revision INTEGER,
            draft_revision INTEGER,
            archived INTEGER NOT NULL DEFAULT 0,
            deleted INTEGER NOT NULL DEFAULT 0,
            payload TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS task_definitions_source
            ON task_definitions(source_conversation_id, updated_at);
        CREATE TABLE IF NOT EXISTS task_revisions(
            task_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(task_id, revision)
        );
        CREATE TABLE IF NOT EXISTS task_runs(
            task_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            run_id TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS task_runs_task ON task_runs(task_id, created_at);
    "#,
    ),
    // 会话列表要支持置顶、归档、搜索，以及「每个会话选择模型」。归档是可撤销的
    // 可见性变化，不是删除；删除会话另走确认后的明确动作。
    (
        12,
        r#"
        ALTER TABLE conversations ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE conversations ADD COLUMN archived_at TEXT;
        ALTER TABLE conversations ADD COLUMN model_id TEXT;
    "#,
    ),
    (
        13,
        r#"
        CREATE TABLE IF NOT EXISTS conversation_groups(
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            position INTEGER NOT NULL,
            collapsed INTEGER NOT NULL DEFAULT 0
        );
        ALTER TABLE conversations ADD COLUMN group_id TEXT;
    "#,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_ordered_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = Connection::open(directory.path().join("test.db")).unwrap();
        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();
        let latest: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        let distinct: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(latest, LATEST_SCHEMA_VERSION);
        assert_eq!(distinct, MIGRATIONS.len() as i64);
    }

    /// A database written by the previous release already has the conversation
    /// tables (created by the removed `store` module) and the never-read `tasks`
    /// table. Upgrading must adopt the data and retire the dead table without
    /// touching conversations or messages.
    #[test]
    fn legacy_conversation_storage_is_adopted_and_the_dead_task_table_is_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = Connection::open(directory.path().join("test.db")).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);",
            )
            .unwrap();
        for &(version, sql) in MIGRATIONS.iter().filter(|(version, _)| *version <= 8) {
            connection.execute_batch(sql).unwrap();
            connection
                .execute(
                    "INSERT INTO schema_migrations(version,applied_at) VALUES(?1,'now')",
                    [version],
                )
                .unwrap();
        }
        connection
            .execute_batch(
                "CREATE TABLE conversations(id TEXT PRIMARY KEY,title TEXT NOT NULL,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
                 CREATE TABLE messages(id INTEGER PRIMARY KEY AUTOINCREMENT,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,role TEXT NOT NULL,content TEXT NOT NULL,tool_call_id TEXT,tool_calls_json TEXT,created_at TEXT NOT NULL);
                 CREATE TABLE tasks(id TEXT PRIMARY KEY,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,prompt TEXT NOT NULL,state TEXT NOT NULL,result TEXT,error TEXT,created_at TEXT NOT NULL,updated_at TEXT NOT NULL);
                 CREATE TABLE tool_calls(id INTEGER PRIMARY KEY AUTOINCREMENT,conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,call_id TEXT NOT NULL,tool_name TEXT NOT NULL,arguments_json TEXT NOT NULL,result_json TEXT NOT NULL,created_at TEXT NOT NULL);
                 INSERT INTO conversations VALUES('c','kept','t','t');
                 INSERT INTO messages(conversation_id,role,content,created_at) VALUES('c','user','hello','t');
                 INSERT INTO tasks(id,conversation_id,prompt,state,created_at,updated_at) VALUES('t1','c','stale','queued','t','t');",
            )
            .unwrap();

        migrate(&mut connection).unwrap();
        migrate(&mut connection).unwrap();

        let conversations: i64 = connection
            .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
            .unwrap();
        let messages: i64 = connection
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        let tasks: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tasks'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((conversations, messages, tasks), (1, 1, 0));
        let version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, LATEST_SCHEMA_VERSION);
        // 迁移 10 给 messages 加了推理列。断言它确实落地，而不是被
        // `CREATE TABLE IF NOT EXISTS` 静默跳过。
        let reasoning_column: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name='reasoning_json'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reasoning_column, 1);
    }
}
