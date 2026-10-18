use super::*;
use crate::runtime::types::Run;

pub(crate) fn summary_value(entry: &MemoryEntry) -> Value {
    // Old records can predate introduction limits. Keep retrieval bounded too.
    json!({"id":entry.id,"revision":entry.revision,"intent":entry.experience.intent.chars().take(160).collect::<String>(),
        "summary":entry.experience.summary.chars().take(240).collect::<String>(),"validationState":entry.validation_state})
}

impl MemoryStore {
    pub(crate) fn invalidate_contexts(&self) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_memory_meta SET edit_epoch=edit_epoch+1 WHERE id=1",
            [],
        )?;
        Ok(())
    }

    pub(crate) fn learning_started(&self) -> Result<()> {
        self.connection.lock().unwrap().execute(
            "UPDATE runtime_memory_meta SET last_learning_at=?1 WHERE id=1",
            [crate::runtime::types::unix_now()],
        )?;
        Ok(())
    }

    pub(super) fn stage_revision(
        &self,
        run: &Run,
        entry: &MemoryEntry,
        experience: &Experience,
        schemas: &BTreeMap<String, String>,
        evidence: &[String],
    ) -> Result<()> {
        experience.validate()?;
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let count: u64 = tx.query_row(
            "SELECT COUNT(*) FROM runtime_memory_drafts WHERE run_id=?1",
            [&run.id],
            |r| r.get(0),
        )?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM runtime_memory_drafts WHERE run_id=?1 AND entry_id=?2)",
            params![run.id, entry.id],
            |r| r.get(0),
        )?;
        if count >= 3 && !exists {
            return Err(Error::Tool(
                "本轮最多合并修订三条记忆，请保留最相关的事实".into(),
            ));
        }
        let epoch: i64 = tx.query_row(
            "SELECT edit_epoch FROM runtime_memory_meta WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let actual: u64 = tx.query_row(
            "SELECT revision FROM runtime_experiences WHERE id=?1",
            [&entry.id],
            |r| r.get(0),
        )?;
        if actual != entry.revision {
            return Err(Error::Stale("记忆已变化，请重新读取后合并修订".into()));
        }
        tx.execute("INSERT INTO runtime_memory_drafts(run_id,entry_id,expected_revision,source_epoch,body,tool_schemas,evidence_refs) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(run_id,entry_id) DO UPDATE SET expected_revision=excluded.expected_revision,source_epoch=excluded.source_epoch,body=excluded.body,tool_schemas=excluded.tool_schemas,evidence_refs=excluded.evidence_refs",
            params![run.id,entry.id,entry.revision,epoch,serde_json::to_string(experience)?,serde_json::to_string(schemas)?,serde_json::to_string(evidence)?])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn flush_revisions(&self, run: &Run, apply: bool) -> Result<Value> {
        let mut db = self.connection.lock().unwrap();
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let drafts = {
            let mut q=tx.prepare("SELECT entry_id,expected_revision,source_epoch,body,tool_schemas,evidence_refs FROM runtime_memory_drafts WHERE run_id=?1")?;
            q.query_map([&run.id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let epoch: i64 = tx.query_row(
            "SELECT edit_epoch FROM runtime_memory_meta WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let (mut updated, mut unchanged, mut conflicts) = (0, 0, 0);
        if apply {
            for (id, revision, source_epoch, body, schemas, evidence) in drafts {
                let current: Option<(u64, String, String)> = tx
                    .query_row(
                        "SELECT revision,body,tool_schemas FROM runtime_experiences WHERE id=?1",
                        [&id],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                let Some((actual, old_body, old_schemas)) = current else {
                    conflicts += 1;
                    continue;
                };
                if actual != revision || source_epoch != epoch {
                    conflicts += 1;
                    continue;
                }
                if old_body == body && old_schemas == schemas {
                    unchanged += 1;
                    continue;
                }
                let experience: Experience = serde_json::from_str(&body)?;
                let schema_map: BTreeMap<String, String> = serde_json::from_str(&schemas)?;
                let key = implementation_key(&experience, &schema_map);
                let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_experiences WHERE implementation_key=?1 AND id<>?2)",params![key,id],|r|r.get(0))?;
                if duplicate {
                    conflicts += 1;
                    continue;
                }
                updated+=tx.execute("UPDATE runtime_experiences SET intent=?2,keywords=?3,body=?4,tool_schemas=?5,implementation_key=?6,evidence_refs=?7,validation_state='candidate',user_edited=0,revision=revision+1,source_run_id=?8,source_conversation_id=?9,source_order=?10,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND revision=?11",
                    params![id,experience.intent,experience.keywords.join(" "),body,schemas,key,evidence,run.id,run.conversation_id,run.message_boundary,revision])?;
                tx.execute(
                    "DELETE FROM runtime_memory_invalid_methods WHERE entry_id=?1",
                    [id],
                )?;
            }
        }
        tx.execute(
            "DELETE FROM runtime_memory_drafts WHERE run_id=?1",
            [&run.id],
        )?;
        if updated > 0 {
            tx.execute(
                "UPDATE runtime_memory_meta SET edit_epoch=edit_epoch+1 WHERE id=1",
                [],
            )?;
        }
        tx.commit()?;
        Ok(json!({"updated":updated,"unchanged":unchanged,"conflicts":conflicts}))
    }
}

impl crate::runtime::Supervisor {
    pub(crate) fn memory_enabled(&self) -> bool {
        self.config.read().unwrap().runtime.memory_enabled
    }

    pub(crate) fn update_memory_enabled(&self, enabled: bool) -> Result<()> {
        if self.memory_enabled() == enabled {
            return Ok(());
        }
        self.memory.invalidate_contexts()?;
        self.config.write().unwrap().runtime.memory_enabled = enabled;
        if !enabled && let Some(control) = self.memory_learning.lock().unwrap().as_ref() {
            control.cancel.cancel();
        }
        Ok(())
    }

    pub(crate) fn project_memory_context(
        &self,
        history: &[crate::model::Message],
    ) -> Result<Vec<crate::model::Message>> {
        if self.memory_enabled() {
            self.memory
                .project_contexts(history, &self.memory_definitions())
        } else {
            Ok(history
                .iter()
                .filter(|m| {
                    !(m.role == crate::model::Role::Context && m.content.starts_with(MEMORY_HEADER))
                })
                .cloned()
                .collect())
        }
    }

    fn memory_write_trigger(
        &self,
        run: &Run,
        arguments: &Value,
        history: &[crate::model::Message],
    ) -> Result<BTreeSet<String>> {
        let trigger = arguments["trigger"].as_str().unwrap_or_default();
        if ![
            "explicitRequest",
            "contradictedMemory",
            "failureRecovery",
            "nonObviousConstraint",
        ]
        .contains(&trigger)
        {
            return Err(Error::Tool(
                "没有有效的记忆触发条件；普通任务完成或成功调用不触发保存".into(),
            ));
        }
        let reason = arguments["reason"].as_str().unwrap_or_default().trim();
        if !(12..=400).contains(&reason.chars().count()) {
            return Err(Error::Tool(
                "说明具体的新事实、可复用价值以及为何需要改变已有做法，不能只写任务已完成".into(),
            ));
        }
        if trigger == "explicitRequest" {
            let quote = arguments["quote"].as_str().unwrap_or_default().trim();
            let goal = self.journal.goal(&run.id)?;
            if quote.is_empty()
                || !goal.is_some_and(|g| g.inputs.iter().any(|input| input.text.contains(quote)))
            {
                return Err(Error::Tool(
                    "用户要求记忆时，必须引用本轮真实用户输入中的原话".into(),
                ));
            }
        }
        let evidence = arguments["evidenceCallIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        if evidence.is_empty() || evidence.len() > 8 {
            return Err(Error::Tool("记忆触发需要本轮一到八条真实工具结果".into()));
        }
        let calls = history
            .iter()
            .flat_map(|m| &m.tool_calls)
            .filter(|c| !c.name.starts_with("memory."))
            .map(|c| c.id.as_str())
            .collect::<BTreeSet<_>>();
        let observed = history
            .iter()
            .filter(|m| m.role == crate::model::Role::Tool)
            .filter_map(|m| {
                let id = m.tool_call_id.as_deref()?;
                let result: Value = serde_json::from_str(&m.content).ok()?;
                (calls.contains(id) && result["ok"].is_boolean())
                    .then_some((id, result["ok"] == true))
            })
            .collect::<BTreeMap<_, _>>();
        if !evidence.iter().all(|id| observed.contains_key(id.as_str())) {
            return Err(Error::Tool(
                "依据必须是本轮真实返回的非记忆工具结果，不能用虚构、其它会话或记忆自身作为证据"
                    .into(),
            ));
        }
        if trigger == "failureRecovery"
            && !(evidence.iter().any(|id| observed[id.as_str()])
                && evidence.iter().any(|id| !observed[id.as_str()]))
        {
            return Err(Error::Tool(
                "故障恢复必须同时引用实际失败和修复后的成功结果，单纯试错或常规成功不触发记忆"
                    .into(),
            ));
        }
        Ok(evidence)
    }

    pub(crate) fn memory_learning_request(&self, run: &Run) -> Result<Option<Value>> {
        let history = self.journal.run_history(run)?;
        let calls = history
            .iter()
            .flat_map(|m| &m.tool_calls)
            .filter(|c| c.name == "memory.learn")
            .map(|c| (c.id.as_str(), &c.arguments))
            .collect::<BTreeMap<_, _>>();
        Ok(history
            .iter()
            .filter(|m| m.role == crate::model::Role::Tool)
            .find_map(|m| {
                let request = calls.get(m.tool_call_id.as_deref()?)?;
                let result: Value = serde_json::from_str(&m.content).ok()?;
                (result["ok"] == true && result["value"]["admitted"] == true)
                    .then(|| (*request).clone())
            }))
    }

    pub(crate) fn memory_learning_requested(&self, run: &Run) -> Result<bool> {
        Ok(self.memory_learning_request(run)?.is_some())
    }

    pub(crate) fn request_memory_learning(&self, run: &Run, arguments: &Value) -> Result<Value> {
        if !self.memory_enabled() || !matches!(run.source, crate::runtime::types::RunSource::Agent)
        {
            return Err(Error::Tool("当前不接受记忆维护".into()));
        }
        if self.memory_learning_requested(run)? {
            return Ok(json!({"admitted":false,"duplicate":true,"silent":true}));
        }
        let history = self.journal.run_history(run)?;
        self.memory_write_trigger(run, arguments, &history)?;
        if arguments["trigger"] == "contradictedMemory" {
            return Err(Error::Tool(
                "旧记忆被证伪时请修订原条目，不创建重复记忆".into(),
            ));
        }
        if source_trace(&history, &run.prompt, &self.memory_definitions()).is_none() {
            return Err(Error::Tool(
                "尚无足够的真实操作关系可提炼，不为单次观察触发记忆".into(),
            ));
        }
        Ok(json!({"admitted":true,"silent":true}))
    }

    pub(crate) fn stage_memory_revision(&self, run: &Run, arguments: &Value) -> Result<Value> {
        if !self.memory_enabled() {
            return Err(Error::Tool("记忆功能已关闭".into()));
        }
        let id = arguments["id"].as_str().unwrap_or_default();
        let entry = self.memory.get(id)?;
        if arguments["revision"].as_u64() != Some(entry.revision) {
            return Err(Error::Stale("记忆已变化，请重新读取".into()));
        }
        if !matches!(run.source, crate::runtime::types::RunSource::Agent) {
            return Err(Error::Tool("固定任务不修订记忆".into()));
        }
        let history = self.journal.run_history(run)?;
        let evidence = self.memory_write_trigger(run, arguments, &history)?;
        let reads = history
            .iter()
            .flat_map(|m| &m.tool_calls)
            .filter(|c| c.name == "memory.read")
            .map(|c| c.id.as_str())
            .collect::<BTreeSet<_>>();
        let read_current = history
            .iter()
            .filter(|m| {
                m.role == crate::model::Role::Tool
                    && m.tool_call_id
                        .as_deref()
                        .is_some_and(|id| reads.contains(id))
            })
            .filter_map(|m| serde_json::from_str::<Value>(&m.content).ok())
            .any(|v| {
                v["ok"] == true
                    && v["value"]["entry"]["id"] == entry.id
                    && v["value"]["entry"]["revision"] == entry.revision
            });
        if !read_current {
            return Err(Error::Tool(
                "修订前先用 memory.read 读取当前完整记忆，合并事实后提交".into(),
            ));
        }
        let experience: Experience = serde_json::from_value(arguments["experience"].clone())?;
        let schemas = tool_schemas(&experience, &self.memory_definitions())?;
        self.memory.stage_revision(
            run,
            &entry,
            &experience,
            &schemas,
            &evidence.into_iter().collect::<Vec<_>>(),
        )?;
        Ok(json!({"staged":true,"id":id,"silent":true}))
    }
}
