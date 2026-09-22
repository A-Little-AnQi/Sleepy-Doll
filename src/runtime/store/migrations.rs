use rusqlite::Connection;

use crate::error::Result;

pub const LATEST_SCHEMA_VERSION: i64 = 15;

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
    // 会话、消息与工具调用表；删除已废弃的 tasks 表。
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
    // 下一轮拒绝请求。它随消息一起持久化，单独一列；该列只在迁移 10 添加。
    (
        10,
        r#"
        ALTER TABLE messages ADD COLUMN reasoning_json TEXT;
    "#,
    ),
    // 快捷任务：定义与不可变修订分表。发布只改定义的指针，修订写入后不再更新。
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
    // 会话置顶、归档与按会话选择模型。
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
    // 对话压缩只保存滚动摘要与一个稳定的逻辑排序边界；原始消息仍完整保留。
    (
        14,
        r#"
        CREATE TABLE IF NOT EXISTS conversation_compactions(
            conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL,
            through_sort_key INTEGER NOT NULL,
            through_message_id INTEGER NOT NULL,
            summary TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
    "#,
    ),
    // Tie persisted assistant text to the exact deltas it replaces, including
    // snapshots read between append_message and assistant.completed.
    (
        15,
        "ALTER TABLE messages ADD COLUMN stream_boundary INTEGER;",
    ),
];
