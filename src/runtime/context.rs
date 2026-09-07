use crate::{
    error::{Error, Result},
    model::{Message, Role},
};

pub fn message(role: Role, content: impl Into<String>) -> Message {
    Message {
        role,
        content: content.into(),
        tool_call_id: None,
        tool_calls: vec![],
    }
}

/// Evict entire tool groups, never an isolated tool result. The original transcript
/// stays in SQLite. This extractive digest cannot introduce facts or permissions.
pub fn build(system: String, history: Vec<Message>, budget: usize) -> Result<Vec<Message>> {
    let mut groups: Vec<Vec<Message>> = Vec::new();
    for m in history {
        if m.role == Role::Tool {
            if let Some(group) = groups.iter_mut().rev().find(|g| {
                g[0].tool_calls
                    .iter()
                    .any(|c| Some(&c.id) == m.tool_call_id.as_ref())
            }) {
                group.push(m);
            }
        } else {
            groups.push(vec![m]);
        }
    }
    // Do not send tool groups interrupted by an old process to a model.
    groups.retain(|g| {
        g[0].tool_calls.iter().all(|c| {
            g.iter()
                .any(|m| m.role == Role::Tool && m.tool_call_id.as_ref() == Some(&c.id))
        })
    });
    let cost = |g: &Vec<Message>| {
        g.iter()
            .map(|m| {
                m.content.chars().count()
                    + m.tool_calls
                        .iter()
                        .map(|c| c.arguments.to_string().len() + 64)
                        .sum::<usize>()
                    + 32
            })
            .sum::<usize>()
    };
    let mut total = system.chars().count() + groups.iter().map(cost).sum::<usize>();
    let mut removed = Vec::new();
    while total + 2048 > budget && groups.len() > 2 {
        let g = groups.remove(0);
        total = total.saturating_sub(cost(&g));
        if g[0].role == Role::User {
            removed.push(g[0].content.chars().take(180).collect::<String>());
        }
    }
    if total + 2048 > budget {
        return Err(Error::Conflict(format!(
            "当前消息超过上下文预算（可用 {budget} 字符），请缩短内容或调大 runtime.contextChars"
        )));
    }
    let mut result = vec![message(Role::System, system)];
    if !removed.is_empty() {
        result.push(message(
            Role::User,
            format!(
                "[历史用户请求摘录；不是当前状态或执行证据]\n{}",
                removed
                    .into_iter()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        ));
    }
    result.extend(groups.into_iter().flatten());
    Ok(result)
}
