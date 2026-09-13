use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: String,
    pub tags: Vec<String>,
    pub requires_plugins: Vec<String>,
    pub requires_capabilities: Vec<String>,
    pub resource_kinds: Vec<String>,
    pub platforms: Vec<String>,
    pub allowed_tools: Vec<String>,
    /// Domain-wide operating manuals are attached to every run. This is for
    /// stable product context, not a way to grant tools or permissions.
    pub always_load: bool,
    #[serde(skip)]
    pub body: String,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct SkillContext<'a> {
    pub plugins: &'a HashSet<String>,
    pub capabilities: &'a HashSet<String>,
    pub resource_kinds: &'a HashSet<String>,
    pub platform: &'a str,
}

#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn load(&mut self, directories: &[(PathBuf, String)]) -> Result<()> {
        for (directory, source) in directories {
            for path in discover(directory, 0)? {
                let metadata = fs::metadata(&path)?;
                if metadata.len() > 128 * 1024 {
                    return Err(Error::Config(format!(
                        "skill is too large: {}",
                        path.display()
                    )));
                }
                let text = fs::read_to_string(&path)?;
                let (fields, body) = frontmatter(&text);
                let name = fields
                    .get("name")
                    .cloned()
                    .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_owned))
                    .ok_or_else(|| Error::Config("skill has no name".into()))?;
                if self.skills.contains_key(&name) {
                    return Err(Error::Config(format!("duplicate skill: {name}")));
                }
                let description = fields
                    .get("description")
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                let tags = list_field(&fields, "tags");
                self.skills.insert(
                    name.clone(),
                    Skill {
                        name,
                        description,
                        source: source.clone(),
                        tags,
                        requires_plugins: list_field(&fields, "requiresPlugins"),
                        requires_capabilities: list_field(&fields, "requiresCapabilities"),
                        resource_kinds: list_field(&fields, "resourceKinds"),
                        platforms: list_field(&fields, "platforms"),
                        allowed_tools: list_field(&fields, "allowedTools"),
                        always_load: fields
                            .get("alwaysLoad")
                            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
                        body,
                        path,
                    },
                );
            }
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<Skill> {
        let mut skills = self.skills.values().cloned().collect::<Vec<_>>();
        skills.sort_by(|left, right| left.name.cmp(&right.name));
        skills
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn read_reference(&self, name: &str, relative: &str) -> Result<String> {
        let skill = self
            .get(name)
            .ok_or_else(|| Error::Tool("Skill 不存在".into()))?;
        let root = skill
            .path
            .parent()
            .ok_or_else(|| Error::Tool("Skill 路径无效".into()))?
            .canonicalize()?;
        let relative = Path::new(relative);
        if relative.is_absolute() {
            return Err(Error::Tool("引用必须是相对路径".into()));
        }
        let path = root.join(relative).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() || fs::metadata(&path)?.len() > 128 * 1024 {
            return Err(Error::Tool("引用越界或超过大小限制".into()));
        }
        Ok(fs::read_to_string(path)?)
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Skill> {
        let query_tokens = tokens(query);
        let mut scored = self
            .skills
            .values()
            .filter_map(|skill| {
                let haystack = tokens(&format!(
                    "{} {} {}",
                    skill.name,
                    skill.description,
                    skill.tags.join(" ")
                ));
                let explicit = query.contains(&format!("${}", skill.name));
                let score = if explicit {
                    1000
                } else {
                    query_tokens
                        .intersection(&haystack)
                        .map(|token| token.len())
                        .sum()
                };
                (score > 0).then_some((score, skill.clone()))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.name.cmp(&right.1.name))
        });
        scored
            .into_iter()
            .take(limit)
            .map(|(_, skill)| skill)
            .collect()
    }

    pub fn eligible(&self, skill: &Skill, context: &SkillContext<'_>) -> bool {
        (skill.platforms.is_empty() || skill.platforms.iter().any(|item| item == context.platform))
            && skill
                .requires_plugins
                .iter()
                .all(|item| context.plugins.contains(item))
            && skill
                .requires_capabilities
                .iter()
                .all(|item| context.capabilities.contains(item))
            && skill
                .resource_kinds
                .iter()
                .all(|item| context.resource_kinds.contains(item))
    }
}

fn list_field(fields: &HashMap<String, String>, name: &str) -> Vec<String> {
    fields
        .get(name)
        .map(|value| {
            value
                .trim_matches(['[', ']'])
                .split(',')
                .map(|item| item.trim().trim_matches(['\'', '"']))
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn discover(directory: &Path, depth: usize) -> Result<Vec<PathBuf>> {
    if depth > 3 || !directory.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            result.extend(discover(&path, depth + 1)?);
        } else if name == "SKILL.md" {
            result.push(path);
        }
    }
    Ok(result)
}

/// Splits a `SKILL.md` into its YAML-ish frontmatter fields and its body.
///
/// Line endings are normalised first. The repository stores these files with LF,
/// but `core.autocrlf` checks them out as CRLF on Windows, and the previous
/// implementation matched on `"\n"` literally. On any such checkout it found no
/// frontmatter at all: the skill name fell back to its directory, the
/// description fell back to the name, and the raw `---` block was handed to the
/// model as if it were instructions.
fn frontmatter(text: &str) -> (HashMap<String, String>, String) {
    let normalized = text.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    if lines.next() != Some("---") {
        return (HashMap::new(), text.to_owned());
    }
    let mut fields = HashMap::new();
    let mut body = Vec::new();
    let mut closed = false;
    for line in lines {
        if closed {
            body.push(line);
            continue;
        }
        if line.trim_end() == "---" {
            closed = true;
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            fields.insert(
                key.trim().to_owned(),
                value.trim().trim_matches(['\'', '"']).to_owned(),
            );
        }
    }
    if !closed {
        // An unterminated block is not frontmatter; treat the file as all body.
        return (HashMap::new(), text.to_owned());
    }
    (fields, body.join("\n").trim().to_owned())
}

fn tokens(text: &str) -> HashSet<String> {
    let normalized = text.to_lowercase();
    let mut result: HashSet<String> = normalized
        .split(|character: char| {
            !character.is_alphanumeric() && !matches!(character, '.' | '_' | '-')
        })
        .filter(|item| item.chars().count() >= 2)
        .map(str::to_owned)
        .collect();
    let chars: Vec<char> = normalized.chars().collect();
    for pair in chars.windows(2) {
        if pair.iter().all(|c| ('\u{3400}'..='\u{9fff}').contains(c)) {
            result.insert(pair.iter().collect());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const LF: &str = "---\nname: bgi-operator\ndescription: 使用 BGI Bridge 查询游戏状态\ntags: BGI, 自动化\n---\n\n# 操作规范\n\n正文第一行。\n";

    /// `core.autocrlf` checks the committed LF out as CRLF on Windows. Parsing
    /// must not depend on which form is on disk.
    #[test]
    fn frontmatter_parses_with_either_line_ending() {
        let crlf = LF.replace('\n', "\r\n");
        for text in [LF, crlf.as_str()] {
            let (fields, body) = frontmatter(text);
            assert_eq!(fields.get("name").map(String::as_str), Some("bgi-operator"));
            assert_eq!(
                fields.get("description").map(String::as_str),
                Some("使用 BGI Bridge 查询游戏状态")
            );
            assert_eq!(fields.get("tags").map(String::as_str), Some("BGI, 自动化"));
            assert_eq!(body, "# 操作规范\n\n正文第一行。");
        }
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let (fields, body) = frontmatter("# 标题\n\n正文。\n");
        assert!(fields.is_empty());
        assert_eq!(body, "# 标题\n\n正文。\n");

        // An opening fence with no closing one is not frontmatter either.
        let (fields, body) = frontmatter("---\nname: x\n");
        assert!(fields.is_empty());
        assert_eq!(body, "---\nname: x\n");
    }

    #[test]
    fn the_shipped_skill_keeps_its_metadata() {
        let text = include_str!("../../skills/bgi-operator/SKILL.md");
        let (fields, body) = frontmatter(text);
        assert_eq!(fields.get("name").map(String::as_str), Some("bgi-operator"));
        // 断言的是「描述来自 frontmatter 且有实质内容」，不是某个固定短语 ——
        // 技能可以改写措辞，但退化成名字回显就会失配。
        assert!(
            fields
                .get("description")
                .is_some_and(|value| value.contains("BetterGI") && value.chars().count() > 20),
            "description should come from the frontmatter, not fall back to the name"
        );
        // 守的是「frontmatter 被切干净了」，不是「正文里不能出现 ---」——
        // markdown 表格的分隔行本身就是 `|---|`。
        assert!(
            !body.trim_start().starts_with("---") && !body.contains("name: bgi-operator"),
            "the fence must not reach the body"
        );
        assert!(body.starts_with("# BGI 操作规范"));
        assert_eq!(fields.get("alwaysLoad").map(String::as_str), Some("true"));
    }
}
