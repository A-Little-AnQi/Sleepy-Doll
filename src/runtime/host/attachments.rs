use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    Checkpoint,
    SkillCatalog,
    SkillInstructions,
    Preference,
    Resource,
    Diagnostic,
    HookContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub kind: AttachmentKind,
    pub content: String,
    pub priority: u8,
}

#[derive(Debug, Default)]
pub struct AttachmentSet {
    attachments: Vec<Attachment>,
    ids: HashSet<String>,
}

impl AttachmentSet {
    pub fn insert(&mut self, attachment: Attachment) {
        if attachment.content.is_empty() || !self.ids.insert(attachment.id.clone()) {
            return;
        }
        self.attachments.push(attachment);
    }

    pub fn render(mut self, max_chars: usize) -> String {
        self.attachments.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.id.cmp(&right.id))
        });
        let full_size = self
            .attachments
            .iter()
            .map(|attachment| {
                format!(
                    "<attachment kind={:?} id={:?}>\n",
                    attachment.kind, attachment.id
                )
                .chars()
                .count()
                    + attachment.content.chars().count()
                    + "\n</attachment>\n".chars().count()
            })
            .sum::<usize>();
        let report_budget = if full_size > max_chars {
            (max_chars / 4).min(1024)
        } else {
            0
        };
        let content_budget = max_chars.saturating_sub(report_budget);
        let mut result = String::new();
        let mut omitted = Vec::new();
        for attachment in self.attachments {
            let header = format!(
                "<attachment kind={:?} id={:?}>\n",
                attachment.kind, attachment.id
            );
            let footer = "\n</attachment>\n";
            if result.chars().count()
                + header.chars().count()
                + attachment.content.chars().count()
                + footer.chars().count()
                > content_budget
            {
                omitted.push(attachment.id);
                continue;
            }
            result.push_str(&header);
            result.push_str(&attachment.content);
            result.push_str(footer);
        }
        if !omitted.is_empty() {
            let header =
                "\n未载入的资料（仅目录，不能当作已读；技能可用 skills.read 按名称读取）：\n";
            let mut report = String::from(header);
            for id in omitted {
                let line = format!("- {}\n", id.strip_prefix("skill:").unwrap_or(&id));
                if report.chars().count() + line.chars().count() > report_budget {
                    break;
                }
                report.push_str(&line);
            }
            if report.chars().count() <= report_budget {
                result.push_str(&report);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_instructions_are_named_without_partial_body_or_budget_overrun() {
        let mut attachments = AttachmentSet::default();
        attachments.insert(Attachment {
            id: "skill:large-guide".into(),
            kind: AttachmentKind::SkillInstructions,
            content: "正文".repeat(4000),
            priority: 30,
        });
        attachments.insert(Attachment {
            id: "catalog".into(),
            kind: AttachmentKind::SkillCatalog,
            content: "目录".into(),
            priority: 40,
        });
        let result = attachments.render(1200);
        assert!(result.contains("large-guide"));
        assert!(result.contains("未载入"));
        assert!(!result.contains("正文"));
        assert!(result.chars().count() <= 1200);
    }
}
