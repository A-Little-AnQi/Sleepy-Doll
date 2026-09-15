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
        reasoning: None,
    }
}

/// 粗略 token 估算，单位与 `policy.max_tokens` 一致。
///
/// ASCII 每 4 字符 1 token，非 ASCII 每字符 1 token。中文实测约 1～1.5
/// token/字，取 1 是保守下界，宁可早一点修剪上下文。按字节折算会让中文会话
/// 的估算值大出数倍。
pub fn estimate_tokens(text: &str) -> u64 {
    let mut ascii = 0u64;
    let mut wide = 0u64;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            wide += 1;
        }
    }
    ascii.div_ceil(4) + wide
}

/// 估算一组消息的输入 token。
///
/// 逐字段估算而非整体序列化：结构化字段按实际长度算，不把 JSON 的括号引号
/// 当成内容长度。
pub fn estimate_messages_tokens(messages: &[Message]) -> u64 {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &Message) -> u64 {
    // 角色、分隔符与消息外层的固定开销。
    let mut total = estimate_tokens(&message.content) + 8;
    for call in &message.tool_calls {
        total += estimate_tokens(&call.name) + estimate_tokens(&call.arguments.to_string());
    }
    if let Some(reasoning) = &message.reasoning {
        // Anthropic 的 `text` 是 `blocks` 里思考文本的复述，两者都算会重复；
        // 只有签名类载荷（Gemini）才需要另算 `text`。
        total += if reasoning.blocks.is_empty() {
            estimate_tokens(&reasoning.text)
        } else {
            reasoning
                .blocks
                .iter()
                .map(|block| estimate_tokens(&block.to_string()))
                .sum::<u64>()
        };
    }
    total
}

/// 超预算时旧工具结果的正文被换成这条提示。工具调用与结果的配对保持不变，
/// 模型仍看得到自己调过什么、返回过什么形状，只是不再带全文。
const CLEARED_TOOL_RESULT: &str = "[较早的工具结果内容已清除，需要时重新调用]";

/// 发给模型前的上下文。原文仍在 SQLite；这里只是这一轮实际装进窗口的内容。
#[derive(Debug)]
pub struct PackedContext {
    pub messages: Vec<Message>,
    pub tokens: u64,
    pub cleared_results: usize,
    pub dropped_groups: usize,
}

impl PackedContext {
    pub fn compacted(&self) -> bool {
        self.cleared_results > 0 || self.dropped_groups > 0
    }
}

impl std::ops::Deref for PackedContext {
    type Target = [Message];
    fn deref(&self) -> &Self::Target {
        &self.messages
    }
}

/// 无论如何都保留全文的工具结果条数。留一条就够让模型看到最近一次调用的
/// 返回；留多了会变成硬下限 —— 恰好这么多条大结果时一条都清不掉，仍然超限。
const KEEP_RECENT_RESULTS: usize = 1;

/// 结果可以随时重新取得的工具。只有这些的正文可以清 —— 写操作与 Job 的结果
/// 是「动作是否发生过」的证据，不能只留一句占位。
fn reobtainable(name: &str) -> bool {
    matches!(
        name,
        "bgi.user.list" | "bgi.user.read" | "bgi.user.inspect_script" | "bgi.user.resolve"
    ) || matches!(
        name,
        "bgi.state.get"
            | "bgi.api.search"
            | "bgi.api.describe"
            | "bgi.api.read"
            | "artifact.read"
            | "skills.search"
            | "skills.read"
            | "skills.reference"
            | "plugins.list"
            | "tools.search"
            | "resource.search"
    )
}

/// 一组消息占用多少预算。工具参数与推理载荷都算在内 —— 它们同样在回传的
/// 报文里，漏算会让余量判断偏乐观。
fn group_cost(group: &[Message]) -> usize {
    group
        .iter()
        .map(|m| {
            m.content.chars().count()
                + m.tool_calls
                    .iter()
                    .map(|c| c.arguments.to_string().len() + 64)
                    .sum::<usize>()
                + m.reasoning
                    .iter()
                    .flat_map(|r| r.blocks.iter())
                    .map(|b| b.to_string().len() + 16)
                    .sum::<usize>()
                + 32
        })
        .sum()
}

/// 当前历史已占用的预算。执行工具前用它算出这一轮还能放进多少结果 ——
/// 拍一个固定比例会让多轮累积后仍然超限。
pub fn used(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| group_cost(std::slice::from_ref(m)))
        .sum()
}

/// Evict entire tool groups, never an isolated tool result. The original transcript
/// stays in SQLite. This extractive digest cannot introduce facts or permissions.
pub fn build(system: String, history: Vec<Message>, budget: usize) -> Result<PackedContext> {
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
    let cost = |g: &Vec<Message>| group_cost(g);
    let mut total = system.chars().count() + groups.iter().map(cost).sum::<usize>();
    let mut removed = Vec::new();
    let mut cleared_results = 0usize;
    let mut dropped_groups = 0usize;

    // 预算不够时先清旧工具结果的正文，再考虑整组丢弃。占大头的就是这些结果，
    // 而它们的调用与结果配对必须留着 —— 直接报错会让整轮以「超出上下文预算」
    // 收场，用户看到的是一次白跑。
    let mut clearable: Vec<(usize, usize)> = Vec::new();
    for (gi, group) in groups.iter().enumerate() {
        for (mi, m) in group.iter().enumerate() {
            if m.role != Role::Tool
                || m.content.chars().count() <= CLEARED_TOOL_RESULT.chars().count()
            {
                continue;
            }
            let produced_by = group[0]
                .tool_calls
                .iter()
                .find(|c| Some(&c.id) == m.tool_call_id.as_ref())
                .map(|c| c.name.as_str())
                .unwrap_or_default();
            if reobtainable(produced_by) {
                clearable.push((gi, mi));
            }
        }
    }
    // 最旧的先清，最近的 KEEP_RECENT_RESULTS 条留全文。
    let stale = clearable.len().saturating_sub(KEEP_RECENT_RESULTS);
    for (gi, mi) in clearable.drain(..stale) {
        if total + 2048 <= budget {
            break;
        }
        let m = &mut groups[gi][mi];
        let freed = m.content.chars().count() - CLEARED_TOOL_RESULT.chars().count();
        m.content = CLEARED_TOOL_RESULT.into();
        total = total.saturating_sub(freed);
        cleared_results += 1;
    }

    while total + 2048 > budget && groups.len() > 2 {
        let g = groups.remove(0);
        total = total.saturating_sub(cost(&g));
        dropped_groups += 1;
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
    let tokens = estimate_messages_tokens(&result);
    Ok(PackedContext {
        messages: result,
        tokens,
        cleared_results,
        dropped_groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ToolCall;
    use serde_json::json;

    #[test]
    fn refuses_oversized_recent_messages() {
        assert!(
            build(
                "system".into(),
                vec![message(Role::User, "a".repeat(5000))],
                4096
            )
            .is_err()
        );
    }

    #[test]
    fn clears_stale_tool_results_and_reports_compaction() {
        let mut older = message(Role::Assistant, "先查");
        older.tool_calls = vec![ToolCall {
            id: "old".into(),
            name: "bgi.state.get".into(),
            arguments: json!({}),
        }];
        let mut older_result = message(Role::Tool, "旧".repeat(3000));
        older_result.tool_call_id = Some("old".into());
        let mut newer = message(Role::Assistant, "再查");
        newer.tool_calls = vec![ToolCall {
            id: "new".into(),
            name: "bgi.state.get".into(),
            arguments: json!({}),
        }];
        let mut newer_result = message(Role::Tool, "新".repeat(3000));
        newer_result.tool_call_id = Some("new".into());
        let packed = build(
            "system".into(),
            vec![
                older,
                older_result,
                newer,
                newer_result,
                message(Role::User, "继续"),
            ],
            7_000,
        )
        .unwrap();
        assert!(packed.compacted());
        assert_eq!(packed.cleared_results, 1);
        assert!(packed.tokens > 0);
        assert!(packed.iter().any(|m| m.content.contains("已清除")));
    }
}
