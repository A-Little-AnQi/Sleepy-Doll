use super::types::hash;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capability {
    pub id: String,
    pub description: String,
    pub method_id: String,
    pub catalog_version: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub resource_fields: Vec<String>,
    #[serde(default)]
    pub postconditions: Vec<Predicate>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Predicate {
    Equals {
        pointer: String,
        value: Value,
        max_age_sec: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: PathBuf,
    pub content_hash: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub coordinate_system: Option<String>,
    #[serde(default)]
    pub map_layer: Option<String>,
}
#[derive(Clone, Default)]
pub struct Catalog {
    root: PathBuf,
    pub capabilities: HashMap<String, Capability>,
    pub resources: HashMap<String, Resource>,
}
impl Catalog {
    pub fn load(root: &Path) -> Result<Self> {
        let mut catalog = Self {
            root: root.canonicalize()?,
            ..Self::default()
        };
        for (dir, is_capability) in [("capabilities", true), ("resources", false)] {
            let path = root.join(dir);
            if !path.exists() {
                continue;
            }
            for entry in fs::read_dir(path)? {
                let path = entry?.path();
                if path.extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                if fs::metadata(&path)?.len() > 1024 * 1024 {
                    return Err(Error::Config("catalog descriptor too large".into()));
                }
                let data = fs::read_to_string(path)?;
                if is_capability {
                    let c: Capability = serde_json::from_str(&data)?;
                    if c.id.is_empty()
                        || c.catalog_version.is_empty()
                        || catalog.capabilities.insert(c.id.clone(), c).is_some()
                    {
                        return Err(Error::Config(
                            "invalid or duplicate semantic capability".into(),
                        ));
                    }
                } else {
                    let mut r: Resource = serde_json::from_str(&data)?;
                    r.path = root.join(&r.path).canonicalize()?;
                    if !r.path.starts_with(root.canonicalize()?) {
                        return Err(Error::Config(
                            "resource outside configured catalog root".into(),
                        ));
                    }
                    if catalog.resources.insert(r.id.clone(), r).is_some() {
                        return Err(Error::Config("duplicate resource".into()));
                    }
                }
            }
        }
        Ok(catalog)
    }
    pub fn search(&self, query: &str) -> Value {
        let q = query.to_lowercase();
        let terms = q.chars().collect::<Vec<_>>();
        let matching = |text: String| {
            let t = text.to_lowercase();
            t.contains(&q)
                || terms
                    .windows(2)
                    .any(|p| t.contains(&p.iter().collect::<String>()))
        };
        json!({"capabilities":self.capabilities.values().filter(|c|matching(format!("{} {} {}",c.id,c.description,c.aliases.join(" ")))).take(12).collect::<Vec<_>>(),"resources":self.resources.values().filter(|r|matching(format!("{} {} {}",r.id,r.name,r.aliases.join(" ")))).take(12).map(|r|json!({"id":r.id,"name":r.name,"kind":r.kind,"hash":r.content_hash,"coordinateSystem":r.coordinate_system,"mapLayer":r.map_layer})).collect::<Vec<_>>()})
    }
    pub fn resolve(&self, id: &str, args: &Value) -> Result<(Capability, Value)> {
        let c = self
            .capabilities
            .get(id)
            .ok_or_else(|| Error::Tool("能力尚未建立语义绑定".into()))?
            .clone();
        let mut refs = Vec::new();
        for field in &c.resource_fields {
            let id = args
                .pointer(field)
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Tool("缺少真实资源 ID".into()))?;
            let r = self
                .resources
                .get(id)
                .ok_or_else(|| Error::Tool("资源不存在，请选择已登记资源".into()))?;
            let path = r.path.canonicalize()?;
            if !path.starts_with(&self.root) || fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
                return Err(Error::Tool("资源路径越界或内容过大".into()));
            }
            let bytes = fs::read(&path)?;
            use sha2::{Digest, Sha256};
            let current = format!("{:x}", Sha256::digest(bytes));
            if current != r.content_hash {
                return Err(Error::Tool("资源内容已变化，需要更新索引并重新授权".into()));
            }
            refs.push(json!({"id":id,"hash":current}));
        }
        let version = hash(&json!(c));
        Ok((c, json!({"semanticVersion":version,"resources":refs})))
    }
}
