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
    search_documents: Vec<SearchDocument>,
    inverse_frequency: HashMap<String, f64>,
    script_types: serde_json::Map<String, Value>,
    source_snapshot_hash: String,
}

struct SearchDocument {
    id: HashSet<String>,
    title: HashSet<String>,
    all: HashSet<String>,
}

impl FeatureIndex {
    pub fn parse(text: &str) -> Result<Self> {
        let data: Value = serde_json::from_str(text)?;
        if data["format"] != 2 {
            return Err(Error::Config("BGI 能力索引格式不支持".into()));
        }
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
            for key in ["title", "kind", "availability"] {
                if item[key].as_str().is_none() {
                    return Err(Error::Config(format!("BGI 功能 {id} 缺少 {key}")));
                }
            }
            for key in ["inputSources", "references"] {
                if item[key].as_array().is_none() {
                    return Err(Error::Config(format!("BGI 功能 {id} 缺少 {key}")));
                }
            }
        }
        let search_documents = items
            .iter()
            .map(|item| {
                let id = terms(item["id"].as_str().unwrap_or_default());
                let title = terms(item["title"].as_str().unwrap_or_default());
                let text = format!(
                    "{} {} {} {}",
                    item["id"].as_str().unwrap_or_default(),
                    item["title"].as_str().unwrap_or_default(),
                    item["summary"].as_str().unwrap_or_default(),
                    item["keywords"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                SearchDocument {
                    id,
                    title,
                    all: terms(&text),
                }
            })
            .collect::<Vec<_>>();
        let mut frequency = HashMap::<String, usize>::new();
        for document in &search_documents {
            for term in &document.all {
                *frequency.entry(term.clone()).or_default() += 1;
            }
        }
        let total = search_documents.len() as f64;
        let inverse_frequency = frequency
            .into_iter()
            .map(|(term, count)| {
                (
                    term,
                    (1.0 + (total - count as f64 + 0.5) / (count as f64 + 0.5)).ln(),
                )
            })
            .collect();
        Ok(Self {
            items,
            ids,
            revision: data["sourceRevision"].as_str().unwrap_or("").into(),
            search_documents,
            inverse_frequency,
            script_types: data["scriptTypes"].as_object().cloned().unwrap_or_default(),
            source_snapshot_hash: data["sourceSnapshotHash"]
                .as_str()
                .unwrap_or_default()
                .into(),
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
        self.read_page(id, 0, 8)
    }

    pub fn read_page(&self, id: &str, member_offset: usize, member_limit: usize) -> Result<Value> {
        let index = self
            .ids
            .get(id)
            .ok_or_else(|| Error::Tool("BGI 能力声明不存在；使用查询返回的精确标识".into()))?;
        let item = &self.items[*index];
        let declaration = &item["declaration"];
        let mut value = json!({"source":"bgiPluginFeatureIndex", "sourceRevision":self.revision,
            "sourceSnapshotHash":self.source_snapshot_hash,"capabilityState":"referenceOnly","declarationVerified":true,"executionVerified":false,"item":item,
            "next":"本结果是源码事实，非现场执行证明。类型成员按 nextMemberOffset 分页；需要执行时取得当前契约。"});
        if item["kind"] == "scriptApi" {
            let mut symbol = json!({"runtimeVerified":false,"kind":declaration["kind"]});
            if let Some(owner) = declaration["clrType"].as_str() {
                symbol["binding"] = declaration.clone();
                if let Some(member) = declaration["member"].as_str() {
                    let overloads = self
                        .script_types
                        .get(owner)
                        .and_then(|definition| definition["members"].as_array())
                        .into_iter()
                        .flatten()
                        .filter(|entry| entry["name"] == member)
                        .map(|entry| self.member_view(entry))
                        .collect::<Vec<_>>();
                    symbol["ownerType"] = json!(owner);
                    symbol["overloads"] = json!(overloads);
                } else {
                    symbol["type"] = self.type_view(owner, member_offset, member_limit);
                }
            } else if let Some(name) = id.strip_prefix("T:") {
                symbol["type"] = self.type_view(name, member_offset, member_limit);
            } else if declaration["owner"].is_string() {
                symbol["declaration"] = self.member_view(declaration);
            }
            value["symbol"] = symbol;
            // Return each fact once: script details live in the structured symbol,
            // while search-only terms and duplicate declaration payloads stay internal.
            if let Some(summary) = value["item"].as_object_mut() {
                for key in ["keywords", "declaration", "inputSources", "availability"] {
                    summary.remove(key);
                }
            }
        }
        Ok(value)
    }

    /// Resolve supplied symbol identities and join declaration facts. No intent classification or execution.
    pub fn inspect_symbols(&self, symbols: &[String]) -> Result<Value> {
        if symbols.is_empty() || symbols.len() > 8 {
            return Err(Error::Tool("一次检查 1 到 8 个符号".into()));
        }
        let mut profiles = Vec::new();
        let mut unresolved = Vec::new();
        let mut relationships = Vec::new();
        for symbol in symbols {
            let exact = self.ids.get(symbol.trim()).copied();
            let aliases = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item["declaration"]["alias"] == *symbol)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let candidates = if let Some(index) = exact {
                vec![index]
            } else if !aliases.is_empty() {
                aliases
            } else {
                self.items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| {
                        item["title"]
                            .as_str()
                            .is_some_and(|title| title.eq_ignore_ascii_case(symbol.trim()))
                            || item["declaration"]["name"] == *symbol
                    })
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>()
            };
            if candidates.len() != 1 {
                unresolved.push(json!({"symbol":symbol,"status":if candidates.is_empty(){"notFound"}else{"ambiguous"},
                    "candidates":candidates.iter().take(8).map(|index|json!({"id":self.items[*index]["id"],"title":self.items[*index]["title"]})).collect::<Vec<_>>()}));
                continue;
            }
            let item = &self.items[candidates[0]];
            let mut profile = self.read_page(item["id"].as_str().unwrap(), 0, 2)?;
            profile["requestedSymbol"] = json!(symbol);
            for overload in profile["symbol"]["overloads"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(target) = overload["returns"]["resolvedType"].as_str() {
                    relationships.push(json!({"from":symbol,"kind":"returns","to":target,"source":overload["source"],"line":overload["line"]}));
                    for base in overload["returns"]["declaredBases"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        relationships.push(json!({"from":target,"kind":"inherits","to":base}));
                    }
                }
            }
            let type_view = &profile["symbol"]["type"];
            for base in type_view["bases"].as_array().into_iter().flatten() {
                relationships.push(json!({"from":type_view["name"],"kind":"inherits","to":base,"source":type_view["source"],"line":type_view["line"]}));
            }
            let mut definition = json!({"symbol":symbol,"id":item["id"],"kind":profile["symbol"]["kind"],
                "source":item["source"],"summary":item["summary"]});
            if let Some(overloads) = profile["symbol"]["overloads"].as_array() {
                definition["overloads"] = json!(overloads.iter().map(|entry| json!({"parameters":entry["parameters"],
                    "returns":entry["returns"],"isStatic":entry["isStatic"],"source":entry["source"],"line":entry["line"]})).collect::<Vec<_>>());
            }
            if type_view.is_object() {
                definition["type"] = json!({"name":type_view["name"],"bases":type_view["bases"],"memberTotal":type_view["totalMembers"],
                    "summary":type_view["summary"],"source":type_view["source"],"line":type_view["line"]});
            }
            profiles.push(definition);
        }
        let mut seen = HashSet::new();
        relationships.retain(|edge| {
            seen.insert(format!("{}:{}:{}", edge["from"], edge["kind"], edge["to"]))
        });
        Ok(
            json!({"source":"compilerDeclarations","sourceSnapshotHash":self.source_snapshot_hash,
            "declarationVerified":true,"execution":"notRequested","resolutionStatus":if unresolved.is_empty(){"complete"}else{"partial"},"definitions":profiles,"relationships":relationships,"unresolved":unresolved,
            "scope":"已确定的声明、别名、输入/返回和直接继承关系；不执行、不验证当前运行环境。未解析的部分单独列出，不影响其他已确定的源码事实。"}),
        )
    }

    fn member_view(&self, member: &Value) -> Value {
        let mut value = member.clone();
        if let Some(return_type) = member["resolvedReturnType"]
            .as_str()
            .or_else(|| member["returnType"].as_str())
        {
            let definition = self.script_types.get(return_type);
            value["returns"] = json!({"declaredType":member["returnType"],"resolvedType":return_type,
                "definitionId":definition.map(|_|format!("T:{return_type}")),
                "declaredBases":definition.map(|value|&value["bases"]),"runtimeVerified":false});
        }
        if let Some(fields) = value.as_object_mut() {
            fields.remove("returnType");
            fields.remove("resolvedReturnType");
        }
        value
    }

    fn type_view(&self, name: &str, offset: usize, limit: usize) -> Value {
        let Some(definition) = self.script_types.get(name) else {
            return Value::Null;
        };
        let members = definition["members"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut value = definition.clone();
        let page = members
            .iter()
            .skip(offset)
            .take(limit.clamp(1, 30))
            .map(|entry| {
                let mut member = self.member_view(entry);
                let implementation_available = member
                    .get("implementation")
                    .is_some_and(|value| !value.is_null());
                member.as_object_mut().unwrap().remove("implementation");
                member["implementationAvailable"] = json!(implementation_available);
                member
            })
            .collect::<Vec<_>>();
        let next = offset.saturating_add(page.len());
        value["members"] = json!(page);
        value["totalMembers"] = json!(members.len());
        value["memberOffset"] = json!(offset);
        value["nextMemberOffset"] = json!((next < members.len()).then_some(next));
        value["definitionId"] = json!(format!("T:{name}"));
        value["runtimeVerified"] = json!(false);
        value
    }

    pub fn search(
        &self,
        query: &str,
        kind: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        if query.chars().count() > 200 {
            return Err(Error::Tool(
                "query 最多 200 字符；留空按页浏览真实能力目录".into(),
            ));
        }
        if let Some(index) = self.ids.get(query.trim()) {
            let item = &self.items[*index];
            if kind.is_none_or(|kind| item["kind"] == kind) {
                let items = if offset == 0 {
                    vec![
                        json!({"id":item["id"],"kind":item["kind"],"title":item["title"],"summary":item["summary"].as_str().unwrap_or_default().chars().take(240).collect::<String>(),"declarationKind":item["declaration"]["kind"],"availability":item["availability"]}),
                    ]
                } else {
                    vec![]
                };
                return Ok(
                    json!({"source":"bgiPluginFeatureIndex","query":query,"total":1,"offset":offset,"nextOffset":null,"items":items}),
                );
            }
        }
        let needle = normalize(query);
        let tokens = terms(query);
        let mut matches = self
            .items
            .iter()
            .zip(&self.search_documents)
            .filter(|(item, _)| kind.is_none_or(|kind| item["kind"] == kind))
            .filter_map(|(item, document)| {
                let id = item["id"].as_str().unwrap_or_default();
                if query.trim().is_empty() {
                    return Some((1.0, id, item));
                }
                let exact = normalize(id) == needle
                    || normalize(item["title"].as_str().unwrap_or_default()) == needle;
                let mut score = if id.eq_ignore_ascii_case(query.trim()) {
                    20_000.0
                } else if exact {
                    10_000.0
                } else {
                    0.0
                };
                for term in tokens.intersection(&document.all) {
                    let weight = if document.id.contains(term) {
                        3.0
                    } else if document.title.contains(term) {
                        2.0
                    } else {
                        1.0
                    };
                    score += self
                        .inverse_frequency
                        .get(term)
                        .copied()
                        .unwrap_or_default()
                        * weight;
                }
                (score > 0.0).then_some((score, id, item))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        let limit = limit.clamp(1, 12);
        let items = matches.iter().skip(offset).take(limit).map(|(_,_,item)| json!({
            "id":item["id"], "kind":item["kind"], "title":item["title"].as_str().unwrap_or("").chars().take(140).collect::<String>(),
            "summary":item["summary"].as_str().unwrap_or_default().chars().take(240).collect::<String>(),"declarationKind":item["declaration"]["kind"],"availability":item["availability"], "next":"bgi.feature.read 读取这一项声明"})).collect::<Vec<_>>();
        Ok(
            json!({"source":"bgiPluginFeatureIndex","query":query,"total":matches.len(),"offset":offset,
            "nextOffset": (offset + items.len() < matches.len()).then_some(offset + items.len()),"items":items,
            "next":"使用精确 ID 读取最相关条目；只展开本次需要的声明，不加载整份索引。"}),
        )
    }
}

/// Read one progressive card's current contracts; never create UI context or
/// execute the functions being described. A connection failure stays unknown.
pub(crate) async fn inspect_live(
    mut card: Value,
    offset: usize,
    bridge: &crate::runtime::host::bridge::Bridge,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<Value> {
    let methods = card["item"]["methodIds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    if methods.is_empty() {
        return Ok(card);
    }
    let total = methods.len();
    let mut contracts = Vec::new();
    let mut identity = None;
    let mut consistent = true;
    for id in methods.iter().skip(offset).take(4) {
        match bridge.describe(id, cancel).await {
            Ok(contract) => {
                let key = (
                    contract["instanceId"].clone(),
                    contract["catalogVersion"].clone(),
                );
                if key.0.is_null()
                    || key.1.is_null()
                    || identity.as_ref().is_some_and(|v| v != &key)
                {
                    consistent = false;
                }
                identity = Some(key);
                contracts.push(json!({"methodId":id,"state":if contract["callable"]==true {"executable"} else {"unavailable"},
                    "route":if contract["effect"]=="readOnly" {"bgi.api.read"} else {"bgi.api.invoke"},
                    "priority":if id.starts_with("cmd.") || id.starts_with("bgi.ui.") {"uiFallback"} else {"direct"},"contract":contract}));
            }
            Err(crate::error::Error::Cancelled) => return Err(crate::error::Error::Cancelled),
            Err(error) => {
                card["capabilityState"] = json!("unknown");
                card["unavailableReason"] = json!(error.user_message());
                card["contracts"] = json!(contracts);
                return Ok(card);
            }
        }
    }
    let next = (offset + contracts.len() < total).then_some(offset + contracts.len());
    card["capabilityState"] = json!(if !consistent || contracts.is_empty() || next.is_some() {
        "unknown"
    } else if contracts.iter().all(|c| c["state"] == "executable") {
        "executable"
    } else {
        "unavailable"
    });
    card["contracts"] = json!(contracts);
    card["snapshotConsistent"] = json!(consistent);
    card["methodTotal"] = json!(total);
    card["nextMethodOffset"] = json!(next);
    if card["item"]["kind"] == "scriptApi" {
        card["capabilityState"] = json!("referenceOnly");
        card["usage"] = json!(
            "编写脚本的 API 参考；contracts 是读取 JS 绑定说明的入口，不能把 JS 成员直接当桥操作调用。"
        );
    }
    card["next"] = json!(
        "以逐项 contract 的参数、前置条件和核验范围为准；优先 direct，UI 只作末级回退。executable 不是已执行，也不是用户授权。"
    );
    Ok(card)
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
    for word in value.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if word.len() < 2 {
            continue;
        }
        result.insert(word.to_ascii_lowercase());
        let mut start = 0;
        for (at, byte) in word.bytes().enumerate() {
            let camel_boundary =
                at > 0 && byte.is_ascii_uppercase() && word.as_bytes()[at - 1].is_ascii_lowercase();
            if byte == b'_' || camel_boundary {
                if at - start >= 2 {
                    result.insert(word[start..at].to_ascii_lowercase());
                }
                start = if byte == b'_' { at + 1 } else { at };
            }
        }
        if word.len() - start >= 2 {
            result.insert(word[start..].to_ascii_lowercase());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_source_features_have_progressive_cards() {
        let index = FeatureIndex::bundled().unwrap();
        let audit: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/fixtures/host-features.json"
        ))
        .unwrap();
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
            assert!(card["item"]["source"]["path"].is_string());
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
    fn factual_catalog_search_and_browse_have_no_scenario_overlay() {
        let index = FeatureIndex::bundled().unwrap();
        for item in &index.items {
            assert_ne!(item["kind"], "workflow");
            assert!(!item["id"].as_str().unwrap().starts_with("workflow."));
            for key in [
                "nouns",
                "actions",
                "openVocabulary",
                "steps",
                "branches",
                "routingPriority",
            ] {
                assert!(item.get(key).is_none(), "{}: {key}", item["id"]);
            }
            let id = item["id"].as_str().unwrap();
            let found = index.search(id, None, 0, 1).unwrap();
            assert_eq!(
                found["items"][0]["id"], id,
                "Exact declaration lookup: {id}"
            );
            assert!(found["items"][0].get("declaration").is_none());
        }
        assert!(index.read("untrusted-guessed-id").is_err());
        let first = index.search("", None, 0, 12).unwrap();
        let second = index.search("", None, 12, 12).unwrap();
        assert_eq!(first["total"], index.items.len());
        assert_eq!(first["nextOffset"], 12);
        assert_eq!(second["items"].as_array().unwrap().len(), 12);
        assert!(first["items"].as_array().unwrap().iter().all(|item| {
            !second["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|other| other["id"] == item["id"])
        }));
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

    /// 生产工具目录：全部契约来自 BgiClient::register_tools 的实际登记，不手写镜像。
    fn production_catalog() -> (
        crate::extension::ToolRegistry,
        crate::runtime::operation::task::ToolCatalog,
    ) {
        use crate::runtime::operation::task::{ToolCatalog, ToolContract};
        let mut registry = crate::extension::ToolRegistry::default();
        let client = crate::bridge::BgiClient::new(crate::config::BridgeConfig {
            enabled: false,
            base_url: "http://127.0.0.1:0".into(),
            token: None,
            instance_id: None,
            timeout_ms: 1000,
            host_install_path: None,
            auto_start: true,
            launch_silently: false,
        });
        crate::bridge::register_tools(&mut registry, std::sync::Arc::new(client)).unwrap();
        let mut catalog = ToolCatalog::new();
        for definition in registry.definitions() {
            catalog.insert(
                &definition.name,
                ToolContract {
                    execution: definition.execution.clone(),
                    provider_version: definition.provider_version.clone(),
                    input_schema: definition.input_schema.clone(),
                },
            );
        }
        (registry, catalog)
    }

    #[test]
    fn embedded_create_shortcut_is_registered_with_real_registry() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut skills = crate::extension::skills::SkillRegistry::default();
        skills
            .load(&[(
                root.join("plugins/bgi/skills"),
                crate::extension::skills::SkillSource::Plugin("bgi".into()),
            )])
            .unwrap();
        for text in crate::extension::skills::EMBEDDED_SKILLS {
            skills.load_embedded(text).unwrap();
        }
        // create-shortcut 不是常驻 Skill，预算约束只适用常驻头；这里只验证它
        // 连同插件入口一起真实装载、可检索。
        let shortcut = skills.get("create-shortcut").unwrap();
        assert!(!shortcut.body.trim().is_empty());
        assert!(
            !shortcut.body.contains("bgi."),
            "Domain APIs belong to the BGI plugin, not the generic shortcut skill"
        );
        assert!(!skills.search("创建快捷任务", 4).is_empty());
    }

    #[test]
    fn shortcut_binding_uses_current_contract_without_shipped_api_examples() {
        use crate::runtime::operation::shortcuts::{ShortcutBinding, nodes};
        let (registry, catalog) = production_catalog();
        let descriptors: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/bgi-method-metadata.json"
        ))
        .unwrap();
        let (id, descriptor) = descriptors
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, d)| {
                d["readOnly"] == true
                    && d["inputSchema"]["required"]
                        .as_array()
                        .is_none_or(|r| r.is_empty())
            })
            .unwrap();
        let tool = registry
            .definitions()
            .into_iter()
            .find(|definition| {
                definition.name.ends_with(".api.read")
                    && definition.execution.effect == crate::extension::ToolEffect::ReadOnly
            })
            .unwrap();
        assert!(crate::extension::validate(&json!({}), &descriptor["inputSchema"], "$").is_empty());
        let binding: ShortcutBinding = serde_json::from_value(
            json!({"applicationName":"BetterGI","targetName":"本次发现的只读能力",
            "action":{"tool":tool.name,"arguments":{"methodId":id,"arguments":{}}}}),
        )
        .unwrap();
        assert!(!nodes(&binding, &catalog).unwrap().is_empty());
    }

    #[test]
    fn plugin_skills_do_not_pin_native_interfaces_or_engine_symbols() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut skills = crate::extension::skills::SkillRegistry::default();
        skills
            .load(&[(
                root.join("plugins/bgi/skills"),
                crate::extension::skills::SkillSource::Plugin("bgi".into()),
            )])
            .unwrap();
        let descriptors: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/bgi-method-metadata.json"
        ))
        .unwrap();
        let bindings: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/managed/generated/host-documentation.json"
        ))
        .unwrap();
        for skill in skills.list() {
            let mut bodies = vec![skill.body.clone()];
            for file in std::fs::read_dir(skill.path.parent().unwrap().join("references")).unwrap()
            {
                let path = file.unwrap().path();
                if path.extension().is_some_and(|extension| extension == "md") {
                    bodies.push(std::fs::read_to_string(path).unwrap());
                }
            }
            for body in bodies {
                for id in descriptors.as_object().unwrap().keys() {
                    assert!(!body.contains(id), "{} pins {id}", skill.name);
                }
                for binding in bindings["scriptBindings"].as_array().unwrap() {
                    let alias = binding["alias"].as_str().unwrap();
                    for (start, _) in body.match_indices(alias) {
                        let end = start + alias.len();
                        let adjacent = |value: Option<char>| {
                            value.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
                        };
                        let part_of_word = adjacent(body[..start].chars().next_back())
                            || adjacent(body[end..].chars().next());
                        let path_suffix = body[end..]
                            .chars()
                            .take_while(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '/')
                            })
                            .collect::<String>();
                        if !part_of_word && !path_suffix.ends_with(".md") {
                            panic!(
                                "{} pins an engine binding instead of discovering it: {alias}",
                                skill.name
                            );
                        }
                    }
                }
                for prefix in ["cmd.", "setting."] {
                    assert!(
                        !body.contains(prefix),
                        "{} pins a native identifier",
                        skill.name
                    );
                }
            }
        }
    }

    #[test]
    fn behavioral_fixtures_keep_raw_requests_and_outcomes_without_retrieval_answers() {
        let scenarios: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/fixtures/user-scenarios.json"
        ))
        .unwrap();
        let domains = scenarios["domains"].as_array().unwrap();
        let mut ids = HashSet::new();
        for case in scenarios["scenarios"].as_array().unwrap() {
            assert!(ids.insert(case["id"].as_str().unwrap()));
            assert!(domains.contains(&case["domain"]));
            assert!(!case["utterance"].as_str().unwrap().is_empty());
            assert!(!case["expectedOutcome"].as_str().unwrap().is_empty());
            assert!(!case["sourceAnchors"].as_array().unwrap().is_empty());
            for key in ["expectedWorkflow", "normalizedQuery", "discoverySurface"] {
                assert!(
                    case.get(key).is_none(),
                    "Fixture must not provide retrieval answers: {key}"
                );
            }
        }
    }

    #[test]
    fn structured_symbols_follow_new_names_types_and_parameter_shapes() {
        for (owner, member, alias, returned, base, parameter) in [
            (
                "First.Source",
                "Acquire",
                "capture",
                "First.Frame",
                "First.Area",
                "scope",
            ),
            (
                "Refactored.Provider",
                "ReadNow",
                "takeImage",
                "Refactored.Picture",
                "Refactored.Surface",
                "region",
            ),
        ] {
            let data = json!({"format":2,"sourceRevision":"fixture","sourceSnapshotHash":"snapshot",
            "scriptTypes":{
                owner:{"name":owner,"kind":"type","bases":[],"members":[{"name":member,"kind":"method","parameters":[{"name":parameter,"type":"int"}],"returnType":returned,"resolvedReturnType":returned}]},
                returned:{"name":returned,"kind":"type","bases":[base],"members":[{"name":"A","kind":"property","returnType":"int"},{"name":"B","kind":"property","returnType":"string"}]}
            },"items":[
                {"id":format!("js.binding.{alias}"),"kind":"scriptApi","title":alias,"summary":"source binding","availability":"sourceDeclaration","inputSources":[],"references":[],"source":{"path":"fixture.cs"},"declaration":{"alias":alias,"kind":"function","clrType":owner,"member":member}},
                {"id":format!("T:{returned}"),"kind":"scriptApi","title":returned,"summary":"source type","availability":"sourceDeclaration","inputSources":[],"references":[],"source":{"path":"fixture.cs"},"declaration":{"kind":"type"}}
            ]});
            let index = FeatureIndex::parse(&data.to_string()).unwrap();
            let binding = index.read(&format!("js.binding.{alias}")).unwrap();
            assert_eq!(
                binding["symbol"]["overloads"][0]["parameters"][0]["name"],
                parameter
            );
            assert_eq!(
                binding["symbol"]["overloads"][0]["returns"]["resolvedType"],
                returned
            );
            assert_eq!(
                binding["symbol"]["overloads"][0]["returns"]["declaredBases"][0],
                base
            );
            let inspection = index
                .inspect_symbols(&[alias.into(), returned.into()])
                .unwrap();
            assert_eq!(inspection["resolutionStatus"], "complete");
            assert_eq!(inspection["definitions"].as_array().unwrap().len(), 2);
            assert!(inspection["relationships"].as_array().unwrap().iter().any(
                |edge| edge["kind"] == "returns" && edge["from"] == alias && edge["to"] == returned
            ));
            assert!(inspection["relationships"].as_array().unwrap().iter().any(
                |edge| edge["kind"] == "inherits" && edge["from"] == returned && edge["to"] == base
            ));
            let missing = index.inspect_symbols(&["missing-symbol".into()]).unwrap();
            assert_eq!(missing["resolutionStatus"], "partial");
            assert_eq!(missing["unresolved"][0]["status"], "notFound");
            let first = index.read_page(&format!("T:{returned}"), 0, 1).unwrap();
            let second = index.read_page(&format!("T:{returned}"), 1, 1).unwrap();
            assert_eq!(first["symbol"]["type"]["nextMemberOffset"], 1);
            assert_eq!(first["symbol"]["type"]["members"][0]["name"], "A");
            assert_eq!(second["symbol"]["type"]["members"][0]["name"], "B");
            assert!(second["symbol"]["type"]["nextMemberOffset"].is_null());
        }
    }

    #[test]
    fn registered_contract_descriptions_are_searchable_as_real_metadata() {
        let index = FeatureIndex::bundled().unwrap();
        let descriptors: Value = serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/bgi-method-metadata.json"
        ))
        .unwrap();
        for (id, descriptor) in descriptors.as_object().unwrap() {
            let card = index.read(id).unwrap();
            assert_eq!(card["item"]["title"], descriptor["guide"]["title"]);
            assert_eq!(card["item"]["summary"], descriptor["guide"]["purpose"]);
            assert_eq!(card["item"]["effect"], descriptor["effect"]);
            let found = index
                .search(
                    descriptor["guide"]["title"].as_str().unwrap(),
                    Some("stable"),
                    0,
                    12,
                )
                .unwrap();
            assert!(
                found["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["id"] == *id)
            );
        }
    }

    #[test]
    fn lexical_terms_preserve_word_boundaries_camel_case_and_chinese() {
        let tokens = terms("ReadImageRegion script_config OCR 队伍 切换");
        for token in [
            "readimageregion",
            "read",
            "image",
            "region",
            "script_config",
            "script",
            "config",
            "ocr",
            "队伍",
            "切换",
        ] {
            assert!(tokens.contains(token), "{token}");
        }
        assert!(!tokens.contains("伍切"));
    }
}
