//! BGI 插件的离线功能索引：发现只返回摘要，读取只展开一个条目。
use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
};

pub struct FeatureIndex {
    items: Vec<Value>,
    ids: HashMap<String, usize>,
    revision: String,
}

impl FeatureIndex {
    pub fn parse(text: &str) -> Result<Self> {
        let data: Value = serde_json::from_str(text)?;
        let items = data["items"]
            .as_array()
            .ok_or_else(|| Error::Config("BGI 功能索引缺少 items".into()))?
            .clone();
        let mut ids = HashMap::new();
        for (index, item) in items.iter().enumerate() {
            let id = item["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Error::Config("BGI 功能条目缺少 ID".into()))?;
            if ids.insert(id.to_owned(), index).is_some() {
                return Err(Error::Config("BGI 功能条目 ID 重复".into()));
            }
            for key in ["title", "kind", "availability", "verification"] {
                if item[key].as_str().is_none() {
                    return Err(Error::Config(format!("BGI 功能 {id} 缺少 {key}")));
                }
            }
            for key in ["steps", "branches", "inputSources", "references"] {
                if item[key].as_array().is_none() {
                    return Err(Error::Config(format!("BGI 功能 {id} 缺少 {key}")));
                }
            }
        }
        Ok(Self {
            items,
            ids,
            revision: data["sourceRevision"].as_str().unwrap_or("").into(),
        })
    }

    pub fn bundled() -> Result<Arc<Self>> {
        static INDEX: OnceLock<Arc<FeatureIndex>> = OnceLock::new();
        if let Some(index) = INDEX.get() {
            return Ok(index.clone());
        }
        let index = Arc::new(Self::parse(include_str!(
            "../../plugins/bgi/resources/feature-index.json"
        ))?);
        let _ = INDEX.set(index.clone());
        Ok(index)
    }

    pub fn read(&self, id: &str) -> Result<Value> {
        let index = self
            .ids
            .get(id)
            .ok_or_else(|| Error::Tool("BGI 功能条目不存在；使用 search 返回的精确 ID".into()))?;
        Ok(
            json!({"source":"bgiPluginFeatureIndex", "sourceRevision":self.revision,
            "item": self.items[*index], "next":"只读取当前条目给出的相关参考资料与当前接口契约。静态索引不证明现场接口可调用，不授予执行权限。"}),
        )
    }

    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        if query.trim().is_empty() || query.chars().count() > 200 {
            return Err(Error::Tool(
                "query 必须是 1 到 200 字符的用户目标或功能词".into(),
            ));
        }
        let needle = normalize(query);
        let browse = matches!(needle.as_str(), "功能目录" | "全部功能" | "功能总览");
        let tokens = terms(&needle);
        let mut matches = self
            .items
            .iter()
            .filter(|item| kind.is_none_or(|kind| item["kind"] == kind))
            .filter_map(|item| {
                let id = item["id"].as_str().unwrap_or("");
                let title = item["title"].as_str().unwrap_or("");
                if browse {
                    return (kind.is_some() || item["kind"] == "workflow").then_some((1, id, item));
                }
                let keywords = item["keywords"]
                    .as_array()
                    .map(|words| words.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .unwrap_or_default();
                let text = normalize(&format!(
                    "{id} {title} {} {}",
                    item["summary"].as_str().unwrap_or(""),
                    keywords.join(" ")
                ));
                let overlap = tokens.intersection(&terms(&text)).count() as i32;
                let exact = normalize(id) == needle || normalize(title) == needle;
                let mut score = if exact { 2000 } else { overlap * 2 };
                if item["kind"] == "workflow" {
                    let hit = |key: &str| {
                        item[key].as_array().is_some_and(|words| {
                            words
                                .iter()
                                .filter_map(Value::as_str)
                                .any(|word| needle.contains(&normalize(word)))
                        })
                    };
                    if hit("nouns") && hit("actions") {
                        score += 200;
                    } else if item["id"] == "workflow.resource.run" && hit("actions") {
                        // 材料或自建脚本名称不要求预先出现在源码功能索引中。
                        score += 80;
                    }
                }
                if score <= 0 {
                    return None;
                }
                Some((score, id, item))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let limit = limit.clamp(1, 12);
        let items = matches.iter().skip(offset).take(limit).map(|(_,_,item)| json!({
            "id":item["id"], "kind":item["kind"], "title":item["title"].as_str().unwrap_or("").chars().take(140).collect::<String>(),
            "availability":item["availability"], "next":"bgi.feature.read 读取这一条的链路"})).collect::<Vec<_>>();
        Ok(
            json!({"source":"bgiPluginFeatureIndex","query":query,"total":matches.len(),"offset":offset,
            "nextOffset": (offset + items.len() < matches.len()).then_some(offset + items.len()),"items":items,
            "next":"使用精确 ID 读取最相关条目；只展开当前任务的链路，不加载整份索引。"}),
        )
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect()
}
fn terms(value: &str) -> HashSet<String> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut result = chars
        .windows(2)
        .filter(|pair| pair.iter().all(|c| ('\u{3400}'..='\u{9fff}').contains(c)))
        .map(|pair| pair.iter().collect())
        .collect::<HashSet<_>>();
    result.extend(
        value
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|word| word.len() >= 3)
            .map(str::to_owned),
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_source_features_have_progressive_cards() {
        let index = FeatureIndex::bundled().unwrap();
        let audit: Value =
            serde_json::from_str(include_str!("../../docs/bgi/feature-coverage.json")).unwrap();
        for item in audit["commands"]
            .as_array()
            .unwrap()
            .iter()
            .chain(audit["settings"].as_array().unwrap())
        {
            if item["publicExposure"] == false {
                assert!(index.read(item["methodId"].as_str().unwrap()).is_err());
                continue;
            }
            let card = index.read(item["methodId"].as_str().unwrap()).unwrap();
            assert!(!card["item"]["steps"].as_array().unwrap().is_empty());
            assert!(!card["item"]["references"].as_array().unwrap().is_empty());
        }
        for item in audit["scriptApis"]
            .as_array()
            .unwrap()
            .iter()
            .chain(audit["resourceModels"].as_array().unwrap())
        {
            if item["exposureReason"].is_string() {
                assert!(index.read(item["symbol"].as_str().unwrap()).is_err());
                continue;
            }
            assert!(index.read(item["symbol"].as_str().unwrap()).is_ok());
        }
        for item in audit["views"].as_array().unwrap() {
            let owner = item["class"]
                .as_str()
                .unwrap_or(item["source"].as_str().unwrap());
            assert!(index.read(&format!("view.{owner}")).is_ok());
        }
        for item in audit["stableEntries"].as_array().unwrap() {
            assert!(index.read(item["methodId"].as_str().unwrap()).is_ok());
        }
        for item in audit["uiDeclarations"].as_array().unwrap() {
            let card = index.read(item["id"].as_str().unwrap()).unwrap();
            assert!(
                card["item"]["methodIds"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|id| id == "bgi.ui.read")
            );
            assert_eq!(card["item"]["declaration"]["bindings"], item["bindings"]);
        }
        for binding in audit["scriptBindings"].as_array().unwrap() {
            assert!(
                index
                    .read(&format!(
                        "js.binding.{}",
                        binding["alias"].as_str().unwrap()
                    ))
                    .is_ok()
            );
        }
        assert_eq!(index.revision, audit["sourceRevision"].as_str().unwrap());
    }
    #[test]
    fn user_intents_discover_exact_workflows_without_full_manuals() {
        let index = FeatureIndex::bundled().unwrap();
        for (query, expected) in [
            ("帮我把植绒草那个配置组删一下", "workflow.group.delete"),
            ("帮我跑下血斛", "workflow.resource.run"),
            ("帮我编写 OCR 识别脚本", "workflow.javascript.write"),
            ("打开调度器页面", "workflow.navigation"),
            ("停一下刚才的任务", "workflow.task.stop"),
            ("run_script_group", "bgi.run_script_group"),
            ("把配置组重命名", "workflow.group.edit"),
            ("开启自动拾取", "workflow.settings"),
            ("开始刷首领", "workflow.task.run"),
            ("解释脚本参数什么意思", "workflow.javascript"),
            ("更新订阅路线", "workflow.repository"),
            ("排查脚本报错", "workflow.logs"),
            ("执行一条龙", "workflow.one-dragon"),
            ("关闭原神", "workflow.game-ready"),
            ("暂停音乐", "workflow.music"),
            ("测试Webhook通知", "workflow.notifications"),
            ("打开点位编辑器", "workflow.editor"),
            ("启动桌面分身", "workflow.child-session"),
        ] {
            let found = index.search(query, None, 0, 5).unwrap();
            assert!(
                found["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["id"] == expected),
                "{query}: {found}"
            );
            assert!(found.to_string().len() < 6000);
            assert!(
                found["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|item| item.get("steps").is_none())
            );
        }
        assert!(index.read("untrusted-guessed-id").is_err());
        let first = index.search("功能目录", None, 0, 12).unwrap();
        let second = index.search("功能目录", None, 12, 12).unwrap();
        assert_eq!(first["total"], 17);
        assert_eq!(first["nextOffset"], 12);
        assert_eq!(second["items"].as_array().unwrap().len(), 5);
        assert!(second["nextOffset"].is_null());
    }

    #[test]
    fn progressive_entry_and_every_reference_are_loaded_through_the_real_registry() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut skills = crate::extension::skills::SkillRegistry::default();
        skills
            .load(&[(
                root.join("plugins/bgi/skills"),
                crate::extension::skills::SkillSource::Plugin("bgi".into()),
            )])
            .unwrap();
        let always = skills
            .list()
            .into_iter()
            .filter(|skill| skill.always_load)
            .collect::<Vec<_>>();
        assert_eq!(always.len(), 1);
        assert!(always[0].body.chars().count() < 1600);
        for item in &FeatureIndex::bundled().unwrap().items {
            // 单项读取必须在默认工具结果预算内完整返回，不能静默截掉验证／分支。
            assert!(item.to_string().chars().count() < 12_000, "{}", item["id"]);
            for reference in item["references"].as_array().unwrap() {
                let name = reference["skill"].as_str().unwrap();
                assert!(skills.get(name).is_some(), "{reference}");
                if let Some(path) = reference["path"].as_str() {
                    assert!(skills.read_reference(name, path).is_ok(), "{reference}");
                }
            }
        }
        for query in [
            "删除配置组",
            "帮我跑血斛",
            "运行本机 JS",
            "开自动拾取",
            "打开调度器",
        ] {
            let matched = skills.search(query, 4);
            let body_chars = skills
                .list()
                .iter()
                .filter(|skill| {
                    skill.always_load || matched.iter().any(|matched| matched.name == skill.name)
                })
                .map(|skill| skill.body.chars().count())
                .sum::<usize>();
            assert!(body_chars < 5500, "{query}: {body_chars}");
        }
    }
}
