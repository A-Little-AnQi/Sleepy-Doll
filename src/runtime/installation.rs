//! Local plugin installation. Loading metadata never launches the plugin.
use crate::{
    config::AppConfig,
    error::{Error, Result},
    plugins::PluginManifest,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn install(config: &AppConfig, source: &Path) -> Result<String> {
    let source = source.canonicalize()?;
    let manifest: PluginManifest =
        serde_json::from_slice(&fs::read(source.join(".sleepy-doll-plugin/plugin.json"))?)?;
    let id = &manifest.id;
    if manifest.schema_version != 1
        || id.is_empty()
        || id.len() > 80
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(Error::Config("插件标识或版本不合法".into()));
    }
    if config.plugins.enabled.contains(id) {
        return Err(Error::Config("更新插件前请先停用它".into()));
    }
    for skill in &manifest.skills {
        let resolved = source.join(skill).canonicalize()?;
        if skill.is_absolute() || !resolved.starts_with(&source) {
            return Err(Error::Config("插件 Skill 引用超出插件目录".into()));
        }
    }
    for tool in &manifest.http_tools {
        jsonschema::validator_for(&tool.input_schema)
            .map_err(|_| Error::Config("插件工具 Schema 无效".into()))?;
        if let Some(schema) = &tool.output_schema {
            jsonschema::validator_for(schema)
                .map_err(|_| Error::Config("插件工具输出 Schema 无效".into()))?;
        }
        tool.execution.validate()?;
    }
    for server in &manifest.mcp_servers {
        for execution in server.tool_execution.values() {
            execution.validate()?;
        }
    }
    let root = config
        .plugins
        .directories
        .first()
        .ok_or_else(|| Error::Config("请先配置插件存放目录".into()))?;
    fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let destination = root.join(id);
    if source == destination || source.starts_with(&destination) || destination.starts_with(&source)
    {
        return Err(Error::Config("源目录与安装目录不能重叠".into()));
    }
    let mut files = Vec::new();
    collect(&source, &source, &mut files)?;
    let total = files.iter().try_fold(0u64, |sum, p| {
        Ok::<_, std::io::Error>(sum + fs::metadata(source.join(p))?.len())
    })?;
    if files.len() > 1000 || total > 50 * 1024 * 1024 {
        return Err(Error::Config("插件超过本地安装大小限制".into()));
    }
    let stage = root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&stage)?;
    for path in files {
        let target = stage.join(&path);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(source.join(path), target)?;
    }
    let backup = if destination.exists() {
        Some(retire(&root, &destination)?)
    } else {
        None
    };
    if let Err(error) = fs::rename(&stage, &destination) {
        if let Some(backup) = backup {
            let _ = fs::rename(backup, &destination);
        }
        return Err(error.into());
    }
    Ok(id.clone())
}

pub fn remove(config: &AppConfig, id: &str) -> Result<PathBuf> {
    if config.plugins.enabled.iter().any(|p| p == id) {
        return Err(Error::Config("移出插件前请先停用它".into()));
    }
    for root in &config.plugins.directories {
        if !root.exists() {
            continue;
        }
        let root = root.canonicalize()?;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') || !entry.file_type()?.is_dir()
            {
                continue;
            }
            let path = entry.path();
            let file = path.join(".sleepy-doll-plugin/plugin.json");
            if !file.exists() {
                continue;
            }
            let manifest: PluginManifest = serde_json::from_slice(&fs::read(file)?)?;
            if manifest.id == id {
                return retire(&root, &path);
            }
        }
    }
    Err(Error::Config("插件未找到".into()))
}
fn retire(root: &Path, target: &Path) -> Result<PathBuf> {
    let exact = target.canonicalize()?;
    if exact.parent() != Some(root) {
        return Err(Error::Config("插件路径越界".into()));
    }
    let retired = root.join(".retired");
    fs::create_dir_all(&retired)?;
    let destination = retired.join(format!(
        "{}-{}",
        target.file_name().unwrap().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    fs::rename(exact, &destination)?;
    Ok(destination)
}
fn collect(root: &Path, dir: &Path, result: &mut Vec<PathBuf>) -> Result<()> {
    if dir.components().count() > root.components().count() + 16 {
        return Err(Error::Config("插件目录层级过深".into()));
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(Error::Config("插件安装不接受符号链接".into()));
        }
        if entry.file_name() == "node_modules" || entry.file_name() == ".git" {
            continue;
        }
        if ty.is_dir() {
            collect(root, &entry.path(), result)?;
        } else if ty.is_file() {
            result.push(entry.path().strip_prefix(root).unwrap().into());
        } else {
            return Err(Error::Config("插件含有不支持的文件类型".into()));
        }
        if result.len() > 1000 {
            return Err(Error::Config("插件文件数过多".into()));
        }
    }
    Ok(())
}
