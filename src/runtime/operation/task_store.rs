//! 快捷任务的持久化：定义、不可变修订与运行归属。

use std::{collections::HashSet, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

use crate::{
    error::{Error, Result},
    runtime::operation::task::{
        DefinitionState, ModelUsage, TaskNode, ToolCatalog, WorkflowDefinition, WorkflowRevision,
    },
};

/// 列表项：定义摘要加计算出的可见状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub shortcut: Option<super::shortcuts::ShortcutBinding>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub state: DefinitionState,
    pub state_label: String,
    pub action_label: String,
    pub runnable: bool,
    pub pinned: bool,
    pub source_conversation_id: Option<String>,
    pub source_title_snapshot: String,
    pub source_deleted: bool,
    pub published_revision: Option<u64>,
    pub revision: Option<u64>,
    pub model_usage: ModelUsage,
    pub zero_token: bool,
    pub node_count: usize,
    pub updated_at: String,
    pub last_run_id: Option<String>,
    pub issue: Option<String>,
}

/// 依赖可用性由调用方判定。
pub type Availability<'a> = &'a dyn Fn(&str) -> bool;

pub struct TaskStore {
    connection: Mutex<Connection>,
}

impl TaskStore {
    pub fn open(path: &Path) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        crate::runtime::store::migrations::migrate(&mut connection)?;
        let store = Self {
            connection: Mutex::new(connection),
        };
        store.adopt_legacy_workflows()?;
        Ok(store)
    }

    /// 把旧版 `runtime_workflows` 的行读成新模型。
    fn adopt_legacy_workflows(&self) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='runtime_workflows')",
            [],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(());
        }
        let mut query = db.prepare(
            "SELECT w.payload FROM runtime_workflows w
             JOIN (SELECT id,MAX(revision) revision FROM runtime_workflows GROUP BY id) latest
               ON latest.id=w.id AND latest.revision=w.revision",
        )?;
        let payloads = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(query);
        for payload in payloads {
            let Ok(legacy) =
                serde_json::from_str::<crate::runtime::operation::workflow::Workflow>(&payload)
            else {
                continue;
            };
            let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let still_present: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM runtime_workflows WHERE id=?1 AND revision=?2)",
                params![legacy.id, legacy.revision],
                |row| row.get(0),
            )?;
            let known: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM task_definitions WHERE id=?1)",
                [&legacy.id],
                |row| row.get(0),
            )?;
            if known || !still_present {
                continue;
            }
            let Some(revision) = legacy.to_revision() else {
                continue;
            };
            let definition = WorkflowDefinition {
                id: legacy.id.clone(),
                name: legacy.name.clone(),
                description: legacy.description.clone(),
                source_conversation_id: None,
                source_message_id: None,
                source_title_snapshot: format!("由运行 {} 提取", legacy.verified_from_run),
                published_revision: Some(revision.revision),
                draft_revision: None,
                pinned: false,
                last_run_id: None,
                created_at: legacy.created_at.clone(),
                updated_at: legacy.created_at.clone(),
            };
            tx.execute(
                "INSERT OR IGNORE INTO task_definitions VALUES(?1,?2,?3,NULL,?4,NULL,0,0,?5,?6)",
                params![
                    definition.id,
                    definition.name,
                    definition.description,
                    definition.published_revision,
                    serde_json::to_string(&definition)?,
                    definition.updated_at
                ],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO task_revisions VALUES(?1,?2,?3,?4)",
                params![
                    revision.task_id,
                    revision.revision,
                    serde_json::to_string(&revision)?,
                    revision.created_at
                ],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn create_definition(&self, definition: &WorkflowDefinition) -> Result<()> {
        if definition.id.trim().is_empty() || definition.name.trim().is_empty() {
            return Err(Error::Config("快捷任务需要名称".into()));
        }
        let db = self.connection.lock().unwrap();
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_definitions WHERE id=?1)",
            [&definition.id],
            |row| row.get(0),
        )?;
        if exists {
            return Err(Error::Conflict("快捷任务已存在".into()));
        }
        db.execute(
            "INSERT INTO task_definitions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                definition.id,
                definition.name,
                definition.description,
                definition.source_conversation_id,
                definition.published_revision,
                definition.draft_revision,
                0,
                0,
                serde_json::to_string(definition)?,
                definition.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn save_shortcut_atomically(
        &self,
        definition: &WorkflowDefinition,
        revision: &WorkflowRevision,
        require_existing: bool,
    ) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_definitions WHERE id=?1)",
            [&definition.id],
            |row| row.get(0),
        )?;
        if require_existing && !exists {
            return Err(Error::Conflict("快捷任务已删除，无法保存旧版本".into()));
        }
        if !require_existing && exists {
            return Err(Error::Conflict("快捷任务已存在".into()));
        }
        tx.execute(
            "INSERT INTO task_revisions(task_id,revision,payload,created_at) VALUES(?1,?2,?3,?4)",
            params![
                revision.task_id,
                revision.revision,
                serde_json::to_string(revision)?,
                revision.created_at
            ],
        )?;
        tx.execute("INSERT INTO task_definitions VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
            ON CONFLICT(id) DO UPDATE SET name=excluded.name,description=excluded.description,source_conversation_id=excluded.source_conversation_id,published_revision=excluded.published_revision,draft_revision=excluded.draft_revision,archived=0,deleted=0,payload=excluded.payload,updated_at=excluded.updated_at",params![definition.id,definition.name,definition.description,definition.source_conversation_id,definition.published_revision,definition.draft_revision,0,0,serde_json::to_string(definition)?,definition.updated_at])?;
        tx.commit()?;
        Ok(())
    }

    pub fn save_definition(&self, definition: &WorkflowDefinition) -> Result<()> {
        let changed=self.connection.lock().unwrap().execute(
            "UPDATE task_definitions SET name=?1,description=?2,source_conversation_id=?3,published_revision=?4,draft_revision=?5,archived=?6,deleted=?7,payload=?8,updated_at=?9 WHERE id=?10",
            params![
                definition.name,
                definition.description,
                definition.source_conversation_id,
                definition.published_revision,
                definition.draft_revision,
                0,
                0,
                serde_json::to_string(definition)?,
                definition.updated_at,
                definition.id
            ],
        )?;
        if changed == 0 {
            return Err(Error::Config("快捷任务不存在".into()));
        }
        Ok(())
    }

    /// 永久删除定义、全部修订和运行关联，旧版导入源也一并移除。
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        for sql in [
            "DELETE FROM task_runs WHERE task_id=?1",
            "DELETE FROM task_revisions WHERE task_id=?1",
            "DELETE FROM runtime_workflow_runs WHERE workflow_id=?1",
            "DELETE FROM runtime_workflows WHERE id=?1",
            "DELETE FROM task_definitions WHERE id=?1",
        ] {
            tx.execute(sql, [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 同一修订号只接受内容相同的重写。
    pub fn save_revision(&self, revision: &WorkflowRevision) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_definitions WHERE id=?1)",
            [&revision.task_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(Error::Config("快捷任务不存在".into()));
        }
        let existing: Option<String> = tx
            .query_row(
                "SELECT payload FROM task_revisions WHERE task_id=?1 AND revision=?2",
                params![revision.task_id, revision.revision],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(payload) = existing {
            let stored: WorkflowRevision = serde_json::from_str(&payload)?;
            return if stored == *revision {
                Ok(())
            } else {
                Err(Error::Conflict("已发布的修订不可改写".into()))
            };
        }
        tx.execute(
            "INSERT INTO task_revisions VALUES(?1,?2,?3,?4)",
            params![
                revision.task_id,
                revision.revision,
                serde_json::to_string(revision)?,
                revision.created_at
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn definition(&self, id: &str) -> Result<WorkflowDefinition> {
        let payload: String = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM task_definitions WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Config("快捷任务不存在".into()))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn revision(&self, task_id: &str, revision: u64) -> Result<WorkflowRevision> {
        let payload: String = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM task_revisions WHERE task_id=?1 AND revision=?2",
                params![task_id, revision],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Config("该修订不存在".into()))?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn latest_revision(&self, task_id: &str) -> Result<Option<WorkflowRevision>> {
        let payload: Option<String> = self
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload FROM task_revisions WHERE task_id=?1 ORDER BY revision DESC LIMIT 1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?;
        payload
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .transpose()
    }

    /// 已发布修订：运行只读这里。
    pub fn published_revision(&self, task_id: &str) -> Result<Option<WorkflowRevision>> {
        let definition = self.definition(task_id)?;
        match definition.published_revision {
            Some(revision) => self.revision(task_id, revision).map(Some),
            None => Ok(None),
        }
    }

    pub fn next_revision_number(&self, task_id: &str) -> Result<u64> {
        Ok(self
            .latest_revision(task_id)?
            .map(|revision| revision.revision + 1)
            .unwrap_or(1))
    }

    pub fn definitions(&self) -> Result<Vec<WorkflowDefinition>> {
        let db = self.connection.lock().unwrap();
        let mut query =
            db.prepare("SELECT payload FROM task_definitions ORDER BY updated_at DESC")?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .collect()
    }

    pub fn record_run(&self, task_id: &str, revision: u64, run_id: &str) -> Result<()> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let payload: String = tx
            .query_row(
                "SELECT payload FROM task_definitions WHERE id=?1",
                [task_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Config("快捷任务不存在".into()))?;
        let mut definition: WorkflowDefinition = serde_json::from_str(&payload)?;
        tx.execute(
            "INSERT INTO task_runs VALUES(?1,?2,?3,?4)",
            params![task_id, revision, run_id, crate::runtime::types::now()],
        )?;
        definition.last_run_id = Some(run_id.into());
        definition.updated_at = crate::runtime::types::now();
        tx.execute(
            "UPDATE task_definitions SET payload=?1,updated_at=?2 WHERE id=?3",
            params![
                serde_json::to_string(&definition)?,
                definition.updated_at,
                task_id
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn run_ids(&self, task_id: &str, limit: usize) -> Result<Vec<String>> {
        let db = self.connection.lock().unwrap();
        let mut query = db.prepare(
            "SELECT run_id FROM task_runs WHERE task_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        Ok(query
            .query_map(params![task_id, limit as i64], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 定义当前可见状态：草稿、依赖可用性和验证结论。
    pub fn state_of(
        &self,
        definition: &WorkflowDefinition,
        available: Availability<'_>,
        introduced: &HashSet<String>,
    ) -> (DefinitionState, Option<WorkflowRevision>, Option<String>) {
        if definition.published_revision.is_none() {
            let revision = definition
                .draft_revision
                .and_then(|revision| self.revision(&definition.id, revision).ok());
            return match revision {
                Some(revision) => {
                    let issue = revision
                        .validation
                        .issues
                        .first()
                        .map(|issue| issue.message.clone());
                    (DefinitionState::Draft, Some(revision), issue)
                }
                None => (
                    DefinitionState::Draft,
                    None,
                    Some("还需要确定运行所需的输入".into()),
                ),
            };
        }
        let revision = definition
            .published_revision
            .and_then(|revision| self.revision(&definition.id, revision).ok());
        let Some(revision) = revision else {
            return (
                DefinitionState::Invalid,
                None,
                Some("已发布的修订无法读取".into()),
            );
        };
        let missing = missing_dependencies(&revision, available);
        if !missing.is_empty() {
            return (
                DefinitionState::Unavailable,
                Some(revision),
                Some(crate::extension::providers::missing_plugin_issue(
                    &missing, introduced,
                )),
            );
        }
        (DefinitionState::ReadyUnverified, Some(revision), None)
    }

    pub fn summary(
        &self,
        definition: &WorkflowDefinition,
        available: Availability<'_>,
        introduced: &HashSet<String>,
        known_conversations: &HashSet<String>,
    ) -> TaskSummary {
        let (state, revision, issue) = self.state_of(definition, available, introduced);
        let model_usage = revision
            .as_ref()
            .map(|revision| revision.model_usage)
            .unwrap_or(ModelUsage::Unknown);
        TaskSummary {
            shortcut: revision
                .as_ref()
                .and_then(|revision| revision.shortcut.clone()),
            id: definition.id.clone(),
            name: definition.name.clone(),
            description: definition.description.clone(),
            state,
            state_label: state.label().into(),
            action_label: state.action().into(),
            runnable: state.runnable(),
            pinned: definition.pinned,
            source_conversation_id: definition.source_conversation_id.clone(),
            source_title_snapshot: definition.source_title_snapshot.clone(),
            source_deleted: definition
                .source_conversation_id
                .as_ref()
                .is_some_and(|id| !known_conversations.contains(id)),
            published_revision: definition.published_revision,
            revision: revision.as_ref().map(|revision| revision.revision),
            model_usage,
            zero_token: model_usage.is_deterministic(),
            node_count: revision
                .as_ref()
                .map(|r| r.validation.node_count)
                .unwrap_or(0),
            updated_at: definition.updated_at.clone(),
            last_run_id: definition.last_run_id.clone(),
            issue,
        }
    }
}

fn missing_dependencies(revision: &WorkflowRevision, available: Availability<'_>) -> Vec<String> {
    let mut missing = Vec::new();
    for name in tool_names(&revision.nodes) {
        if !available(&name) && !missing.contains(&name) {
            missing.push(name);
        }
    }
    missing
}

pub fn tool_names(nodes: &[TaskNode]) -> Vec<String> {
    let mut names = Vec::new();
    collect_tool_names(nodes, &mut names);
    names
}

fn collect_tool_names(nodes: &[TaskNode], names: &mut Vec<String>) {
    for node in nodes {
        match node {
            TaskNode::Tool(tool) => {
                if let Some(name) = &tool.tool {
                    names.push(name.clone());
                }
            }
            TaskNode::Sequence(node) => collect_tool_names(&node.nodes, names),
            TaskNode::Condition(node) => {
                collect_tool_names(&node.then, names);
                collect_tool_names(&node.otherwise, names);
                collect_tool_names(&node.unknown, names);
            }
            TaskNode::ForEach(node) => collect_tool_names(&node.nodes, names),
            TaskNode::Repeat(node) => collect_tool_names(&node.nodes, names),
            TaskNode::Wait(wait) => {
                if let Some(name) = wait.probe.as_ref().and_then(|probe| probe.tool.as_ref()) {
                    names.push(name.clone());
                }
            }
            TaskNode::Result(_) => {}
        }
    }
}

/// 按工具名补齐执行契约。
pub fn contract_catalog(
    contracts: &[(String, crate::runtime::operation::task::ToolContract)],
) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    for (name, contract) in contracts {
        catalog.insert(name, contract.clone());
    }
    catalog
}

/// 定义的可编辑字段。
#[derive(Debug, Clone, Default)]
pub struct DefinitionPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub pinned: Option<bool>,
}

impl WorkflowDefinition {
    pub fn apply(&mut self, patch: DefinitionPatch) -> Result<()> {
        if let Some(name) = patch.name {
            let name = name.trim().to_owned();
            if name.is_empty() || name.chars().count() > 120 {
                return Err(Error::Config("任务名称应为 1 到 120 个字符".into()));
            }
            self.name = name;
        }
        if let Some(description) = patch.description {
            self.description = description.trim().chars().take(400).collect();
        }
        if let Some(pinned) = patch.pinned {
            self.pinned = pinned;
        }
        self.updated_at = crate::runtime::types::now();
        Ok(())
    }
}

pub fn empty_arguments() -> Value {
    Value::Object(serde_json::Map::new())
}

#[cfg(test)]
mod deletion_tests {
    use super::*;
    use crate::runtime::operation::workflow::{Workflow, WorkflowStep};
    use serde_json::json;

    fn definition(id: &str) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: id.into(),
            description: "用户保存的任务".into(),
            source_conversation_id: None,
            source_message_id: None,
            source_title_snapshot: String::new(),
            published_revision: Some(1),
            draft_revision: None,
            pinned: false,
            last_run_id: None,
            created_at: "2026-11-07T00:00:00Z".into(),
            updated_at: "2026-11-07T00:00:00Z".into(),
        }
    }

    fn revision(id: &str, number: u64) -> WorkflowRevision {
        WorkflowRevision {
            shortcut: None,
            task_id: id.into(),
            revision: number,
            schema_version: 1,
            name: id.into(),
            description: String::new(),
            nodes: vec![],
            limits: Default::default(),
            model_usage: ModelUsage::None,
            validation: Default::default(),
            created_at: "2026-11-07T00:00:00Z".into(),
        }
    }

    fn legacy(id: &str) -> Workflow {
        Workflow {
            id: id.into(),
            revision: 1,
            name: id.into(),
            description: String::new(),
            input_schema: json!({}),
            steps: vec![WorkflowStep {
                id: "read".into(),
                title: "读取".into(),
                tool: "fixture.read".into(),
                arguments: json!({}),
                depends_on: vec![],
                execution: crate::extension::ToolExecution::read_only(),
                provider_version: None,
                resource_versions: vec![],
            }],
            verified_from_run: "old-run".into(),
            verified_at: "2026-11-07T00:00:00Z".into(),
            unattended: Default::default(),
            created_at: "2026-11-07T00:00:00Z".into(),
        }
    }

    fn database(name: &str) -> std::path::PathBuf {
        let root = crate::test_root(name);
        std::fs::create_dir_all(&root).unwrap();
        root.join("tasks.db")
    }

    fn seed_legacy(store: &TaskStore, id: &str) {
        let workflow = legacy(id);
        let db = store.connection.lock().unwrap();
        db.execute(
            "INSERT INTO runtime_workflows VALUES(?1,1,?2,?3)",
            params![
                id,
                serde_json::to_string(&workflow).unwrap(),
                workflow.created_at
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO runtime_workflow_runs VALUES(?1,1,?2,'now')",
            params![id, format!("legacy-{id}")],
        )
        .unwrap();
    }

    fn counts(store: &TaskStore, id: &str) -> Vec<i64> {
        let db = store.connection.lock().unwrap();
        [
            ("task_definitions", "id"),
            ("task_revisions", "task_id"),
            ("task_runs", "task_id"),
            ("runtime_workflows", "id"),
            ("runtime_workflow_runs", "workflow_id"),
        ]
        .into_iter()
        .map(|(table, key)| {
            db.query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE {key}=?1"),
                [id],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect()
    }

    #[test]
    fn hard_delete_purges_legacy_and_all_revisions_without_resurrection() {
        let path = database("task-hard-delete");
        let store = TaskStore::open(&path).unwrap();
        seed_legacy(&store, "target");
        drop(store);
        let store = TaskStore::open(&path).unwrap();
        assert!(
            store.definition("target").is_ok(),
            "先确认真实旧版记录已被导入"
        );
        store.save_revision(&revision("target", 2)).unwrap();
        store.record_run("target", 1, "run-1").unwrap();
        store.create_definition(&definition("other")).unwrap();
        store.save_revision(&revision("other", 1)).unwrap();
        store.delete("target").unwrap();
        assert_eq!(counts(&store, "target"), vec![0; 5]);
        assert!(store.definition("other").is_ok());
        assert!(store.revision("other", 1).is_ok());
        drop(store);
        let reopened = TaskStore::open(&path).unwrap();
        assert!(reopened.definition("target").is_err());
        assert!(reopened.revision("target", 1).is_err());
        assert_eq!(counts(&reopened, "target"), vec![0; 5]);
        assert_eq!(reopened.definitions().unwrap().len(), 1);
        reopened.delete("target").unwrap();
        assert!(reopened.definition("other").is_ok());
    }

    #[test]
    fn hard_delete_rolls_back_every_table_on_failure() {
        let path = database("task-delete-rollback");
        let store = TaskStore::open(&path).unwrap();
        store.create_definition(&definition("target")).unwrap();
        store.save_revision(&revision("target", 1)).unwrap();
        store.record_run("target", 1, "run-1").unwrap();
        seed_legacy(&store, "target");
        let before = counts(&store, "target");
        store.connection.lock().unwrap().execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON task_definitions BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").unwrap();
        assert!(store.delete("target").is_err());
        assert_eq!(counts(&store, "target"), before);
        drop(store);
        let reopened = TaskStore::open(&path).unwrap();
        assert_eq!(counts(&reopened, "target"), before);
    }

    #[test]
    fn hard_delete_rejects_late_updates_and_run_links() {
        let path = database("task-delete-late-write");
        let store = TaskStore::open(&path).unwrap();
        let stale = definition("target");
        store.create_definition(&stale).unwrap();
        store.save_revision(&revision("target", 1)).unwrap();
        store.delete("target").unwrap();
        assert!(store.save_definition(&stale).is_err());
        assert!(store.save_revision(&revision("target", 2)).is_err());
        assert!(
            store
                .save_shortcut_atomically(&stale, &revision("target", 2), true)
                .is_err()
        );
        assert!(store.record_run("target", 1, "late-run").is_err());
        assert_eq!(counts(&store, "target"), vec![0; 5]);
    }

    #[test]
    fn migration_purges_old_deletions_and_returns_archived_tasks_to_normal_list() {
        let path = database("task-delete-migration");
        let store = TaskStore::open(&path).unwrap();
        for id in ["deleted", "archived"] {
            store.create_definition(&definition(id)).unwrap();
            store.save_revision(&revision(id, 1)).unwrap();
            store.record_run(id, 1, &format!("run-{id}")).unwrap();
            let mut payload = serde_json::to_value(definition(id)).unwrap();
            payload["archivedAt"] = json!("old");
            payload["deletedAt"] = if id == "deleted" {
                json!("old")
            } else {
                Value::Null
            };
            store
                .connection
                .lock()
                .unwrap()
                .execute(
                    "UPDATE task_definitions SET archived=1,deleted=?1,payload=?2 WHERE id=?3",
                    params![id == "deleted", payload.to_string(), id],
                )
                .unwrap();
        }
        seed_legacy(&store, "deleted");
        store
            .connection
            .lock()
            .unwrap()
            .execute_batch("DELETE FROM schema_migrations WHERE version=22;PRAGMA user_version=21;")
            .unwrap();
        drop(store);
        let migrated = TaskStore::open(&path).unwrap();
        assert_eq!(counts(&migrated, "deleted"), vec![0; 5]);
        let tasks = migrated.definitions().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "archived");
        let payload:String=migrated.connection.lock().unwrap().query_row("SELECT payload FROM task_definitions WHERE id='archived' AND archived=0 AND deleted=0",[],|row|row.get(0)).unwrap();
        let payload: Value = serde_json::from_str(&payload).unwrap();
        assert!(payload.get("archivedAt").is_none());
        assert!(payload.get("deletedAt").is_none());
        drop(migrated);
        let reopened = TaskStore::open(&path).unwrap();
        assert_eq!(reopened.definitions().unwrap().len(), 1);
        assert_eq!(counts(&reopened, "deleted"), vec![0; 5]);
    }
}
