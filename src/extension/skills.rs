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
    /// 需要哪些领域提供方在线。领域知识随提供方走 —— 提供方关掉，它的说明也
    /// 不该继续出现在提示词里。
    pub requires_providers: Vec<String>,
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
    /// 当前可用的领域提供方标识，例如 `bgi`。
    pub providers: &'a HashSet<String>,
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
                        requires_providers: list_field(&fields, "requiresProviders"),
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
                .requires_providers
                .iter()
                .all(|item| context.providers.contains(item))
            && skill
                .resource_kinds
                .iter()
                .all(|item| context.resource_kinds.contains(item))
    }
}

/// 把一份含 `SKILL.md` 的目录拷进用户技能根。名称来自 frontmatter，否则用目录名。
pub fn install(destination_root: &Path, source: &Path) -> Result<String> {
    let source = source.canonicalize()?;
    let directory = if source.is_file() {
        if source.file_name().and_then(|n| n.to_str()) != Some("SKILL.md") {
            return Err(Error::Config("请选择包含 SKILL.md 的技能目录".into()));
        }
        source
            .parent()
            .ok_or_else(|| Error::Config("技能路径无效".into()))?
            .to_path_buf()
    } else {
        source
    };
    let skill_md = directory.join("SKILL.md");
    if !skill_md.is_file() {
        return Err(Error::Config("目录里没有 SKILL.md".into()));
    }
    let text = fs::read_to_string(&skill_md)?;
    let (fields, _) = frontmatter(&text);
    let name = fields
        .get("name")
        .cloned()
        .or_else(|| {
            directory
                .file_name()
                .and_then(|n| n.to_str())
                .map(str::to_owned)
        })
        .ok_or_else(|| Error::Config("skill has no name".into()))?;
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Config("技能名称不合法".into()));
    }
    fs::create_dir_all(destination_root)?;
    let destination = destination_root.join(&name);
    if destination.exists() {
        return Err(Error::Config(format!("技能 {name} 已存在")));
    }
    if directory == destination
        || directory.starts_with(&destination)
        || destination.starts_with(&directory)
    {
        return Err(Error::Config("源目录与安装目录不能重叠".into()));
    }
    let mut files = Vec::new();
    collect_skill_files(&directory, &directory, &mut files)?;
    let total = files.iter().try_fold(0u64, |sum, path| {
        Ok::<_, std::io::Error>(sum + fs::metadata(directory.join(path))?.len())
    })?;
    if files.len() > 200 || total > 2 * 1024 * 1024 {
        return Err(Error::Config("技能超过本地安装大小限制".into()));
    }
    let stage = destination_root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let copied = (|| -> Result<()> {
        for path in &files {
            let target = stage.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(directory.join(path), target)?;
        }
        Ok(())
    })();
    if let Err(error) = copied {
        let _ = fs::remove_dir_all(&stage);
        return Err(error);
    }
    if let Err(error) = fs::rename(&stage, &destination) {
        let _ = fs::remove_dir_all(&stage);
        return Err(error.into());
    }
    Ok(name)
}

fn collect_skill_files(root: &Path, dir: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    if dir.components().count() > root.components().count() + 8 {
        return Err(Error::Config("技能目录层级过深".into()));
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') || name == "node_modules" {
            continue;
        }
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(Error::Config("技能安装不接受符号链接".into()));
        }
        if ty.is_dir() {
            collect_skill_files(root, &entry.path(), result)?;
        } else if ty.is_file() {
            if fs::metadata(entry.path())?.len() > 128 * 1024 {
                return Err(Error::Config("技能文件超过 128KB".into()));
            }
            result.push(entry.path().strip_prefix(root).unwrap().into());
        } else {
            return Err(Error::Config("技能含有不支持的文件类型".into()));
        }
        if result.len() > 200 {
            return Err(Error::Config("技能文件数过多".into()));
        }
    }
    Ok(())
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

    #[test]
    fn install_copies_a_skill_directory() {
        let source = tempfile::tempdir().unwrap();
        let skill = source.path().join("my-note");
        fs::create_dir(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: my-note\ndescription: 本机笔记\n---\n\n# 笔记\n",
        )
        .unwrap();
        fs::create_dir(skill.join("references")).unwrap();
        fs::write(skill.join("references/a.md"), "ref").unwrap();
        let dest = tempfile::tempdir().unwrap();
        let name = install(dest.path(), &skill).unwrap();
        assert_eq!(name, "my-note");
        assert!(dest.path().join("my-note/SKILL.md").is_file());
        assert!(dest.path().join("my-note/references/a.md").is_file());
        assert!(install(dest.path(), &skill).is_err());
    }
}
