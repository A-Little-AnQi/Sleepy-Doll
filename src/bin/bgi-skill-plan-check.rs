//! 技能规划隔离验收 CLI。默认只做 fixture 自检（不触网、不触模型、不写真实资源）；
//! 显式 `--live-model <配置文件路径>` 才以只读方式解析该配置并调用其 activeModel。
//!
//! 模型可见的工具契约来自生产 `BgiClient::register_tools` 的登记（只取元数据，
//! 绝不调用任何 handler）；core 的 user.ask / shortcut.save 元数据在无法直接
//! 复用生产纯定义处如实标注。用户资源、宿主接口全部由本进程内存模拟器承担：
//! 写入是完整替换（先决条件：先做过全文 read 且 SHA 一致），绑定保存走生产
//! `nodes` 编译，问答走生产 `questions::normalize`。任何分发错误都记入失败，
//! 断言不过就是失败，不折叠成通过。
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sleepy_doll::bridge::features::FeatureIndex;
use sleepy_doll::bridge::{BgiClient, register_tools};
use sleepy_doll::config::{BridgeConfig, ModelConfig, expand_env};
use sleepy_doll::error::{Error, Result};
use sleepy_doll::extension::skills::{EMBEDDED_SKILLS, SkillRegistry, SkillSource};
use sleepy_doll::extension::{ToolDefinition, ToolEffect, ToolExecution, ToolRegistry, validate};
use sleepy_doll::model::{Message, Role, ToolCall};
use sleepy_doll::runtime::gateway;
use sleepy_doll::runtime::operation::shortcuts::{
    ShortcutBinding, nodes, schema as shortcut_schema,
};
use sleepy_doll::runtime::operation::task::{ToolCatalog, ToolContract};
use sleepy_doll::runtime::questions;
use sleepy_doll::runtime::{CORE_AGENT_POLICY, configured_agent_instructions};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// 每个场景的轮数与时限上限；超限即失败，不无限重试。
const MAX_ROUNDS: usize = 8;
const CASE_TIMEOUT_SECS: u64 = 180;

type Sim = Arc<Mutex<Simulator>>;

/// 由 bgi-bridge/dev/export-method-metadata.ps1 从真实 BgiBridge.dll 导出的
/// 纯方法元数据（InputSchema + AgentGuide）；仅描述用，绝不调用 handler。
fn method_metadata() -> &'static Value {
    static METADATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    METADATA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../bgi-bridge/dev/bgi-method-metadata.json"
        ))
        .expect("bgi-method-metadata.json 必须可解析")
    })
}

// ---------------------------------------------------------------- 合成状态

struct Simulator {
    /// 相对 User 路径 → 完整 JSON 文档（即文件内容本身）。
    files: BTreeMap<String, Value>,
    /// 路径 → 最近一次全文 read 时的完整 SHA；写入的先决条件。
    full_reads: BTreeMap<String, String>,
    calls: Vec<Value>,
    failures: Vec<String>,
    prepared_groups: Vec<String>,
    described: Vec<String>,
    saved_shortcuts: Vec<Value>,
    /// 每次 shortcut.save 的事件记录（含 upsert 语义）。
    save_events: Vec<Value>,
    ask_calls: usize,
}

/// 配置组采用真实结构：参数在 config.pathingConfig（enabled/partyName 等），
/// projects.folderName 相对 AutoPathing。
fn pathing_group(name: &str, index: u64, party: &str, auto_fight: &str) -> Value {
    json!({
        "name": name,
        "index": index,
        "version": 3,
        "createdBy": "user",
        "unknownNote": "preserve-me",
        "projects": [{"type":"Pathing","status":"Enabled","folderName":"矿点",
            "name":"水晶矿-01.json","routeInfo":{"positions":[[1204.5,738.2],[998.1,451.0]]}}],
        "config": {"pathingConfig": {"enabled": true, "partyName": party,
            "autoFightConfig": {"name": auto_fight, "enabled": true}, "collectTimeout": 60}},
    })
}

/// prepare 接口契约使用的 path 形态（相对仓库根的 pathing/ 前缀）。
const MINE_PATH: &str = "pathing/矿点";

fn route_names() -> Vec<String> {
    (1..=7)
        .map(|i| format!("水晶矿-{i:02}.json"))
        .chain((1..=6).map(|i| format!("魔晶矿-{i:02}.json")))
        .collect()
}

fn initial_state() -> Simulator {
    let mut files = BTreeMap::new();
    files.insert(
        "ScriptGroup/挖矿讨伐.json".into(),
        pathing_group("挖矿讨伐", 1, "钟离采矿队", "采矿通用"),
    );
    files.insert(
        "ScriptGroup/兽怪暴徒.json".into(),
        pathing_group("兽怪暴徒", 2, "雷九万班", "讨伐通用"),
    );
    for route in route_names() {
        files.insert(
            format!("AutoPathing/矿点/{route}"),
            json!({"name":route.trim_end_matches(".json"),"positions":[[1204.5,738.2]]}),
        );
    }
    Simulator {
        files,
        full_reads: BTreeMap::new(),
        calls: vec![],
        failures: vec![],
        prepared_groups: vec![],
        described: vec![],
        saved_shortcuts: vec![],
        save_events: vec![],
        ask_calls: 0,
    }
}

fn serialize_doc(doc: &Value) -> String {
    serde_json::to_string(doc).unwrap_or_default()
}

fn doc_sha(doc: &Value) -> String {
    format!("{:x}", Sha256::digest(serialize_doc(doc).as_bytes()))
}

// ---------------------------------------------------------------- 模拟器

fn sim_user_list(state: &Simulator, arguments: &Value) -> Result<Value> {
    let path = arguments["path"].as_str().unwrap_or("").trim_matches('/');
    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    // 与真实桥一致：目录不存在直接报错，不用空结果伪装"确认过没有"。
    let dir_exists = path.is_empty()
        || state
            .files
            .keys()
            .any(|relative| relative.starts_with(&prefix));
    if !dir_exists {
        return Err(Error::Tool(format!("目录不存在：{path}")));
    }
    let keys: Vec<String> = arguments["jsonKeys"]
        .as_array()
        .map(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut entries = Vec::new();
    let mut seen_dirs = std::collections::BTreeSet::new();
    for relative in state.files.keys() {
        let Some(rest) = relative
            .strip_prefix(&prefix)
            .filter(|rest| !rest.is_empty())
        else {
            continue;
        };
        let top = rest.split('/').next().unwrap_or(rest);
        let child = format!("{prefix}{top}");
        if rest.contains('/') {
            if seen_dirs.insert(child.clone()) {
                entries.push(json!({"name":top,"relativePath":child,"directory":true}));
            }
            continue;
        }
        let doc = &state.files[relative];
        let mut item = json!({
            "name": top,
            "relativePath": child,
            "directory": false,
            "bytes": serialize_doc(doc).len(),
        });
        if !keys.is_empty() {
            let projected: serde_json::Map<String, Value> = keys
                .iter()
                .filter_map(|key| doc.get(key).map(|value| (key.clone(), value.clone())))
                .collect();
            item["data"] = Value::Object(projected);
        }
        entries.push(item);
    }
    Ok(json!({"path":path,"count":entries.len(),"entries":entries}))
}

/// 全文 read 返回完整 SHA 并登记先决条件；keys 投影同样返回完整 SHA
/// （与生产一致），但只有全文 read 才满足写入先决条件。
fn sim_user_read(state: &mut Simulator, arguments: &Value) -> Result<Value> {
    let path = arguments["path"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/");
    let Some(doc) = state.files.get(&path) else {
        return Err(Error::Tool(format!("文件不存在：{path}")));
    };
    let keys: Vec<String> = arguments["keys"]
        .as_array()
        .map(|keys| {
            keys.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if keys.is_empty() {
        state.full_reads.insert(path.clone(), doc_sha(doc));
        return Ok(json!({"path":path,"sha256":doc_sha(doc),"text":serialize_doc(doc)}));
    }
    let Value::Object(map) = doc else {
        return Err(Error::Tool("目标不是 JSON 对象，无法按名投影".into()));
    };
    let projected: serde_json::Map<String, Value> = keys
        .iter()
        .filter_map(|key| map.get(key).map(|value| (key.clone(), value.clone())))
        .collect();
    let dropped: Vec<String> = map
        .keys()
        .filter(|key| !keys.contains(key))
        .cloned()
        .collect();
    Ok(json!({
        "path":path,
        "sha256":doc_sha(doc),
        "droppedKeys":dropped,
        "text":Value::Object(projected).to_string(),
    }))
}

/// 真实完整替换：不自动合并任何字段。先决条件是此前做过同路径全文 read、
/// 且提交的 SHA 与当前一致；漏字段由断言抓住，模拟器不替模型补救。
fn sim_user_write(state: &mut Simulator, call: &ToolCall) -> Result<Value> {
    let path = call.arguments["path"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/");
    if !path.ends_with(".json") || path == "config.json" {
        return Err(Error::Tool("只接受 User 下的 JSON 资源文件".into()));
    }
    let content: Value = serde_json::from_str(call.arguments["content"].as_str().unwrap_or(""))
        .map_err(|e| Error::Tool(format!("写入内容不是 JSON：{e}")))?;
    let expected = call.arguments["expectedSha256"]
        .as_str()
        .unwrap_or_default();
    let created = !state.files.contains_key(&path);
    if created {
        if !expected.is_empty() {
            return Err(Error::Tool("目标不存在，不应携带 expectedSha256".into()));
        }
    } else {
        let current = doc_sha(&state.files[&path]);
        if expected != current {
            return Err(Error::Tool("expectedSha256 与当前完整文件不一致".into()));
        }
        if state.full_reads.get(&path).map(String::as_str) != Some(current.as_str()) {
            return Err(Error::Tool(
                "写入前必须先全文读取该文件（user.read 不带 keys）".into(),
            ));
        }
    }
    state.files.insert(path.clone(), content);
    Ok(json!({"path":path,"created":created,"sha256":doc_sha(&state.files[&path]),"verified":true}))
}

fn sim_user_resolve(state: &Simulator, arguments: &Value) -> Result<Value> {
    let query = arguments["query"].as_str().unwrap_or_default();
    for relative in state.files.keys() {
        let Some(name) = state.files[relative]["name"].as_str() else {
            continue;
        };
        if query.contains(name) {
            let doc = &state.files[relative];
            let projects: Vec<Value> = doc["projects"]
                .as_array()
                .map(|projects| {
                    projects
                        .iter()
                        .map(|project| {
                            json!({
                                "name":project["name"],"type":project["type"],
                                "folderName":project["folderName"],"exists":true,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            return Ok(json!({
                "query":query,"verdict":"run","groupName":name,
                "missing":[],
                "candidates":[],
                "groups":[{"name":name,"file":relative,"missing":[],"config":Value::Null,"projects":projects}],
                "lookupScope":"local",
                "next":"组引用完整、可直接运行；但先比对用户目标与返回的 config 槽位：目标还包含队伍、赶路角色或参数变更时，先用 bgi.set_pathing_party 等原生配置入口写入并回读核验，再运行；仅当目标与组当前配置一致才直接运行。不重复搜索或读取路线叶子文件。",
            }));
        }
    }
    if ["矿点", "路线"].iter().any(|word| query.contains(word)) {
        return Ok(json!({
            "query":query,"verdict":"create","groupName":Value::Null,
            "missing":[],
            "candidates":[{"path":MINE_PATH,"children":route_names().len(),"kind":"Pathing"}],
            "groups":[],
            "lookupScope":"local",
            "next":"本机已有资源；Pathing 用完整父目录准备配置组，不读取叶子 JSON；用 bgi.prepare_pathing_group 建组后按用户目标配置队伍，游戏就绪再 bgi.run_script_group。",
        }));
    }
    Ok(json!({
        "query":query,"verdict":"notFound","groupName":Value::Null,
        "missing":[],"candidates":[],"groups":[],
        "lookupScope":"local",
        "next":"仅本机未安装，不代表仓库不存在。采集/地图追踪用 bgi.repo.search category=pathing 查询完整父节点；已有索引先查询，不先反复刷新。",
    }))
}

/// 离线目录检索：仅覆盖已导出的方法元数据，返回与生产 catalog 相同的
/// 响应形状（items 为 Discovery 对象）。已知稳定接口 ID 的请求应直接
/// describe；把这里的空结果当成宿主缺接口属于规划错误。
fn sim_api_search(arguments: &Value) -> Result<Value> {
    let query = arguments["query"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    let group_filter = arguments["group"].as_str().unwrap_or_default();
    let offset = arguments["offset"].as_u64().unwrap_or(0) as usize;
    let limit = arguments["limit"].as_u64().unwrap_or(8).clamp(1, 50) as usize;
    let metadata = method_metadata();
    let matched: Vec<(String, &Value)> = metadata
        .as_object()
        .map(|methods| {
            methods
                .iter()
                .filter(|(id, contract)| {
                    if !group_filter.is_empty() && contract["group"] != group_filter {
                        return false;
                    }
                    if query.is_empty() {
                        return true;
                    }
                    let haystack = format!(
                        "{} {} {} {}",
                        id.to_lowercase(),
                        contract["summary"].as_str().unwrap_or_default(),
                        contract["guide"]["purpose"].as_str().unwrap_or_default(),
                        contract["guide"]["whenToUse"].to_string()
                    )
                    .to_lowercase();
                    haystack.contains(&query)
                })
                .map(|(id, contract)| (id.clone(), contract))
                .collect()
        })
        .unwrap_or_default();
    let items: Vec<Value> = matched
        .iter()
        .skip(offset)
        .take(limit)
        .map(|(id, contract)| {
            json!({
                "methodId": id,
                "displayName": contract["guide"]["title"],
                "group": contract["group"],
                "summary": contract["summary"],
                "whenToUse": contract["guide"]["whenToUse"],
                "effect": contract["effect"],
                "callable": true,
                "executionMode": if contract["readOnly"] == true { "inline" } else { "job" },
            })
        })
        .collect();
    let hint = (matched.is_empty() && !query.trim().is_empty()).then(|| {
        "没有接口命中。脚本名、脚本参数与脚本行为改用 bgi.repo.search/read 查询中央仓库；只有宿主设置或动作才在此用一个核心词重试一次。"
    });
    Ok(json!({
        "catalogVersion":"offline-export",
        "total": matched.len(),
        "offset": offset,
        "nextOffset": (offset + items.len() < matched.len()).then_some(offset + items.len()),
        "items": items,
        "hint": hint,
    }))
}

/// describe 是 invoke 的先决记录；契约文本来自真实导出的方法元数据。
fn sim_api_describe(state: &mut Simulator, arguments: &Value) -> Result<Value> {
    let method_id = arguments["methodId"].as_str().unwrap_or_default();
    let contract = method_metadata()
        .get(method_id)
        .ok_or_else(|| Error::Tool(format!("当前合成宿主没有接口：{method_id}")))?;
    if !state.described.iter().any(|id| id == method_id) {
        state.described.push(method_id.to_string());
    }
    Ok(json!({
        "methodId":method_id,
        "effect":contract["effect"],
        "callable":true,
        "inputSchema":contract["inputSchema"],
        "guide":contract["guide"],
    }))
}

/// 只接受本次已 describe 的契约；运行类接口一律拒绝——验收场景没有运行授权。
fn sim_api_invoke(state: &mut Simulator, call: &ToolCall) -> Result<Value> {
    let method_id = call.arguments["methodId"].as_str().unwrap_or_default();
    if !state.described.iter().any(|id| id == method_id) {
        return Err(Error::Tool(
            "必须先用 bgi.api.describe 读取该接口契约".into(),
        ));
    }
    let arguments = &call.arguments["arguments"];
    match method_id {
        "bgi.prepare_pathing_group" => {
            // 与 native 一致：pathing/ 前缀替换为 AutoPathing/（不追加），
            // 目录必须真实存在，否则报错——不掩盖前缀拼接错误。
            let source_path = arguments["path"].as_str().unwrap_or_default();
            let Some(relative) = source_path.strip_prefix("pathing/") else {
                return Err(Error::Tool("path 必须是 pathing/ 开始的目录路径".into()));
            };
            let user_dir = format!("AutoPathing/{relative}");
            if !state
                .files
                .keys()
                .any(|path| path.starts_with(&format!("{user_dir}/")))
            {
                return Err(Error::Tool(format!(
                    "所选路线目录未安装：{source_path}（对应 {user_dir} 不存在）"
                )));
            }
            let name = arguments["groupName"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("矿点采集组")
                .to_string();
            let group_path = format!("ScriptGroup/{name}.json");
            if state.files.contains_key(&group_path) {
                return Err(Error::Tool("同名配置组已存在，未覆盖".into()));
            }
            let projects: Vec<Value> = route_names()
                .into_iter()
                .enumerate()
                .map(|(index, route)| {
                    json!({"type":"Pathing","status":"Enabled","folderName":"矿点",
                        "name":route,"index":index+1,
                        "routeInfo":{"positions":[[1204.5,738.2]]}})
                })
                .collect();
            state.files.insert(
                group_path,
                json!({"name":name,"index":99,"version":3,"projects":projects,
                    "config":{"pathingConfig":{
                        "enabled":false,"partyName":"","autoFightConfig":{"name":"默认","enabled":false},
                        "collectTimeout":30,"unknownHostDefault":"keep"}}}),
            );
            state.prepared_groups.push(name);
            Ok(json!({"prepared":true,"routeCount":13,
                "note":"配置组已落盘，尚未运行；队伍等参数按用户目标写入组配置。"}))
        }
        "bgi.run_script_group" | "bgi.run_script_groups" => Err(Error::Tool(
            "模拟器未授权运行；验收场景不包含运行请求".into(),
        )),
        other => Err(Error::Tool(format!("未授权或未知的宿主调用：{other}"))),
    }
}

/// 绑定保存：结构校验与节点编译都用生产实现，只在内存记录。
fn sim_shortcut_save(
    state: &mut Simulator,
    call: &ToolCall,
    catalog: &ToolCatalog,
) -> Result<Value> {
    let binding: ShortcutBinding = serde_json::from_value(call.arguments["binding"].clone())
        .map_err(|e| Error::Tool(format!("binding 不符合结构：{e}")))?;
    let compiled = nodes(&binding, catalog)?;
    let saved = json!({
        "id": call.arguments["id"].as_str().unwrap_or_default(),
        "name": call.arguments["name"].as_str().unwrap_or(&binding.target_name),
        "description": call.arguments["description"].as_str().unwrap_or_default(),
        "binding": serde_json::to_value(&binding).unwrap_or(Value::Null),
    });
    // 与生产一致的 upsert：带既有 id 时原位更新，不产生第二份。
    let id = saved["id"].as_str().unwrap_or_default().to_string();
    let upsert = !id.is_empty()
        && state
            .saved_shortcuts
            .iter()
            .any(|entry| entry["id"].as_str() == Some(id.as_str()));
    if upsert {
        if let Some(slot) = state
            .saved_shortcuts
            .iter_mut()
            .find(|entry| entry["id"].as_str() == Some(id.as_str()))
        {
            *slot = saved.clone();
        }
    } else {
        state.saved_shortcuts.push(saved);
    }
    state.save_events.push(json!({"id":id,"upsert":upsert}));
    Ok(json!({"saved":true,"nodes":compiled.len(),"upsert":upsert}))
}

/// 生产 questions::normalize 归一；不替用户作答，回答前即结束本轮模拟。
fn sim_user_ask(state: &mut Simulator, call: &ToolCall) -> Result<Value> {
    let (questions, legacy) = questions::normalize(&call.arguments)?;
    state.ask_calls += questions.len().max(1);
    Ok(
        json!({"status":"asked","requestId":call.id,"questions":questions,"legacy":legacy,
        "note":"等待用户答复；不要重复提问。"}),
    )
}

fn dispatch(
    state: &Sim,
    call: &ToolCall,
    definitions: &[ToolDefinition],
    catalog: &ToolCatalog,
    skills: &SkillRegistry,
    index: &FeatureIndex,
) -> Result<Value> {
    let mut state = state.lock().unwrap();
    // 先按真实 ToolDefinition.input_schema 校验外层参数（与原
    // Registry.call 的 extension::validate 同一实现）；校验错误记入失败，
    // 未知参数不会被忽略。
    let schema = definitions
        .iter()
        .find(|definition| definition.name == call.name)
        .map(|definition| definition.input_schema.clone());
    let Some(schema) = schema else {
        let error = Error::Tool(format!("模拟器未提供该工具：{}", call.name));
        state.failures.push(error.to_string());
        state.calls.push(json!({
            "name":call.name,"arguments":call.arguments,
            "ok":false,"result":error.to_string(),
        }));
        return Err(error);
    };
    if !validate(&call.arguments, &schema, "$").is_empty() {
        let error = Error::Tool(format!("参数不符合 {} 的 input_schema", call.name));
        state.failures.push(error.to_string());
        state.calls.push(json!({
            "name":call.name,"arguments":call.arguments,
            "ok":false,"result":error.to_string(),
        }));
        return Err(error);
    }
    // 以下宿主接口契约是本进程合成的 mock，仅用于隔离验收，
    // 不代表真实原生运行的验证。
    let result = match call.name.as_str() {
        "bgi.user.list" => sim_user_list(&state, &call.arguments),
        "bgi.user.read" => sim_user_read(&mut state, &call.arguments),
        "bgi.user.write" => sim_user_write(&mut state, call),
        "bgi.user.resolve" => sim_user_resolve(&state, &call.arguments),
        "bgi.api.search" => sim_api_search(&call.arguments),
        "bgi.api.describe" => sim_api_describe(&mut state, &call.arguments),
        "bgi.api.invoke" => sim_api_invoke(&mut state, call),
        "shortcut.save" => sim_shortcut_save(&mut state, call, catalog),
        "user.ask" => sim_user_ask(&mut state, call),
        "bgi.feature.search" => index.search(
            call.arguments["query"].as_str().unwrap_or_default(),
            call.arguments["kind"].as_str(),
            call.arguments["offset"].as_u64().unwrap_or(0) as usize,
            call.arguments["limit"].as_u64().unwrap_or(5) as usize,
        ),
        "bgi.feature.read" => index.read(call.arguments["id"].as_str().unwrap_or_default()),
        "skills.read" => {
            let skill = skills
                .get(call.arguments["name"].as_str().unwrap_or_default())
                .ok_or_else(|| Error::Tool("skill not found".into()))?;
            Ok(json!({"name":skill.name,"description":skill.description,"instructions":skill.body}))
        }
        "skills.reference" => {
            let content = skills.read_reference(
                call.arguments["name"].as_str().unwrap_or_default(),
                call.arguments["path"].as_str().unwrap_or_default(),
            )?;
            Ok(json!({"content":content}))
        }
        other => Err(Error::Tool(format!("模拟器未提供该工具：{other}"))),
    };
    let summary = match &result {
        Ok(value) => value.to_string().chars().take(240).collect(),
        Err(error) => error.to_string(),
    };
    state.calls.push(json!({
        "name":call.name,"arguments":call.arguments,
        "ok":result.is_ok(),"result":summary,
    }));
    // 任何分发 Err 都是失败步骤，进入 failures，断言可见。
    if let Err(error) = &result {
        state.failures.push(format!("{}：{error}", call.name));
    }
    result
}

// ---------------------------------------------------------------- 工具清单

/// 模型工具 = 生产桥登记（按 name 唯一）+ 两个 core 元数据。
/// 名称重复直接报错，绝不静默覆盖。
fn all_definitions(bridge: &ToolRegistry) -> Result<Vec<ToolDefinition>> {
    let mut definitions = bridge.definitions();
    let read_only = ToolExecution::read_only();
    definitions.push(ToolDefinition {
        // schema 文字复制自 runtime definitions 中的 user.ask（runtime 未导出
        // 该纯定义；问答归一逻辑本身复用生产 questions::normalize）。
        name: "user.ask".into(),
        label: "询问用户".into(),
        description: "仅询问无法从本机文件、接口契约或状态取得，且不同答案会改变目标或不可逆结果的信息；没有确实缺少的必要信息就不要问。一次问完。".into(),
        input_schema: json!({
            "type":"object",
            "properties":{
                "questions":{"type":"array","minItems":1,"maxItems":3,"items":{"type":"object","required":["id","header","question"],"properties":{
                    "id":{"type":"string"},"header":{"type":"string"},"question":{"type":"string"},
                    "options":{"type":"array","maxItems":8,"items":{"type":"object","required":["label"],"properties":{"label":{"type":"string"},"description":{"type":"string"}}}}
                }}},
                "question":{"type":"string","description":"旧格式：优先用 questions"}
            },
            "additionalProperties":false
        }),
        output_schema: None,
        source: "core:runtime".into(),
        provider_version: None,
        execution: ToolExecution { effect: ToolEffect::InternalState, ..ToolExecution::default() },
    });
    definitions.push(ToolDefinition {
        // schema 复用生产 shortcuts::schema()；说明文字复制自 runtime 定义。
        name: "shortcut.save".into(),
        label: "添加快捷任务".into(),
        description: "用户明确要求加入快捷任务时保存组合入口。BetterGI 配置组组合优先用 binding.action 绑定一次批量调用：groupNames 现场读取真实名称，关闭游戏等收尾用 closeGameAfter；默认 waitForCompletion=false 后台交接。binding.action.arguments 按契约分层：外层 {methodId, arguments:{…}}，宿主参数只在内层；编辑已有入口只改内层参数值，不得扁平化或改变层级，保存时按契约校验。助手自拟入口名取 ≤20 字自然中文短名，用户给的名称照用。不得因动作数量拆分入口。".into(),
        input_schema: shortcut_schema(),
        output_schema: None,
        source: "core:runtime".into(),
        provider_version: None,
        execution: ToolExecution { effect: ToolEffect::LocalWrite, ..ToolExecution::default() },
    });
    definitions.extend([
        ToolDefinition {
            // 说明文字复制自 app 注册的 skills.read（同样未导出纯定义）。
            name: "skills.read".into(),
            label: "读取技能说明".into(),
            description: "读取已发现但未自动加载的 Skill。当前上下文已有完整说明时不要重复读取。".into(),
            input_schema: json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"],"additionalProperties":false}),
            output_schema: None,
            source: "core:skills".into(),
            provider_version: None,
            execution: read_only.clone(),
        },
        ToolDefinition {
            name: "skills.reference".into(),
            label: "读取技能资料".into(),
            description: "当已加载 Skill 明确引用同目录资料时读取该资料；不用于发现宿主内容。".into(),
            input_schema: json!({"type":"object","properties":{"name":{"type":"string"},"path":{"type":"string"}},"required":["name","path"],"additionalProperties":false}),
            output_schema: None,
            source: "core:skills".into(),
            provider_version: None,
            execution: read_only,
        },
    ]);
    let mut seen = std::collections::BTreeSet::new();
    for definition in &definitions {
        if !seen.insert(definition.name.clone()) {
            return Err(Error::Config(format!("工具名重复：{}", definition.name)));
        }
    }
    definitions.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));
    Ok(definitions)
}

fn combined_catalog(definitions: &[ToolDefinition]) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    for definition in definitions {
        catalog.insert(
            &definition.name,
            ToolContract {
                execution: definition.execution.clone(),
                provider_version: definition.provider_version.clone(),
                input_schema: definition.input_schema.clone(),
            },
        );
    }
    catalog
}

// ---------------------------------------------------------------- 场景与断言

struct Scenario {
    id: &'static str,
    /// 合成历史（上下文，不含答案）：先前对话中的助手陈述。
    history: &'static [&'static str],
    prompt: &'static str,
    seed_shortcut: bool,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            id: "1-改队不动采矿",
            history: &["已按顺序读取两个配置组：1. 挖矿讨伐（采矿）；2. 兽怪暴徒（讨伐）。"],
            prompt: "把后面那个配置组的队伍换成蒸发，采矿那个别动。",
            seed_shortcut: false,
        },
        Scenario {
            id: "2-未知目标先问",
            history: &[
                "当前在讨论兽怪暴徒配置组；挖矿讨伐组保持不动。已读取两个配置组：挖矿讨伐（采集）、兽怪暴徒（战斗）。",
            ],
            prompt: "打怪那队换个队。",
            seed_shortcut: false,
        },
        Scenario {
            id: "3-路线包装成组",
            history: &["已读取路线目录 AutoPathing/矿点，共 13 条路线，尚未建立配置组。"],
            prompt: "这路线给我包起来，用讨伐队。",
            seed_shortcut: false,
        },
        Scenario {
            id: "4-两组合一入口",
            history: &["已按顺序读取两个配置组：1. 挖矿讨伐；2. 兽怪暴徒。"],
            prompt: "把这两个配置组加一个快捷任务入口，跑完把游戏关掉。",
            seed_shortcut: false,
        },
        Scenario {
            id: "5-改为后台交接",
            history: &[
                "已保存快捷任务入口「挖矿讨伐一键」（id: sc-mining-wait），其完整绑定如下（本次读取到的当前保存内容）：\n{\"id\":\"sc-mining-wait\",\"name\":\"挖矿讨伐一键\",\"description\":\"运行挖矿讨伐配置组并等待完成\",\"binding\":{\"applicationName\":\"BetterGI\",\"targetName\":\"挖矿讨伐\",\"prepare\":[{\"tool\":\"bgi.api.describe\",\"arguments\":{\"methodId\":\"bgi.run_script_group\"}}],\"action\":{\"tool\":\"bgi.api.invoke\",\"arguments\":{\"methodId\":\"bgi.run_script_group\",\"arguments\":{\"groupName\":\"挖矿讨伐\",\"waitForCompletion\":true}}}}}\n用户此前确认过要等待运行完成；入口其余设置（目标、顺序、prepare 契约核对）都是用户认可的。",
            ],
            prompt: "以后后台跑就行，不用等我，把它改掉。",
            seed_shortcut: true,
        },
        Scenario {
            id: "6-只读不改",
            history: &[],
            prompt: "就看看现在挖矿讨伐用的是哪个队伍，先别改。",
            seed_shortcut: false,
        },
    ]
}

fn ok_calls<'a>(state: &'a Simulator, name: &str) -> Vec<&'a Value> {
    state
        .calls
        .iter()
        .filter(|call| call["name"] == name && call["ok"] == true)
        .collect()
}

fn write_args(state: &Simulator) -> Vec<&Value> {
    ok_calls(state, "bgi.user.write")
        .into_iter()
        .map(|call| &call["arguments"])
        .collect()
}

/// 按用户意图对模拟器终态断言；任何失败步骤都使场景失败。
fn assertions(
    scenario: &Scenario,
    state: &Simulator,
    rounds: &[Value],
) -> Vec<(String, bool, String)> {
    let mut result = vec![];
    let mut check = |name: &str, pass: bool, detail: String| {
        result.push((name.to_string(), pass, detail));
    };
    let writes = write_args(state);
    let saved = &state.saved_shortcuts;
    let invokes = ok_calls(state, "bgi.api.invoke");
    // 建组场景里 prepare_pathing_group 的 invoke 是合法准备动作；
    // 其余任何宿主调用（run/start_game 等）都算运行。
    let non_prepare_invokes: Vec<&Value> = invokes
        .iter()
        .copied()
        .filter(|call| {
            call["arguments"]["methodId"].as_str().unwrap_or_default()
                != "bgi.prepare_pathing_group"
        })
        .collect();
    let failures = state.failures.len();
    check(
        "无失败步骤",
        failures == 0,
        format!("{failures} 个：{:?}", state.failures),
    );
    let mut noisy_rounds = vec![];
    let mut final_text = String::new();
    for (at, round) in rounds.iter().enumerate() {
        let text = round["text"].as_str().unwrap_or_default();
        let has_calls = round["calls"]
            .as_array()
            .is_some_and(|calls| !calls.is_empty());
        if has_calls && !text.trim().is_empty() {
            noisy_rounds.push(at + 1);
        }
        if !has_calls {
            final_text = text.to_string();
        }
    }
    check(
        "工具轮静默无中途旁白",
        noisy_rounds.is_empty(),
        format!("带正文的工具轮 {noisy_rounds:?}"),
    );
    let had_ask = rounds.iter().any(|round| {
        round["calls"]
            .as_array()
            .is_some_and(|calls| calls.iter().any(|call| call["name"] == "user.ask"))
    });
    check(
        "收敛出最终答复",
        had_ask || !final_text.trim().is_empty(),
        format!("len={}", final_text.chars().count()),
    );
    let ask_echo = rounds.iter().any(|round| {
        let Some(calls) = round["calls"].as_array() else {
            return false;
        };
        let Some(call) = calls.iter().find(|call| call["name"] == "user.ask") else {
            return false;
        };
        let text = round["text"].as_str().unwrap_or_default();
        call["arguments"]["questions"]
            .as_array()
            .map(|questions| {
                questions
                    .iter()
                    .filter_map(|question| question["question"].as_str())
                    .any(|question| !question.is_empty() && text.contains(question))
            })
            .unwrap_or(false)
    });
    check(
        "user.ask 不在正文复读问题",
        !ask_echo,
        format!("{ask_echo}"),
    );
    match scenario.id {
        "1-改队不动采矿" => {
            check(
                "只写一次",
                writes.len() == 1,
                format!("{} 次", writes.len()),
            );
            let target = writes
                .first()
                .map(|a| a["path"].as_str().unwrap_or_default())
                .unwrap_or_default();
            check(
                "目标是兽怪暴徒组",
                target == "ScriptGroup/兽怪暴徒.json",
                format!("{target}"),
            );
            let written: Value = writes
                .first()
                .and_then(|a| serde_json::from_str(a["content"].as_str().unwrap_or("")).ok())
                .unwrap_or(Value::Null);
            // 完整等价：除允许的 partyName 外，整份文档必须逐字段一致。
            let expected = {
                let mut doc = pathing_group("兽怪暴徒", 2, "雷九万班", "讨伐通用");
                doc["config"]["pathingConfig"]["partyName"] = json!("蒸发");
                doc
            };
            check(
                "除 partyName 外整份文档完全一致",
                written == expected,
                format!(
                    "字段数 written={} expected={}",
                    written.as_object().map(|map| map.len()).unwrap_or(0),
                    expected.as_object().map(|map| map.len()).unwrap_or(0)
                ),
            );
            let seed = pathing_group("挖矿讨伐", 1, "钟离采矿队", "采矿通用");
            check(
                "挖矿组未被改动",
                state
                    .files
                    .get("ScriptGroup/挖矿讨伐.json")
                    .map(doc_sha)
                    .as_deref()
                    == Some(doc_sha(&seed).as_str()),
                "sha 比对".to_string(),
            );
            check(
                "无运行调用",
                invokes.is_empty(),
                format!("{} 次", invokes.len()),
            );
        }
        "2-未知目标先问" => {
            check(
                "恰好一次 user.ask",
                state.ask_calls == 1,
                format!("{} 次", state.ask_calls),
            );
            // 问之前只允许一次必要的 list/read 定位，不做 API/JS/全局穷举。
            let ask_at = state
                .calls
                .iter()
                .position(|call| call["name"] == "user.ask")
                .unwrap_or(state.calls.len());
            let pre: Vec<&str> = state.calls[..ask_at]
                .iter()
                .map(|call| call["name"].as_str().unwrap_or_default())
                .collect();
            let locating = pre
                .iter()
                .filter(|name| **name == "bgi.user.list" || **name == "bgi.user.read")
                .count();
            check(
                "ask 前只做一次定位读取",
                locating == pre.len() && locating <= 1,
                format!("ask 前 {pre:?}"),
            );
            check(
                "ask 前不写不存不调用",
                !pre.iter().any(|name| {
                    name.contains("write") || name.contains("save") || name.contains("invoke")
                }),
                format!("ask 前 {pre:?}"),
            );
            let asked = state
                .calls
                .iter()
                .find(|call| call["name"] == "user.ask")
                .map(|call| call["arguments"].clone())
                .unwrap_or(Value::Null);
            let questions = asked["questions"].as_array().cloned().unwrap_or_default();
            let about_party = questions
                .iter()
                .filter_map(|question| question["question"].as_str())
                .any(|question| question.contains('队'));
            let asking_group = questions
                .iter()
                .filter_map(|question| question["question"].as_str())
                .any(|question| {
                    question.contains("哪个组")
                        || question.contains("哪个配置组")
                        || question.contains("组名")
                });
            let free_fill = questions.iter().any(|question| {
                question["options"]
                    .as_array()
                    .map(Vec::is_empty)
                    .unwrap_or(true)
            });
            check("问的是目标队伍", about_party, format!("{questions:?}"));
            check(
                "不问组名（目标已在上下文明确）",
                !asking_group,
                format!("{questions:?}"),
            );
            check(
                "目标队伍可自由填写而非确认",
                free_fill,
                format!("{questions:?}"),
            );
            check(
                "不写配置不保存入口",
                writes.is_empty() && saved.is_empty(),
                format!("写 {} 存 {}", writes.len(), saved.len()),
            );
            check(
                "无运行调用",
                invokes.is_empty(),
                format!("{} 次", invokes.len()),
            );
        }
        "3-路线包装成组" => {
            check(
                "原生 prepare 建了一个组",
                state.prepared_groups.len() == 1,
                format!("{:?}", state.prepared_groups),
            );
            // 用户未给名称：生成名须是简短自然中文目标名，无目录/作者/版本串。
            let name = state.prepared_groups.first().cloned().unwrap_or_default();
            let name_chars: Vec<char> = name.chars().collect();
            let junk_punctuation = name
                .chars()
                .any(|c| "!@#$%^&*()+=[]{};:'\"\\|,.<>/?`~".contains(c));
            check(
                "组名自然短名无路径作者版本串",
                (2..=20).contains(&name_chars.len())
                    && name_chars
                        .iter()
                        .any(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
                    && !name.contains('/')
                    && !name.contains("pathing")
                    && !name.contains("AutoPathing")
                    && !name.contains('@')
                    && !name.contains("v1")
                    && !name.contains("20")
                    && !junk_punctuation,
                format!("{name}"),
            );
            let new_group = state
                .prepared_groups
                .first()
                .map(|name| format!("ScriptGroup/{name}.json"));
            check(
                "只写新组配置",
                writes.len() == 1
                    && writes.first().and_then(|a| a["path"].as_str()) == new_group.as_deref(),
                format!(
                    "写 {:?}",
                    writes
                        .iter()
                        .filter_map(|a| a["path"].as_str())
                        .collect::<Vec<_>>()
                ),
            );
            let written: Value = writes
                .first()
                .and_then(|a| serde_json::from_str(a["content"].as_str().unwrap_or("")).ok())
                .unwrap_or(Value::Null);
            let config = &written["config"]["pathingConfig"];
            check(
                "partyName=讨伐队 且 enabled=true",
                config["partyName"] == "讨伐队" && config["enabled"] == true,
                format!(
                    "party={} enabled={}",
                    config["partyName"], config["enabled"]
                ),
            );
            // 以 prepare 落盘的组为基准：除 partyName/enabled 外整份文档逐字段一致。
            let prepared_doc = new_group
                .as_deref()
                .and_then(|path| state.files.get(path))
                .cloned()
                .unwrap_or(Value::Null);
            let expected_written = {
                let mut doc = prepared_doc.clone();
                doc["config"]["pathingConfig"]["partyName"] = json!("讨伐队");
                doc["config"]["pathingConfig"]["enabled"] = json!(true);
                doc
            };
            check(
                "除 partyName/enabled 外与 prepare 产物完全一致",
                written == expected_written,
                format!(
                    "pathingConfig 字段数 {}（默认字段必须保留）",
                    config.as_object().map(|m| m.len()).unwrap_or(0)
                ),
            );
            // 13 条项目必须与 prepare 生成的项目在内容与顺序上完全一致。
            let expected_projects: Vec<Value> = route_names()
                .into_iter()
                .enumerate()
                .map(|(index, route)| {
                    json!({"type":"Pathing","status":"Enabled","folderName":"矿点",
                        "name":route,"index":index+1,
                        "routeInfo":{"positions":[[1204.5,738.2]]}})
                })
                .collect();
            check(
                "13 条项目内容与顺序完全一致",
                written["projects"].as_array() == Some(&expected_projects),
                format!(
                    "{} 条",
                    written["projects"].as_array().map(Vec::len).unwrap_or(0)
                ),
            );
            // 除新组外，全部既有文件（组与路线）与初始状态逐字段一致。
            let fresh = initial_state();
            let changed: Vec<String> = fresh
                .files
                .iter()
                .filter(|(path, doc)| {
                    state.files.get(path.as_str()).map(|have| doc_sha(have)) != Some(doc_sha(doc))
                })
                .map(|(path, _)| path.clone())
                .collect();
            check(
                "既有组与路线全部原样",
                changed.is_empty() && state.files.len() == fresh.files.len() + 1,
                format!("{changed:?}"),
            );
            check(
                "无新 JS",
                !state.files.keys().any(|path| path.starts_with("JsScript/")),
                "无写入".to_string(),
            );
            check(
                "只允许建组调用，无运行调用",
                non_prepare_invokes.is_empty(),
                format!("非 prepare 调用 {} 次", non_prepare_invokes.len()),
            );
        }
        "4-两组合一入口" => {
            check(
                "只保存一个入口",
                saved.len() == 1,
                format!("{} 次", saved.len()),
            );
            if let Some(entry) = saved.first() {
                let action = &entry["binding"]["action"];
                let args = &action["arguments"];
                let group_names: Vec<&str> = args["arguments"]["groupNames"]
                    .as_array()
                    .map(|names| names.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let entry_name = entry["name"].as_str().unwrap_or_default();
                let name_chars: Vec<char> = entry_name.chars().collect();
                let junk_punctuation = entry_name
                    .chars()
                    .any(|c| "!@#$%^&*()+=[]{};:'\"\\|,.<>/?`~".contains(c));
                check(
                    "入口名自然短名无路径作者版本串",
                    (2..=20).contains(&name_chars.len())
                        && name_chars
                            .iter()
                            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
                        && !entry_name.contains('/')
                        && !entry_name.contains("pathing")
                        && !entry_name.contains("AutoPathing")
                        && !entry_name.contains('@')
                        && !entry_name.contains("v1")
                        && !entry_name.contains("20")
                        && !junk_punctuation,
                    format!("{entry_name}"),
                );
                check(
                    "单动作 binding.action",
                    action.is_object(),
                    "binding.action".to_string(),
                );
                check(
                    "canonical 工具名",
                    action["tool"] == "bgi.api.invoke",
                    format!("{}", action["tool"]),
                );
                check(
                    "批量 methodId",
                    args["methodId"] == "bgi.run_script_groups",
                    format!("{}", args["methodId"]),
                );
                check(
                    "组名精确按序",
                    group_names == vec!["挖矿讨伐", "兽怪暴徒"],
                    format!("{group_names:?}"),
                );
                check(
                    "跑完关游戏",
                    args["arguments"]["closeGameAfter"] == true,
                    format!("{}", args["arguments"]["closeGameAfter"]),
                );
                check(
                    "后台交接",
                    args["arguments"]["waitForCompletion"] == false,
                    format!("{}", args["arguments"]["waitForCompletion"]),
                );
                check(
                    "prepare 与 action 同 methodId",
                    entry["binding"]["prepare"]
                        .as_array()
                        .is_some_and(|prepare| {
                            !prepare.is_empty()
                                && prepare.iter().all(|call| {
                                    call["tool"] == "bgi.api.describe"
                                        && call["arguments"]["methodId"] == args["methodId"]
                                })
                        }),
                    "prepare".to_string(),
                );
            }
            check(
                "无运行调用",
                invokes.is_empty(),
                format!("{} 次", invokes.len()),
            );
        }
        "5-改为后台交接" => {
            // 与生产一致：必须恰好发生一次同 id 的 upsert，不新增副本。
            check(
                "恰好一次同 id 更新",
                state.save_events.len() == 1
                    && state.save_events[0]["id"] == "sc-mining-wait"
                    && state.save_events[0]["upsert"] == true,
                format!("事件 {:?}", state.save_events),
            );
            check(
                "原位更新未新增副本",
                state.saved_shortcuts.len() == 1,
                format!("{} 份", state.saved_shortcuts.len()),
            );
            let stored = state
                .saved_shortcuts
                .iter()
                .find(|entry| entry["id"] == "sc-mining-wait");
            if let Some(entry) = stored {
                let action = &entry["binding"]["action"];
                let args = &action["arguments"];
                check(
                    "仍是原单组 methodId（不擅自转批量）",
                    args["methodId"] == "bgi.run_script_group"
                        && args["arguments"].get("groupNames").is_none(),
                    format!("method={}", args["methodId"]),
                );
                check(
                    "binding 其余部分保留",
                    action["tool"] == "bgi.api.invoke"
                        && args["arguments"]["groupName"] == "挖矿讨伐"
                        && entry["binding"]["targetName"] == "挖矿讨伐"
                        && entry["binding"]["applicationName"] == "BetterGI"
                        && entry["binding"]["prepare"]
                            .as_array()
                            .is_some_and(|prepare| {
                                prepare.iter().any(|call| {
                                    call["tool"] == "bgi.api.describe"
                                        && call["arguments"]["methodId"] == "bgi.run_script_group"
                                })
                            }),
                    format!(
                        "tool={} group={} target={}",
                        action["tool"],
                        args["arguments"]["groupName"],
                        entry["binding"]["targetName"]
                    ),
                );
                check(
                    "等待改为 false",
                    args["arguments"]["waitForCompletion"] == false,
                    format!("{}", args["arguments"]["waitForCompletion"]),
                );
            }
            check(
                "无运行调用",
                invokes.is_empty(),
                format!("{} 次", invokes.len()),
            );
        }
        "6-只读不改" => {
            let team_read = ok_calls(state, "bgi.user.read").iter().any(|call| {
                call["arguments"]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("挖矿讨伐")
                    && (call["arguments"]["keys"].as_array().map(|keys| {
                        keys.iter()
                            .filter_map(Value::as_str)
                            .any(|key| key == "config")
                    }) == Some(true)
                        || call["arguments"]["keys"].is_null())
            });
            check(
                "实际读取了挖矿讨伐的队伍配置",
                team_read,
                "user.read config".to_string(),
            );
            check(
                "不写配置",
                writes.is_empty(),
                format!("{} 次", writes.len()),
            );
            check(
                "不保存入口",
                saved.is_empty(),
                format!("{} 次", saved.len()),
            );
            check(
                "无运行调用",
                invokes.is_empty(),
                format!("{} 次", invokes.len()),
            );
            check(
                "不提问",
                state.ask_calls == 0,
                format!("{} 次", state.ask_calls),
            );
            let expected_party = state
                .files
                .get("ScriptGroup/挖矿讨伐.json")
                .and_then(|doc| doc["config"]["pathingConfig"]["partyName"].as_str())
                .unwrap_or_default()
                .to_string();
            check(
                "最终答复给出实际读取的队伍名",
                !final_text.trim().is_empty() && final_text.contains(&expected_party),
                format!(
                    "final={:?} expected={expected_party}",
                    final_text.chars().take(60).collect::<String>()
                ),
            );
        }
        _ => {}
    }
    result
}

// ---------------------------------------------------------------- 运行

/// 生产风格的 system 上下文：用户自定义指令 + 技能目录 + 常驻技能正文。
/// 不注入任何场景答案或目标动作。
fn system_context(
    custom: &str,
    skills: &SkillRegistry,
    auto_load: bool,
    max_skills: usize,
    query: &str,
) -> String {
    let configured = configured_agent_instructions(custom);
    let catalog = skills
        .list()
        .iter()
        .map(|skill| format!("{}: {}", skill.name, skill.description))
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let mut context = format!(
        "{}

用户自定义指令：
{}

可用资料：
技能目录：
{catalog}",
        CORE_AGENT_POLICY,
        if configured.is_empty() {
            "无"
        } else {
            configured
        }
    );
    let matched_names: Vec<String> = if auto_load {
        skills
            .search(query, max_skills)
            .iter()
            .map(|skill| skill.name.clone())
            .collect()
    } else {
        vec![]
    };
    let matched: std::collections::HashSet<&str> =
        matched_names.iter().map(String::as_str).collect();
    for skill in skills
        .list()
        .into_iter()
        .filter(|skill| skill.always_load || matched.contains(skill.name.as_str()))
    {
        context.push_str(&format!(
            "

# 技能 {}
{}",
            skill.name, skill.body
        ));
    }
    context
}

async fn run_case(
    scenario: &Scenario,
    model: &ModelConfig,
    system_prompt: &str,
    definitions: &[ToolDefinition],
    skills: &SkillRegistry,
    index: &FeatureIndex,
    catalog: &ToolCatalog,
    state: &Sim,
    round_log: &mut Vec<Value>,
) -> Result<usize> {
    let mut messages: Vec<Message> = vec![Message {
        role: Role::System,
        content: system_prompt.to_string(),
        tool_call_id: None,
        tool_calls: vec![],
        reasoning: None,
    }];
    for line in scenario.history {
        messages.push(Message {
            role: Role::Assistant,
            content: (*line).to_string(),
            tool_call_id: None,
            tool_calls: vec![],
            reasoning: None,
        });
    }
    messages.push(Message {
        role: Role::User,
        content: scenario.prompt.to_string(),
        tool_call_id: None,
        tool_calls: vec![],
        reasoning: None,
    });
    run_loop(
        scenario,
        model,
        definitions,
        skills,
        index,
        catalog,
        state,
        round_log,
        messages,
    )
    .await
}

fn seeded_state(scenario: &Scenario) -> Sim {
    let state: Sim = Arc::new(Mutex::new(initial_state()));
    if scenario.seed_shortcut {
        // 场景5 的真实旧状态：等待完成才返回的既有入口。
        state.lock().unwrap().saved_shortcuts.push(json!({
            "id":"sc-mining-wait","name":"挖矿讨伐一键","description":"",
            "binding":{"targetName":"挖矿讨伐","applicationName":"BetterGI",
                "prepare":[{"tool":"bgi.api.describe","arguments":{"methodId":"bgi.run_script_group"}}],
                "action":{"tool":"bgi.api.invoke","arguments":{"methodId":"bgi.run_script_group",
                    "arguments":{"groupName":"挖矿讨伐","waitForCompletion":true}}}},
        }));
    }
    state
}

async fn run_loop(
    _scenario: &Scenario,
    model: &ModelConfig,
    definitions: &[ToolDefinition],
    skills: &SkillRegistry,
    index: &FeatureIndex,
    catalog: &ToolCatalog,
    state: &Sim,
    round_log: &mut Vec<Value>,
    mut messages: Vec<Message>,
) -> Result<usize> {
    let mut rounds = 0;
    while rounds < MAX_ROUNDS {
        rounds += 1;
        let cancel = CancellationToken::new();
        let response =
            gateway::complete(model.clone(), &messages, definitions, &cancel, |_| Ok(())).await?;
        let calls = response.tool_calls.clone();
        round_log.push(json!({
            "text": response.text,
            "calls": calls.iter().map(|call| json!({"name":call.name,"arguments":call.arguments})).collect::<Vec<_>>(),
            "results": [],
        }));
        messages.push(Message {
            role: Role::Assistant,
            content: response.text.clone(),
            tool_call_id: None,
            tool_calls: calls.clone(),
            reasoning: response.reasoning.clone(),
        });
        if calls.is_empty() {
            break;
        }
        let mut asked = false;
        for (at, call) in calls.iter().enumerate() {
            // user.ask 交给真实用户：同批次不得继续执行，也不再请求模型。
            if call.name == "user.ask" {
                if at + 1 < calls.len() {
                    state
                        .lock()
                        .unwrap()
                        .failures
                        .push("user.ask 之后同批次仍有调用，已拒绝执行".into());
                }
                asked = true;
            }
            let result = dispatch(&state, &call, definitions, catalog, skills, index);
            let result_ok = result.is_ok();
            let content = match result {
                Ok(value) => value.to_string(),
                Err(error) => json!({"error": error.to_string()}).to_string(),
            };
            if let Some(last) = round_log.last_mut() {
                last["results"]
                    .as_array_mut()
                    .unwrap_or(&mut vec![])
                    .push(json!({
                        "call": call.name,
                        "ok": result_ok,
                        "result": content.chars().take(240).collect::<String>(),
                    }));
            }
            messages.push(Message {
                role: Role::Tool,
                content,
                tool_call_id: Some(call.id.clone()),
                tool_calls: vec![],
                reasoning: None,
            });
            if asked {
                break;
            }
        }
        if asked {
            break;
        }
        if rounds == MAX_ROUNDS {
            state
                .lock()
                .unwrap()
                .failures
                .push(format!("已到 {MAX_ROUNDS} 轮上限仍有工具调用，未收敛"));
            break;
        }
    }
    Ok(rounds)
}

/// fixture 自检：不调用模型，只验证装配、模拟器语义与断言的敏感度。
fn fixture_checks(
    definitions: &[ToolDefinition],
    skills: &SkillRegistry,
    index: &FeatureIndex,
    catalog: &ToolCatalog,
) -> Result<Vec<(String, bool, String)>> {
    let mut checks = vec![];
    let mut check = |name: &str, pass: bool, detail: String| {
        checks.push((name.into(), pass, detail));
    };
    let names: Vec<&str> = definitions.iter().map(|d| d.name.as_str()).collect();
    check(
        "生产桥契约在列",
        names.contains(&"bgi.api.describe")
            && names.contains(&"bgi.api.invoke")
            && names.contains(&"bgi.user.write"),
        format!("{} 个定义", names.len()),
    );
    check(
        "core 元数据在列",
        names.contains(&"user.ask") && names.contains(&"shortcut.save"),
        "user.ask/shortcut.save".to_string(),
    );
    check(
        "技能装载",
        skills.list().len() > 0 && skills.get("create-shortcut").is_some(),
        format!("{} 个技能", skills.list().len()),
    );
    let context = system_context("", &skills, true, 4, "查询");
    check(
        "空自定义指令可装配",
        context.starts_with("你是 Sleepy Doll") && context.contains("用户自定义指令"),
        format!("{} 字", context.chars().count()),
    );
    for (query, expected) in [
        ("地图追踪 队伍切换", "workflow.group.edit"),
        ("路线换队", "workflow.group.edit"),
        ("全局设置恢复", "workflow.settings.recovery"),
    ] {
        let found = index.search(query, None, 0, 5)?;
        check(
            &format!("语义检索 {query}"),
            found["items"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["id"] == expected)),
            format!("期望 {expected}"),
        );
    }
    let state: Sim = Arc::new(Mutex::new(initial_state()));
    let call = |name: &str, arguments: Value| ToolCall {
        id: format!("fixture-{name}"),
        name: name.into(),
        arguments,
    };
    let listed = dispatch(
        &state,
        &call(
            "bgi.user.list",
            json!({"path":"ScriptGroup","jsonKeys":["name","index"]}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "list 返回两个组",
        listed.as_ref().is_ok_and(|value| value["count"] == 2),
        format!("{listed:?}"),
    );
    // 投影 read 拿到完整 SHA，但不构成写入先决条件。
    let projected = dispatch(
        &state,
        &call(
            "bgi.user.read",
            json!({"path":"ScriptGroup/兽怪暴徒.json","keys":["config"]}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    let sha = projected
        .as_ref()
        .ok()
        .and_then(|value| value["sha256"].as_str())
        .unwrap_or_default()
        .to_string();
    check(
        "投影 read 带完整 SHA 与 droppedKeys",
        !sha.is_empty(),
        sha.chars().take(8).collect(),
    );
    let dropped_write = dispatch(
        &state,
        &call(
            "bgi.user.write",
            json!({
                "path":"ScriptGroup/兽怪暴徒.json",
                "content": json!({"name":"兽怪暴徒","config":{"pathingConfig":{"enabled":true,"partyName":"蒸发"}}}).to_string(),
                "expectedSha256": sha,
            }),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "未全文 read 的写入被拒",
        dropped_write.is_err(),
        format!("{dropped_write:?}"),
    );
    let full = dispatch(
        &state,
        &call("bgi.user.read", json!({"path":"ScriptGroup/兽怪暴徒.json"})),
        definitions,
        catalog,
        skills,
        index,
    );
    let full_sha = full
        .as_ref()
        .ok()
        .and_then(|value| value["sha256"].as_str())
        .unwrap_or_default()
        .to_string();
    let full_text: Value = serde_json::from_str(
        full.as_ref()
            .ok()
            .and_then(|value| value["text"].as_str())
            .unwrap_or(""),
    )
    .unwrap_or(Value::Null);
    // 错误 SHA 真拒绝。
    let wrong_sha = dispatch(
        &state,
        &call(
            "bgi.user.write",
            json!({
                "path":"ScriptGroup/兽怪暴徒.json","content": full_text.to_string(),
                "expectedSha256": "0".repeat(64),
            }),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "错误 SHA 被拒",
        wrong_sha.is_err(),
        format!("{wrong_sha:?}"),
    );
    // 合法写入是完整替换：漏掉 unknownNote 就真的丢，断言必须能抓住。
    let mut lossy = full_text.clone();
    // 模拟模型漏交未知字段的写入：content 里没有带上它，
    // 完整替换语义下它应当真的丢失，由断言发现。
    if let Some(map) = lossy.as_object_mut() {
        map.remove("unknownNote");
    }
    lossy["config"]["pathingConfig"]["partyName"] = json!("蒸发");
    dispatch(
        &state,
        &call(
            "bgi.user.write",
            json!({
                "path":"ScriptGroup/兽怪暴徒.json","content": lossy.to_string(),
                "expectedSha256": full_sha,
            }),
        ),
        definitions,
        catalog,
        skills,
        index,
    )?;
    let after = state
        .lock()
        .unwrap()
        .files
        .get("ScriptGroup/兽怪暴徒.json")
        .cloned()
        .unwrap();
    // 完整替换：漏掉的 unknownNote 真的丢失（不自动合并），而全文里保留的
    // projects 原样存在——断言必须能区分"丢失"与"保留"。
    check(
        "数据丢失被断言抓住",
        after.get("unknownNote").is_none()
            && after["projects"]
                .as_array()
                .is_some_and(|projects| projects.len() == 1),
        "漏字段确实丢失，保留字段原样".to_string(),
    );
    // user.resolve 必须返回与生产 resolve.rs 相同的 verdict 形状。
    let resolved_run = dispatch(
        &state,
        &call("bgi.user.resolve", json!({"query":"运行挖矿讨伐"})),
        definitions,
        catalog,
        skills,
        index,
    );
    let shape_ok = |value: &Value| {
        for key in [
            "query",
            "verdict",
            "groupName",
            "missing",
            "candidates",
            "groups",
            "lookupScope",
            "next",
        ] {
            if value.get(key).is_none() {
                return false;
            }
        }
        true
    };
    check(
        "resolve run 判定带完整 verdict 形状",
        resolved_run.as_ref().is_ok_and(|value| {
            shape_ok(value) && value["verdict"] == "run" && value["groupName"] == "挖矿讨伐"
        }),
        format!("{resolved_run:?}"),
    );
    let resolved_create = dispatch(
        &state,
        &call("bgi.user.resolve", json!({"query":"矿点路线"})),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "resolve create 判定给出父目录候选",
        resolved_create.as_ref().is_ok_and(|value| {
            shape_ok(value)
                && value["verdict"] == "create"
                && value["candidates"][0]["children"] == 13
        }),
        format!("{resolved_create:?}"),
    );
    let resolved_missing = dispatch(
        &state,
        &call("bgi.user.resolve", json!({"query":"不存在的目标"})),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "resolve notFound 不冒充存在",
        resolved_missing
            .as_ref()
            .is_ok_and(|value| shape_ok(value) && value["verdict"] == "notFound"),
        format!("{resolved_missing:?}"),
    );
    // api.search：离线目录按真实 shape 返回已导出方法，稳定 ID 可命中。
    let searched = dispatch(
        &state,
        &call("bgi.api.search", json!({"query":"prepare_pathing_group"})),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "api.search 命中已导出方法",
        searched.as_ref().is_ok_and(|value| {
            value["total"].as_u64().is_some_and(|total| total >= 1)
                && value["items"][0]["methodId"] == "bgi.prepare_pathing_group"
        }),
        format!("{searched:?}"),
    );
    // 目录不存在必须如实报错，不返回空列表伪证据。
    let ghost_dir = dispatch(
        &state,
        &call("bgi.user.list", json!({"path":"Party"})),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "不存在目录返回错误",
        ghost_dir.is_err(),
        format!("{ghost_dir:?}"),
    );
    // describe 契约来自真实导出元数据：保留 pathing/ 前缀描述与修正后的 wait 语义。
    let contract = dispatch(
        &state,
        &call(
            "bgi.api.describe",
            json!({"methodId":"bgi.prepare_pathing_group"}),
        ),
        definitions,
        catalog,
        skills,
        index,
    )?;
    let wait_desc = dispatch(
        &state,
        &call(
            "bgi.api.describe",
            json!({"methodId":"bgi.run_script_group"}),
        ),
        definitions,
        catalog,
        skills,
        index,
    )?;
    check(
        "describe 使用真实导出契约",
        contract["inputSchema"]["properties"]["path"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("pathing/"))
            && wait_desc["inputSchema"]["properties"]["waitForCompletion"]["description"]
                .as_str()
                .is_some_and(|text| text.contains("只有用户明确要求等结果才为 true")),
        "pathing 前缀与 wait 语义".to_string(),
    );
    // prepare 路径正负例：合法前缀通过，错误追加前缀拒绝。
    let prepared_ok = dispatch(
        &state,
        &call(
            "bgi.api.invoke",
            json!({"methodId":"bgi.prepare_pathing_group",
                "arguments":{"path":"pathing/矿点","groupName":"fixture-合法前缀"}}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "prepare pathing/矿点 通过",
        prepared_ok.is_ok(),
        format!("{prepared_ok:?}"),
    );
    let prepared_bad = dispatch(
        &state,
        &call(
            "bgi.api.invoke",
            json!({"methodId":"bgi.prepare_pathing_group",
                "arguments":{"path":"pathing/AutoPathing/矿点","groupName":"fixture-错误前缀"}}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "prepare pathing/AutoPathing/... 拒绝",
        prepared_bad.is_err(),
        format!("{prepared_bad:?}"),
    );
    // 危险动作：先 describe 再 invoke 真正的运行接口，必须被拒。
    dispatch(
        &state,
        &call(
            "bgi.api.describe",
            json!({"methodId":"bgi.run_script_groups"}),
        ),
        definitions,
        catalog,
        skills,
        index,
    )?;
    let run = dispatch(
        &state,
        &call(
            "bgi.api.invoke",
            json!({"methodId":"bgi.run_script_groups","arguments":{"groupNames":["挖矿讨伐"]}}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check("未授权运行被拒", run.is_err(), format!("{run:?}"));
    let unknown = dispatch(
        &state,
        &call("bgi.host.reboot", json!({})),
        definitions,
        catalog,
        skills,
        index,
    );
    check("未知工具被拒", unknown.is_err(), format!("{unknown:?}"));
    let asked = dispatch(
        &state,
        &call(
            "user.ask",
            json!({"questions":[{"id":"target","header":"目标","question":"改哪一个？","options":[{"label":"兽怪暴徒"}]}]}),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check("user.ask 归一", asked.is_ok(), format!("{asked:?}"));
    // 生产 nodes：合法绑定通过；带临时标识的绑定拒绝。
    let good_binding = json!({
        "name":"挖矿讨伐一键","binding":{"targetName":"挖矿讨伐","applicationName":"BetterGI",
            "prepare":[{"tool":"bgi.api.describe","arguments":{"methodId":"bgi.run_script_groups"}}],
            "action":{"tool":"bgi.api.invoke","arguments":{"methodId":"bgi.run_script_groups",
                "arguments":{"groupNames":["挖矿讨伐","兽怪暴徒"],"closeGameAfter":true,"waitForCompletion":false}}}}});
    let saved_ok = dispatch(
        &state,
        &call("shortcut.save", good_binding),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "合法绑定保存",
        saved_ok.is_ok() && state.lock().unwrap().saved_shortcuts.len() == 1,
        format!("{saved_ok:?}"),
    );
    let bad_binding = dispatch(
        &state,
        &call(
            "shortcut.save",
            json!({
                "name":"坏例","binding":{"targetName":"x","applicationName":"BetterGI",
                    "action":{"tool":"bgi.api.invoke","arguments":{"methodId":"bgi.run_script_groups","arguments":{"groupNames":[],"jobId":"old"}}}}
            }),
        ),
        definitions,
        catalog,
        skills,
        index,
    );
    check(
        "临时标识绑定被拒",
        bad_binding.is_err(),
        format!("{bad_binding:?}"),
    );
    Ok(checks)
}

fn write_report(cache: &std::path::Path, report: &Value) -> Result<PathBuf> {
    std::fs::create_dir_all(cache).map_err(|e| Error::Tool(format!("创建报告目录失败：{e}")))?;
    let path = cache.join("skill-plan-report.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(report).unwrap_or_default(),
    )
    .map_err(|e| Error::Tool(format!("写入报告失败：{e}")))?;
    Ok(path)
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // 本任务登记的唯一缓存目录；中间数据不落系统 Temp。
    let cache = manifest.join("target/.tmp/shortcut-runtime");
    for key in ["TEMP", "TMP"] {
        if !std::env::var_os(key).is_some_and(|value| value.to_string_lossy().contains("target")) {
            unsafe { std::env::set_var(key, &cache) };
        }
    }
    let args: Vec<String> = std::env::args().collect();
    let case_filter: Option<String> = args
        .iter()
        .position(|arg| arg == "--case")
        .and_then(|at| args.get(at + 1))
        .cloned();
    let live = args
        .iter()
        .position(|arg| arg == "--live-model")
        .and_then(|at| args.get(at + 1));

    let mut skills = SkillRegistry::default();
    if let Err(error) = skills.load(&[(
        manifest.join("plugins/bgi/skills"),
        SkillSource::Plugin("bgi".into()),
    )]) {
        eprintln!("技能注册表装载失败：{error}");
        return std::process::ExitCode::FAILURE;
    }
    for text in EMBEDDED_SKILLS {
        let _ = skills.load_embedded(text);
    }
    let Ok(index) = FeatureIndex::bundled() else {
        eprintln!("功能索引装载失败");
        return std::process::ExitCode::FAILURE;
    };
    let mut bridge = ToolRegistry::default();
    let client = BgiClient::new(BridgeConfig {
        enabled: false,
        base_url: "http://127.0.0.1:0".into(),
        token: None,
        instance_id: None,
        timeout_ms: 1000,
        host_install_path: None,
        launch_silently: false,
    });
    if register_tools(&mut bridge, Arc::new(client)).is_err() {
        eprintln!("生产工具契约登记失败");
        return std::process::ExitCode::FAILURE;
    }
    let Ok(definitions) = all_definitions(&bridge) else {
        eprintln!("工具名重复或装配失败");
        return std::process::ExitCode::FAILURE;
    };
    let catalog = combined_catalog(&definitions);

    let Some(config_path) = live else {
        let report = match fixture_checks(&definitions, &skills, &index, &catalog) {
            Ok(checks) => {
                let passed = checks.iter().all(|(_, ok, _)| *ok);
                json!({
                    "mode":"fixture",
                    "passed":passed,
                    "checks":checks.iter().map(|(name, ok, detail)|
                        json!({"name":name,"pass":ok,"detail":detail})).collect::<Vec<_>>(),
                })
            }
            Err(error) => json!({"mode":"fixture","passed":false,"error":error.to_string()}),
        };
        for item in report["checks"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            println!(
                "[{}] {}：{}",
                if item["pass"] == true { "PASS" } else { "FAIL" },
                item["name"].as_str().unwrap_or_default(),
                item["detail"].as_str().unwrap_or_default()
            );
        }
        return match write_report(&cache, &report) {
            Ok(path) => {
                println!("报告：{}", path.display());
                if report["passed"] == true {
                    std::process::ExitCode::SUCCESS
                } else {
                    std::process::ExitCode::FAILURE
                }
            }
            Err(error) => {
                eprintln!("报告写入失败：{error}");
                std::process::ExitCode::FAILURE
            }
        };
    };

    // --live-model：只读解析配置，与生产相同的 ${ENV:} 扩展；密钥只驻内存。
    let raw = match std::fs::read_to_string(config_path) {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("无法读取配置文件（只读）：{error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut root: Value = match serde_json::from_str(&raw) {
        Ok(root) => root,
        Err(_) => {
            eprintln!("配置文件不是 JSON");
            return std::process::ExitCode::FAILURE;
        }
    };
    if expand_env(&mut root).is_err() {
        eprintln!("配置中的环境变量扩展失败");
        return std::process::ExitCode::FAILURE;
    }
    let active = root["activeModel"].as_str().unwrap_or_default().to_string();
    let model: ModelConfig = match root["models"]
        .as_array()
        .and_then(|models| models.iter().find(|model| model["id"] == active).cloned())
        .map(|value| serde_json::from_value(value).ok())
        .flatten()
    {
        Some(model) => model,
        None => {
            eprintln!("配置中没有 activeModel={active} 对应的模型");
            return std::process::ExitCode::FAILURE;
        }
    };
    // agent.systemPrompt 为空是正常默认：装配用底座政策补足。
    let custom_prompt = root["agent"]["systemPrompt"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let auto_load_skills = root["agent"]["autoLoadSkills"].as_bool().unwrap_or(true);
    let max_loaded_skills = root["agent"]["maxLoadedSkills"].as_u64().unwrap_or(4) as usize;
    // 单轮连接超时不超过场景时限；其余配置原样使用。
    let mut model = model;
    model.options.timeout_ms = model.options.timeout_ms.min(CASE_TIMEOUT_SECS * 1000);
    let mut cases = vec![];
    for scenario in scenarios().into_iter().filter(|scenario| {
        case_filter
            .as_deref()
            .is_none_or(|filter| scenario.id.contains(filter))
    }) {
        let started = std::time::Instant::now();
        let system = system_context(
            &custom_prompt,
            &skills,
            auto_load_skills,
            max_loaded_skills,
            scenario.prompt,
        );
        let state = seeded_state(&scenario);
        let mut round_log: Vec<Value> = vec![];
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(CASE_TIMEOUT_SECS),
            run_case(
                &scenario,
                &model,
                &system,
                &definitions,
                &skills,
                &index,
                &catalog,
                &state,
                &mut round_log,
            ),
        )
        .await;
        // 状态与轮日志在 case 外持有：超时也保留已收到的调用与最后完成轮。
        let locked = state.lock().unwrap();
        let mut entry = match outcome {
            Ok(Ok(rounds)) => {
                let checks = assertions(&scenario, &locked, &round_log);
                json!({
                    "id":scenario.id,
                    "prompt":scenario.prompt,
                    "rounds":rounds,
                    "roundsLog":round_log,
                    "elapsedSecs":started.elapsed().as_secs_f64(),
                    "calls":locked.calls,
                    "failures":locked.failures,
                    "savedShortcuts":locked.saved_shortcuts,
                    "assertions":checks.iter().map(|(name, ok, detail)|
                        json!({"name":name,"pass":ok,"detail":detail})).collect::<Vec<_>>(),
                    "passed":checks.iter().all(|(_, ok, _)| *ok),
                })
            }
            Ok(Err(error)) => json!({
                "id":scenario.id,"passed":false,"error":error.to_string(),
                "roundsLog":round_log,"calls":locked.calls,"failures":locked.failures,
            }),
            Err(_) => {
                let mut failures = locked.failures.clone();
                failures.push(format!(
                    "超过 {}s 时限（已完成 {} 轮，最后一次模型调用未返回）",
                    CASE_TIMEOUT_SECS,
                    round_log.len()
                ));
                let checks = assertions(&scenario, &locked, &round_log);
                json!({
                    "id":scenario.id,"passed":false,
                    "error":format!("超过 {}s 时限", CASE_TIMEOUT_SECS),
                    "roundsLog":round_log,"calls":locked.calls,
                    "failures":failures,
                    "assertions":checks.iter().map(|(name, ok, detail)|
                        json!({"name":name,"pass":ok,"detail":detail})).collect::<Vec<_>>(),
                })
            }
        };
        entry["prompt"] = json!(scenario.prompt);
        println!(
            "[{}] {}（{:.1}s）",
            if entry["passed"] == true {
                "PASS"
            } else {
                "FAIL"
            },
            scenario.id,
            started.elapsed().as_secs_f64()
        );
        cases.push(entry);
    }
    let passed = cases.iter().all(|case| case["passed"] == true);
    let report = json!({"mode":"live","passed":passed,"cases":cases});
    return match write_report(&cache, &report) {
        Ok(path) => {
            println!("报告：{}", path.display());
            if passed {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("报告写入失败：{error}");
            std::process::ExitCode::FAILURE
        }
    };
}
