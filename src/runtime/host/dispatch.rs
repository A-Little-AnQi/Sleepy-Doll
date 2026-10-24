//! 提供方的批次归一化结果；内核只处理调用配对与交接终态。
use crate::{error::Error, model::ToolCall};

pub(crate) struct BatchPlan {
    pub merged: ToolCall,
    pub member_indexes: Vec<usize>,
    pub result_kind: String,
    pub handoff: Option<Handoff>,
}

pub(crate) struct Handoff {
    pub summary: String,
}

impl Handoff {
    pub fn message(&self, pending: usize) -> String {
        if pending == 0 {
            self.summary.clone()
        } else {
            format!(
                "{}后续 {pending} 项任务尚未执行，需要时再告诉我。",
                self.summary
            )
        }
    }
}

pub(crate) fn skipped_error(guard: bool) -> Error {
    if guard {
        Error::Tool("后台计划已交接并结束本轮，此查询未执行。".into())
    } else {
        Error::Tool("后台计划已交接并结束本轮，此任务未执行；需要时请再发起。".into())
    }
}
