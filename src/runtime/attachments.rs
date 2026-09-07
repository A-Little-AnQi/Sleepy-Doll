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
        let mut result = String::new();
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
                > max_chars
            {
                continue;
            }
            result.push_str(&header);
            result.push_str(&attachment.content);
            result.push_str(footer);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_are_deduplicated_and_budgeted_by_priority() {
        let mut set = AttachmentSet::default();
        set.insert(Attachment {
            id: "low".into(),
            kind: AttachmentKind::Resource,
            content: "x".repeat(80),
            priority: 1,
        });
        set.insert(Attachment {
            id: "high".into(),
            kind: AttachmentKind::Checkpoint,
            content: "checkpoint".into(),
            priority: 10,
        });
        set.insert(Attachment {
            id: "high".into(),
            kind: AttachmentKind::Checkpoint,
            content: "duplicate".into(),
            priority: 10,
        });
        let rendered = set.render(120);
        assert!(rendered.contains("checkpoint"));
        assert!(!rendered.contains("duplicate"));
        assert!(!rendered.contains(&"x".repeat(80)));
    }
}
