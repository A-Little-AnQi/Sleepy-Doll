//! Ranked lexical discovery; it opens contracts, never grants authorization.
use crate::extension::ToolDefinition;
use std::collections::HashSet;

fn terms(text: &str) -> HashSet<String> {
    let text = text.to_lowercase();
    let mut result = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| part.len() > 1)
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let chars = text.chars().collect::<Vec<_>>();
    for pair in chars.windows(2) {
        if pair.iter().all(|c| ('\u{3400}'..='\u{9fff}').contains(c)) {
            result.insert(pair.iter().collect());
        }
    }
    result
}

pub(super) fn search(tools: Vec<ToolDefinition>, query: &str, limit: usize) -> Vec<ToolDefinition> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return vec![];
    }
    let tokens = terms(&query);
    let mut ranked = tools
        .into_iter()
        .filter_map(|tool| {
            let name = tool.name.to_lowercase();
            let label = tool.label.to_lowercase();
            let description = tool.description.to_lowercase();
            let hint = tool
                .execution
                .search_hint
                .as_deref()
                .unwrap_or_default()
                .to_lowercase();
            let named = query.split_whitespace().any(|part| {
                part.trim_matches(|c: char| !c.is_alphanumeric() && !matches!(c, '.' | '_' | '-'))
                    == name
            });
            let exact = if name == query || named {
                10_000
            } else if name.contains(&query) || label.contains(&query) {
                2_000
            } else {
                0
            };
            let score = exact
                + tokens
                    .iter()
                    .map(|token| {
                        usize::from(name.contains(token)) * 120
                            + usize::from(label.contains(token)) * 80
                            + usize::from(hint.contains(token)) * 40
                            + usize::from(description.contains(token)) * 8
                    })
                    .sum::<usize>();
            // 相关性优先；外部参考只在同分时后排，不能压过明确的工具名匹配。
            (score > 0).then_some((score, tool))
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|(a, left), (b, right)| {
        b.cmp(a)
            .then_with(|| (left.source == "core:web").cmp(&(right.source == "core:web")))
            .then_with(|| left.name.cmp(&right.name))
    });
    ranked
        .into_iter()
        .take(limit)
        .map(|(_, tool)| tool)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::{FunctionTool, Tool};
    use serde_json::json;
    fn tool(name: &str, label: &str, text: &str) -> ToolDefinition {
        FunctionTool::new(name, text, json!({}), "core", |_| Ok(json!({})))
            .with_label(label)
            .definition()
    }
    #[test]
    fn mixed_keywords_and_chinese_sentences_find_real_contracts() {
        let tools = vec![
            tool("shortcut.save", "添加快捷任务", "保存入口"),
            tool("shortcut.list", "查找快捷任务", "列出已保存的入口"),
            tool("workspace.list", "查看目录", "列出文件"),
        ];
        assert!(
            search(tools.clone(), "快捷任务 保存 shortcut", 8)
                .iter()
                .any(|tool| tool.name == "shortcut.save")
        );
        assert_eq!(
            search(tools.clone(), "列出已保存的快捷任务入口", 8)[0].name,
            "shortcut.list"
        );
        assert_eq!(
            search(tools.clone(), "workspace.list", 8)[0].name,
            "workspace.list"
        );
        assert!(search(tools, "unrelated-capability", 8).is_empty());
    }
}
