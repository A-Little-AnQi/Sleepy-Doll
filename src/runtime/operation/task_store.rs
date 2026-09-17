//! 快捷任务的持久化：定义、不可变修订与运行归属。
//!
//! 「每个聊天有它产生的快捷任务清单」由来源关联查询得出，不另存一份可漂移的
//! 数组；来源会话被删除后，定义仍靠标题快照可读。

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

/// 列表项：定义摘要加计算出的可见状态，不返回修订正文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
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

/// 依赖可用性由调用方判定：Core 不认识具体领域工具。
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

    /// 旧版 `runtime_workflows` 是「已验证运行提取」的另一种存储。升级时把它读成
    /// 新模型，而不是让两套定义长期并存。
    fn adopt_legacy_workflows(&self) -> Result<()> {
        let db = self.connection.lock().unwrap();
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
            let known: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM task_definitions WHERE id=?1)",
                [&legacy.id],
                |row| row.get(0),
            )?;
            if known {
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
                archived_at: None,
                deleted_at: None,
                pinned: false,
                last_run_id: None,
                created_at: legacy.created_at.clone(),
                updated_at: legacy.created_at.clone(),
            };
            let tx = db.unchecked_transaction()?;
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
                definition.archived_at.is_some() as i32,
                definition.deleted_at.is_some() as i32,
                serde_json::to_string(definition)?,
                definition.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn save_definition(&self, definition: &WorkflowDefinition) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE task_definitions SET name=?1,description=?2,source_conversation_id=?3,published_revision=?4,draft_revision=?5,archived=?6,deleted=?7,payload=?8,updated_at=?9 WHERE id=?10",
            params![
                definition.name,
                definition.description,
                definition.source_conversation_id,
                definition.published_revision,
                definition.draft_revision,
                definition.archived_at.is_some() as i32,
                definition.deleted_at.is_some() as i32,
                serde_json::to_string(definition)?,
                definition.updated_at,
                definition.id
            ],
        )?;
        Ok(())
    }

    /// 修订发布后不可变：同号重写直接拒绝，避免旧运行读到自己不认识的版本。
    pub fn save_revision(&self, revision: &WorkflowRevision) -> Result<()> {
        let db = self.connection.lock().unwrap();
        let existing: Option<String> = db
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
        db.execute(
            "INSERT INTO task_revisions VALUES(?1,?2,?3,?4)",
            params![
                revision.task_id,
                revision.revision,
                serde_json::to_string(revision)?,
                revision.created_at
            ],
        )?;
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

    /// 已发布修订。运行只读这里，草稿失败不影响它。
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
        let mut query = db.prepare(
            "SELECT payload FROM task_definitions WHERE deleted=0 ORDER BY updated_at DESC",
        )?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|payload| serde_json::from_str(&payload).map_err(Error::from))
            .collect()
    }

    pub fn record_run(&self, task_id: &str, revision: u64, run_id: &str) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "INSERT INTO task_runs VALUES(?1,?2,?3,?4)",
            params![task_id, revision, run_id, crate::runtime::types::now()],
        )?;
        let mut definition = self.definition(task_id)?;
        definition.last_run_id = Some(run_id.into());
        definition.updated_at = crate::runtime::types::now();
        self.save_definition(&definition)?;
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

    /// 定义当前可见状态。优先级：删除 > 归档 > 草稿 > 依赖不可用 > 验证结论。
    pub fn state_of(
        &self,
        definition: &WorkflowDefinition,
        available: Availability<'_>,
        introduced: &HashSet<String>,
    ) -> (DefinitionState, Option<WorkflowRevision>, Option<String>) {
        if definition.deleted_at.is_some() {
            return (DefinitionState::Deleted, None, None);
        }
        if definition.archived_at.is_some() {
            let revision = definition
                .published_revision
                .and_then(|revision| self.revision(&definition.id, revision).ok());
            return (DefinitionState::Archived, revision, None);
        }
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
            TaskNode::Wait(_) | TaskNode::Result(_) => {}
        }
    }
}

/// 编译期按工具名补齐执行契约，让修订自带发布时的契约快照。
pub fn contract_catalog(
    contracts: &[(String, crate::runtime::operation::task::ToolContract)],
) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    for (name, contract) in contracts {
        catalog.insert(name, contract.clone());
    }
    catalog
}

/// 定义的可编辑字段。改名不触碰执行语义，也不改稳定 ID。
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
