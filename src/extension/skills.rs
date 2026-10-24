use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Serialize, Serializer};

use crate::error::{Error, Result};

/// 随二进制分发的产品技能清单。
pub const EMBEDDED_SKILLS: &[&str] = &[
    include_str!("skills/operate-task.md"),
    include_str!("skills/create-shortcut.md"),
    include_str!("skills/continue-task.md"),
    include_str!("skills/web-search.md"),
];

/// 技能的归属，决定谁开关它。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillSource {
    /// 随产品分发，不在任何插件目录里。
    Product,
    /// 用户导入到配置旁的技能目录。
    User,
    /// 插件目录内的技能，登记持有它的插件 id。
    Plugin(String),
}

impl SkillSource {
    /// 持有这项技能的插件。`None` 表示由用户自己开关。
    pub fn plugin(&self) -> Option<&str> {
        match self {
            Self::Plugin(id) => Some(id),
            Self::Product | Self::User => None,
        }
    }
}

impl Serialize for SkillSource {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Product => serializer.serialize_str("product"),
            Self::User => serializer.serialize_str("user"),
            Self::Plugin(id) => serializer.serialize_str(&format!("plugin:{id}")),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    pub tags: Vec<String>,
    pub requires_plugins: Vec<String>,
    pub requires_capabilities: Vec<String>,
    /// 需要哪些领域提供方在线。
    pub requires_providers: Vec<String>,
    pub resource_kinds: Vec<String>,
    pub platforms: Vec<String>,
    pub allowed_tools: Vec<String>,
    /// 已启用的短入口说明；完整操作手册按需读取。
    pub always_load: bool,
    /// 是否出现在用户技能列表和手动选择菜单；不影响模型发现与读取。
    #[serde(skip)]
    pub user_visible: bool,
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
    /// 当前可用的领域提供方标识。
    pub providers: &'a HashSet<String>,
    pub platform: &'a str,
}

#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn load(&mut self, directories: &[(PathBuf, SkillSource)]) -> Result<()> {
        for (directory, source) in directories {
            for path in discover(directory, 0)? {
                let metadata = fs::metadata(&path)?;
                if metadata.len() > 128 * 1024 {
                    return Err(Error::Config(format!("技能文件过大：{}", path.display())));
                }
                let text = fs::read_to_string(&path)?;
                let (fields, body) = frontmatter(&text);
                let name = fields
                    .get("name")
                    .cloned()
                    .or_else(|| path.parent()?.file_name()?.to_str().map(str::to_owned))
                    .ok_or_else(|| Error::Config("技能没有名称".into()))?;
                if self.skills.contains_key(&name) {
                    return Err(Error::Config(format!("技能名重复：{name}")));
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
                        user_visible: fields
                            .get("userVisible")
                            .is_none_or(|value| !value.eq_ignore_ascii_case("false")),
                        body,
                        path,
                    },
                );
            }
        }
        Ok(())
    }

    /// 随二进制分发的产品技能，注册在配置目录技能之后；用户放同名技能时让位。
    pub fn load_embedded(&mut self, text: &'static str) -> Result<()> {
        let (fields, body) = frontmatter(text);
        let name = fields
            .get("name")
            .cloned()
            .ok_or_else(|| Error::Config("内置技能没有名称".into()))?;
        if self.skills.contains_key(&name) {
            return Ok(());
        }
        let description = fields
            .get("description")
            .cloned()
            .unwrap_or_else(|| name.clone());
        self.skills.insert(
            name.clone(),
            Skill {
                name,
                description,
                source: SkillSource::Product,
                tags: list_field(&fields, "tags"),
                requires_plugins: list_field(&fields, "requiresPlugins"),
                requires_capabilities: list_field(&fields, "requiresCapabilities"),
                requires_providers: list_field(&fields, "requiresProviders"),
                resource_kinds: list_field(&fields, "resourceKinds"),
                platforms: list_field(&fields, "platforms"),
                allowed_tools: list_field(&fields, "allowedTools"),
                always_load: fields
                    .get("alwaysLoad")
                    .is_some_and(|value| value.eq_ignore_ascii_case("true")),
                user_visible: fields
                    .get("userVisible")
                    .is_none_or(|value| !value.eq_ignore_ascii_case("false")),
                body,
                path: PathBuf::new(),
            },
        );
        Ok(())
    }

    /// 握手返回的 MCP 操作说明按插件技能装载，停用插件或重载后自然移除。
    pub fn load_mcp_instructions(&mut self, plugin: &str, server: &str, body: &str) -> Result<()> {
        if body.trim().is_empty() {
            return Ok(());
        }
        if body.len() > 128 * 1024 {
            return Err(Error::Config("MCP instructions 超过大小限制".into()));
        }
        let name = format!("mcp-{plugin}-{server}");
        if self.skills.contains_key(&name) {
            return Err(Error::Config(format!("技能名重复：{name}")));
        }
        self.skills.insert(
            name.clone(),
            Skill {
                name,
                description: format!("{plugin} MCP 服务的操作说明"),
                source: SkillSource::Plugin(plugin.into()),
                tags: vec![plugin.into()],
                requires_plugins: vec![plugin.into()],
                requires_capabilities: vec![],
                requires_providers: vec![plugin.into()],
                resource_kinds: vec![],
                platforms: vec![],
                allowed_tools: vec![],
                always_load: true,
                user_visible: false,
                body: body.into(),
                path: PathBuf::new(),
            },
        );
        Ok(())
    }

    /// 全部技能，含插件带来的。
    pub fn list(&self) -> Vec<Skill> {
        self.sorted(|_| true)
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// 用户可见的独立技能；内部说明和插件附带技能不单独列出。
    pub fn standalone(&self) -> Vec<Skill> {
        self.sorted(|skill| skill.user_visible && skill.source.plugin().is_none())
    }

    fn sorted(&self, keep: impl Fn(&Skill) -> bool) -> Vec<Skill> {
        let mut skills = self
            .skills
            .values()
            .filter(|skill| keep(skill))
            .cloned()
            .collect::<Vec<_>>();
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

    pub fn explicit(&self, text: &str) -> Vec<Skill> {
        let names = text
            .split_whitespace()
            .filter_map(|word| word.strip_prefix('$'))
            .collect::<HashSet<_>>();
        let mut found = self
            .skills
            .values()
            .filter(|skill| names.contains(skill.name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        found.sort_by(|left, right| left.name.cmp(&right.name));
        found
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<Skill> {
        let explicit_names = self
            .explicit(query)
            .into_iter()
            .map(|skill| skill.name)
            .collect::<HashSet<_>>();
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
                let explicit = explicit_names.contains(&skill.name);
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

/// 把一份含 `SKILL.md` 的目录拷进用户技能根。名称取自 frontmatter，缺省用目录名。
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
        .ok_or_else(|| Error::Config("技能没有名称".into()))?;
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

/// 把 `SKILL.md` 拆成 frontmatter 字段与正文。
///
/// 先统一换行符：`core.autocrlf` 在 Windows 上把提交的 LF 检出成 CRLF。
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
        // 未闭合的块不算 frontmatter，整份文件当正文。
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
mod visibility_tests {
    use super::*;

    #[test]
    fn internal_product_guides_remain_discoverable_without_user_controls() {
        let mut registry = SkillRegistry::default();
        for source in EMBEDDED_SKILLS {
            registry.load_embedded(source).unwrap();
        }
        let visible = registry.standalone();
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].name, "create-shortcut");
        assert!(visible.iter().any(|skill| skill.name == "网络搜索"
            && skill.body.contains("web.search")
            && skill.body.contains("web.fetch")));
        for name in ["operate-task", "continue-task"] {
            let skill = registry.get(name).unwrap();
            assert!(!skill.body.is_empty());
            assert!(!skill.user_visible);
            assert!(registry.search(name, 8).iter().any(|s| s.name == name));
        }
        assert_eq!(registry.list().len(), 4);
    }

    #[test]
    fn ordinary_skills_keep_their_default_visibility() {
        let mut registry = SkillRegistry::default();
        registry
            .load_embedded("---\nname: my-skill\ndescription: 用户自己的操作说明\n---\n正文")
            .unwrap();
        assert_eq!(registry.standalone()[0].name, "my-skill");
        assert!(registry.get("my-skill").unwrap().user_visible);
    }
}
