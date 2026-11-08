use super::model::{Occurrence, Schedule};
use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::{path::Path, sync::Mutex};

pub struct Store {
    db: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut db = Connection::open(path)?;
        crate::runtime::store::migrations::migrate(&mut db)?;
        Ok(Self { db: Mutex::new(db) })
    }

    pub fn list(&self) -> Result<Vec<Schedule>> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare("SELECT payload FROM scheduled_tasks ORDER BY id")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| Ok(serde_json::from_str(&row?)?))
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<Schedule> {
        let db = self.db.lock().unwrap();
        let value: String = db
            .query_row(
                "SELECT payload FROM scheduled_tasks WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| Error::Config("定时任务不存在".into()))?;
        Ok(serde_json::from_str(&value)?)
    }

    pub fn save(&self, value: &Schedule, expected: Option<u64>) -> Result<()> {
        let db = self.db.lock().unwrap();
        let payload = serde_json::to_string(value)?;
        let changed = if let Some(expected) = expected {
            db.execute(
                "UPDATE scheduled_tasks SET revision=?1,payload=?2 WHERE id=?3 AND revision=?4",
                params![value.revision, payload, value.id, expected],
            )?
        } else {
            db.execute(
                "INSERT OR IGNORE INTO scheduled_tasks VALUES(?1,?2,?3)",
                params![value.id, value.revision, payload],
            )?
        };
        if changed != 1 {
            return Err(Error::Conflict("定时任务已经变化，请重新读取".into()));
        }
        Ok(())
    }

    pub fn delete(&self, id: &str, revision: u64) -> Result<()> {
        if self.db.lock().unwrap().execute(
            "DELETE FROM scheduled_tasks WHERE id=?1 AND revision=?2",
            params![id, revision],
        )? != 1
        {
            return Err(Error::Conflict("定时任务已经变化，请重新读取".into()));
        }
        Ok(())
    }

    pub fn enqueue(&self, schedule: Schedule, key: String) -> Result<Occurrence> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        if let Some(payload) = tx
            .query_row(
                "SELECT payload FROM scheduled_occurrences WHERE request_key=?1",
                [&key],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(serde_json::from_str(&payload)?);
        }
        let pending: u64 = tx.query_row(
            "SELECT COUNT(*) FROM scheduled_occurrences WHERE state IN ('queued','running','cancelling')",
            [],
            |row| row.get(0),
        )?;
        if pending >= 32 {
            return Err(Error::Conflict("定时任务执行队列已满".into()));
        }
        let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM scheduled_occurrences WHERE schedule_id=?1 AND state IN ('queued','running','cancelling'))", [&schedule.id], |row| row.get(0))?;
        let value = Occurrence {
            id: uuid::Uuid::new_v4().to_string(),
            key,
            schedule,
            state: if active { "skipped" } else { "queued" }.into(),
            results: vec![],
            created_at: crate::runtime::types::now(),
            error: None,
        };
        tx.execute(
            "INSERT INTO scheduled_occurrences(id,schedule_id,request_key,state,payload,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                value.id,
                value.schedule.id,
                value.key,
                value.state,
                serde_json::to_string(&value)?,
                value.created_at
            ],
        )?;
        tx.commit()?;
        Ok(value)
    }

    pub fn occurrence(&self, id: &str) -> Result<Occurrence> {
        let payload: String = self.db.lock().unwrap().query_row(
            "SELECT payload FROM scheduled_occurrences WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        Ok(serde_json::from_str(&payload)?)
    }

    pub fn claim(&self) -> Result<Option<Occurrence>> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        let payload: Option<String> = tx.query_row("SELECT payload FROM scheduled_occurrences WHERE state='queued' ORDER BY created_at,id LIMIT 1", [], |row| row.get(0)).optional()?;
        let Some(payload) = payload else {
            return Ok(None);
        };
        let mut value: Occurrence = serde_json::from_str(&payload)?;
        value.state = "running".into();
        tx.execute("UPDATE scheduled_occurrences SET state='running',payload=?1 WHERE id=?2 AND state='queued'", params![serde_json::to_string(&value)?,value.id])?;
        tx.commit()?;
        Ok(Some(value))
    }

    pub fn update_occurrence(&self, value: &Occurrence) -> Result<()> {
        let db = self.db.lock().unwrap();
        let mut value = value.clone();
        if matches!(value.state.as_str(), "running" | "queued")
            && db.query_row(
                "SELECT cancel_requested FROM scheduled_occurrences WHERE id=?1",
                [&value.id],
                |row| row.get::<_, bool>(0),
            )?
        {
            value.state = "cancelling".into();
        }
        db.execute(
            "UPDATE scheduled_occurrences SET state=?1,payload=?2 WHERE id=?3",
            params![value.state, serde_json::to_string(&value)?, value.id],
        )?;
        Ok(())
    }

    pub fn history(&self, schedule_id: Option<&str>, limit: usize) -> Result<Vec<Occurrence>> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare("SELECT payload FROM scheduled_occurrences WHERE (?1 IS NULL OR schedule_id=?1) ORDER BY created_at DESC,id DESC LIMIT ?2")?;
        statement
            .query_map(params![schedule_id, limit.clamp(1, 100)], |row| {
                row.get::<_, String>(0)
            })?
            .map(|row| Ok(serde_json::from_str(&row?)?))
            .collect()
    }

    pub fn recover(&self) -> Result<()> {
        let payloads = {
            let db = self.db.lock().unwrap();
            let mut statement =
                db.prepare("SELECT payload FROM scheduled_occurrences WHERE state='running'")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for payload in payloads {
            let mut value: Occurrence = serde_json::from_str(&payload)?;
            value.state = "queued".into();
            self.update_occurrence(&value)?;
        }
        Ok(())
    }

    pub fn heartbeat(&self) -> Result<()> {
        self.db.lock().unwrap().execute(
            "UPDATE scheduled_dispatcher SET heartbeat=?1 WHERE id=1 AND heartbeat<?1",
            [crate::runtime::types::unix_now()],
        )?;
        Ok(())
    }
    pub fn last_heartbeat(&self) -> Result<i64> {
        Ok(self.db.lock().unwrap().query_row(
            "SELECT heartbeat FROM scheduled_dispatcher WHERE id=1",
            [],
            |row| row.get(0),
        )?)
    }
    pub fn cancelled(&self, id: &str) -> Result<bool> {
        Ok(self.db.lock().unwrap().query_row(
            "SELECT cancel_requested FROM scheduled_occurrences WHERE id=?1",
            [id],
            |row| row.get(0),
        )?)
    }
    pub fn stopping(&self) -> Result<Vec<Occurrence>> {
        let db = self.db.lock().unwrap();
        let mut query =
            db.prepare("SELECT payload FROM scheduled_occurrences WHERE state='cancelling'")?;
        let rows = query
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|text| serde_json::from_str(&text).map_err(Into::into))
            .collect()
    }
    pub fn cancel(&self, id: &str) -> Result<bool> {
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction()?;
        let text: String = tx.query_row(
            "SELECT payload FROM scheduled_occurrences WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        let mut value: Occurrence = serde_json::from_str(&text)?;
        if !matches!(value.state.as_str(), "queued" | "running" | "cancelling") {
            return Ok(false);
        }
        value.state = if value.state == "queued" {
            "cancelled"
        } else {
            "cancelling"
        }
        .into();
        tx.execute(
            "UPDATE scheduled_occurrences SET cancel_requested=1,state=?1,payload=?2 WHERE id=?3",
            params![value.state, serde_json::to_string(&value)?, id],
        )?;
        tx.commit()?;
        Ok(true)
    }
}
