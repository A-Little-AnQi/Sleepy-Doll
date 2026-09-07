use rusqlite::Connection;

use crate::error::Result;

pub const LATEST_SCHEMA_VERSION: i64 = 8;

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
}
