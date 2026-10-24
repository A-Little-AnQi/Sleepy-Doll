//! 用本机 User 目录判定目标是否可运行。
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// 原生路线动作分类；只记录机制事实，不决定是否询问用户。
const BATTLE_ACTIONS: [&str; 2] = ["fight", "combat_script"];
/// 已知绑定特定角色的特殊机制动作。
const SPECIAL_ACTIONS: [&str; 2] = ["up_down_grab_leaf", "linnea_mining"];
/// 机制中性的普通动作/位移类型（本机 131 份真实路线的词汇表核对过）：
/// 不构成角色前提。
const BENIGN_ACTIONS: [&str; 11] = [
    "stop_flying",
    "mining",
    "path",
    "teleport",
    "target",
    "orientation",
    "pick_around",
    "force_tp",
    "log_output",
    "四叶印",
    "hydrogranum",
];
/// 单组/单目录最多深读的路线文件数：程序侧聚合用，聚合结果才给模型。
const MAX_REQUIREMENT_FILES: usize = 60;

/// 本地未命中后继续查当前全仓索引；查询失败不能被解释成资源不存在。
pub fn resolve_target(
    mut local: Value,
    repository_search: impl FnOnce(&Value) -> Result<Value>,
) -> Result<Value> {
    local["lookupScope"] = json!("local");
    if local["verdict"] != "notFound" {
        return Ok(local);
    }
    let arguments = json!({"query":local["query"],"category":"all","limit":8});
    let repository = repository_search(&arguments).and_then(|value| {
        if value["total"].as_u64().is_none() || !value["items"].is_array() {
            return Err(Error::Tool("中央仓库返回的检索证据不完整".into()));
        }
        Ok(value)
    });
    match repository {
        Ok(repository) => {
            let found = repository["total"].as_u64().unwrap_or(0) > 0;
            local["lookupScope"] = json!("localAndCentralRepository");
            local["verdict"] = json!(if found { "resourceFound" } else { "notFound" });
            local["repository"] = repository;
        }
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => {
            local["verdict"] = json!("lookupFailed");
            local["repositoryError"] = json!(error.to_string());
        }
    }
    Ok(local)
}

/// 判定采集或运行目标的入口：一次扫描配置组与 AutoPathing 目录名，不打开路线 JSON。
pub fn resolve_local(root: &Path, query: &str) -> Value {
    let query = query.trim();
    let groups = scan_groups(root);
    // 明确的组名优先于组内项目名称，避免“千星”被同样引用该脚本的“周一”组抢成歧义。
    let exact = groups
        .iter()
        .filter(|group| normalize(&group.name) == normalize(query))
        .cloned()
        .collect::<Vec<_>>();
    let named = groups
        .iter()
        .filter(|group| matches_query(query, &group.name))
        .cloned()
        .collect::<Vec<_>>();
    let matched_groups = if !exact.is_empty() {
        exact
    } else if !named.is_empty() {
        named
    } else {
        groups
            .iter()
            .filter(|group| {
                group
                    .projects
                    .iter()
                    .any(|project| project.enabled && matches_query(query, &project.name))
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut candidates = scan_pathing(root, query);
    candidates.extend(scan_javascript(root, query));

    if !matched_groups.is_empty() {
        let runnable = matched_groups
            .iter()
            .filter(|group| {
                group.missing.is_empty() && group.projects.iter().any(|project| project.enabled)
            })
            .collect::<Vec<_>>();
        if runnable.len() == 1 {
            let group = runnable[0];
            // 路线需求只适用于已启用 Pathing 项目，不能拿 JS 目录当 JSON 读取。
            let files = project_requirement_files(root, group);
            let requirements = scan_requirements(root, &files);
            return report_requirements(
                query,
                "run",
                Some(&group.name),
                &[],
                &candidates,
                &matched_groups,
                requirements,
            );
        }
        if runnable.len() > 1 {
            return report_requirements(
                query,
                "ambiguous",
                None,
                &[],
                &candidates,
                &matched_groups,
                Value::Null,
            );
        }
        let missing = matched_groups
            .iter()
            .flat_map(|group| group.missing.iter().cloned())
            .collect::<Vec<_>>();
        return report_requirements(
            query,
            "repair",
            matched_groups.first().map(|group| group.name.as_str()),
            &missing,
            &candidates,
            &matched_groups,
            Value::Null,
        );
    }

    if !candidates.is_empty() {
        // create 候选同样聚合需求（最多取前两个 pathing 父目录深读）。
        let mut files = Vec::new();
        for candidate in candidates.iter().filter(|c| c.kind == "Pathing").take(2) {
            collect_json_files(
                &root.join(&candidate.path),
                &mut files,
                MAX_REQUIREMENT_FILES,
            );
        }
        let mut requirements = scan_requirements(root, &files);
        if files.is_empty() && candidates.iter().any(|c| c.kind == "Pathing") {
            requirements = unresolved_route_requirements();
        }
        return report_requirements(query, "create", None, &[], &candidates, &[], requirements);
    }
    let mut report = report_requirements(query, "notFound", None, &[], &[], &[], Value::Null);
    // Literal matching cannot resolve every user nickname. Expose bounded local
    // names for semantic selection without guessing a runnable group or aliases.
    report["availableGroups"] =
        json!(groups.iter().take(20).map(|group| json!({
        "name":group.name,"file":path_display(&group.file),
        "enabledProjects":group.projects.iter().filter(|project|project.enabled).count(),
    })).collect::<Vec<_>>());
    report["availableGroupCount"] = json!(groups.len());
    report["groupListing"] = json!({"tool":"bgi.user.list","arguments":{"path":"ScriptGroup","jsonKeys":["name","index"]}});
    report
}

/// 仅组内已启用的地图追踪文件适用路线配队分析。
/// JS 的队伍要求由脚本自己的设置、声明和校验负责，不套用路线分类。
fn project_requirement_files(root: &Path, group: &Group) -> Vec<PathBuf> {
    group
        .projects
        .iter()
        .filter(|project| project.enabled && project.kind.eq_ignore_ascii_case("Pathing"))
        .map(|project| {
            root.join(resource_path(&project.kind, &project.folder_name))
                .join(&project.name)
        })
        .collect()
}

fn unresolved_route_requirements() -> Value {
    json!({"scope":"pathingOnly","status":"unknown","containsPartySensitiveActions":false,
        "inspectionRequired":true,"scannedFiles":0,"unreadableFiles":0,
        "note":"路线内容未能读取，角色相关机制未知。"})
}

/// 递归收集目录下的 JSON 文件（带上限），供候选目录的需求聚合。
fn collect_json_files(directory: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    if out.len() >= limit {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, out, limit);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        {
            out.push(path);
            if out.len() >= limit {
                return;
            }
        }
    }
}

/// 深读路线文件聚合战斗/特殊机制前提。三态结论：preconditionsFound /
/// noPreconditions / unknown——文件读不了、超上限或出现未登记动作时是
/// unknown 要求检查资源，不把未知内容冒充已经证实的配队前提。
fn scan_requirements(root: &Path, files: &[PathBuf]) -> Value {
    if files.is_empty() {
        return json!({"scope":"pathingOnly","status":"notApplicable","battle":false,
            "specialActions":[],"unrecognizedActions":[],"containsPartySensitiveActions":false,
            "inspectionRequired":false,"scannedFiles":0,"unreadableFiles":0,"truncated":false,"sources":[],
            "scriptPreconditions":"scriptDefined",
            "note":"没有已启用的地图追踪项目；此扫描不覆盖 JS 的参数或角色前提。"});
    }
    let mut actions: BTreeMap<String, usize> = BTreeMap::new();
    let mut scanned = 0usize;
    let mut unreadable = 0usize;
    let mut sources: Vec<String> = Vec::new();
    let mut truncated = false;
    for file in files.iter().take(MAX_REQUIREMENT_FILES) {
        let Ok(text) = fs::read_to_string(file) else {
            unreadable += 1;
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            unreadable += 1;
            continue;
        };
        if !["positions", "waypoints"]
            .iter()
            .any(|key| value[*key].is_array())
        {
            unreadable += 1;
            continue;
        }
        scanned += 1;
        if sources.len() < 8 {
            sources.push(
                file.strip_prefix(root)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
        for key in ["positions", "waypoints"] {
            for point in value[key].as_array().into_iter().flatten() {
                for field in ["action", "type"] {
                    if let Some(name) = point[field].as_str()
                        && !name.is_empty()
                    {
                        *actions.entry(name.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }
    if files.len() > MAX_REQUIREMENT_FILES {
        truncated = true;
    }
    let verdict = classify_actions(&actions, unreadable > 0 || truncated);
    let required = verdict.party_sensitive_actions;
    json!({
        "scope":"pathingOnly",
        "status": verdict.status,
        "inspectionRequired": verdict.inspection_required,
        "battle": verdict.battle,
        "specialActions": verdict.special.iter().map(|(name, count)| json!({"action":name,"count":count})).collect::<Vec<_>>(),
        "unrecognizedActions": verdict.unrecognized.iter().map(|(name, count)| json!({"action":name,"count":count})).collect::<Vec<_>>(),
        "containsPartySensitiveActions": required,
        "scannedFiles": scanned,
        "unreadableFiles": unreadable,
        "truncated": truncated,
        "sources": sources,
    })
}

/// 纯分类：battle 命中即战斗；已知特殊机制单列；未登记动作不冒充良性，
/// 与读取不全一起归入 unknown，需检查资源，不能据此要求用户配队。
fn classify_actions(actions: &BTreeMap<String, usize>, incomplete: bool) -> ActionVerdict {
    let battle = actions
        .keys()
        .any(|name| BATTLE_ACTIONS.contains(&name.as_str()));
    let special = actions
        .iter()
        .filter(|(name, _)| SPECIAL_ACTIONS.contains(&name.as_str()))
        .map(|(name, count)| (name.clone(), *count))
        .collect::<Vec<_>>();
    let unrecognized = actions
        .iter()
        .filter(|(name, _)| {
            !BATTLE_ACTIONS.contains(&name.as_str())
                && !SPECIAL_ACTIONS.contains(&name.as_str())
                && !BENIGN_ACTIONS.contains(&name.as_str())
        })
        .map(|(name, count)| (name.clone(), *count))
        .collect::<Vec<_>>();
    let required = battle || !special.is_empty();
    let inspection_required = !unrecognized.is_empty() || incomplete;
    let status = if inspection_required {
        "unknown"
    } else if required {
        "preconditionsFound"
    } else {
        "noPreconditions"
    };
    ActionVerdict {
        status: status.to_string(),
        battle,
        special,
        unrecognized,
        party_sensitive_actions: required,
        inspection_required,
    }
}

struct ActionVerdict {
    status: String,
    battle: bool,
    special: Vec<(String, usize)>,
    unrecognized: Vec<(String, usize)>,
    party_sensitive_actions: bool,
    inspection_required: bool,
}

#[derive(Clone)]
struct Group {
    name: String,
    file: PathBuf,
    projects: Vec<Project>,
    missing: Vec<String>,
    /// 组配置里可原生改写的运行槽位（set_pathing_party 管理的字段）。
    config_slots: Value,
}

#[derive(Clone)]
struct Project {
    name: String,
    kind: String,
    folder_name: String,
    exists: bool,
    /// 只统计会执行的项目：status 缺省视为启用，显式 Disabled 不算执行前提。
    enabled: bool,
}

struct Candidate {
    path: PathBuf,
    children: Vec<String>,
    kind: &'static str,
}

/// 由组 JSON 构造 Group（含运行槽位快照）；纯函数，验收直接测真实解析逻辑。
fn group_from_value(root: &Path, path: PathBuf, name: String, value: &Value) -> Group {
    let projects = value["projects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|project| parse_project(root, project))
        .collect::<Vec<_>>();
    let missing = projects
        .iter()
        .filter(|project| project.enabled && !project.exists)
        .map(|project| resource_display(&project.kind, &project.folder_name))
        .collect();
    let pathing = value["config"]["pathingConfig"].clone();
    let config_slots = if pathing.is_object() {
        json!({
            "partyName": pathing["partyName"],
            "enabled": pathing["enabled"],
            "hurryOnAvatar": pathing["hurryOnAvatar"],
        })
    } else {
        Value::Null
    };
    Group {
        name,
        file: path,
        projects,
        missing,
        config_slots,
    }
}

pub fn missing_paths_for_group(root: &Path, group_name: &str) -> Option<Vec<String>> {
    let needle = normalize(group_name);
    scan_groups(root)
        .into_iter()
        .find(|group| normalize(&group.name) == needle)
        .map(|group| group.missing)
}

#[allow(clippy::too_many_arguments)]
fn report_requirements(
    query: &str,
    verdict: &str,
    group_name: Option<&str>,
    missing: &[String],
    candidates: &[Candidate],
    groups: &[Group],
    requirements: Value,
) -> Value {
    json!({
        "query": query,
        "verdict": if requirements["inspectionRequired"]==true && matches!(verdict,"run"|"create") {"inspect"} else {verdict},
        "groupName": group_name,
        "missing": missing,
        "requirements": requirements,
        "candidates": candidates.iter().map(|candidate| json!({
            "path": path_display(&candidate.path),
            "children": candidate.children,
            "kind": candidate.kind,
        })).collect::<Vec<_>>(),
        "groups": groups.iter().map(|group| json!({
            "name": group.name,
            "file": path_display(&group.file),
            "missing": group.missing,
            "config": group.config_slots,
            "projects": group.projects.iter().map(|project| json!({
                "name": project.name,
                "type": project.kind,
                "folderName": project.folder_name,
                "exists": project.exists,
                "enabled": project.enabled,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "references": [{"skill":"bgi-operator","path":"references/resources.md"}],
    })
}

fn scan_javascript(root: &Path, query: &str) -> Vec<Candidate> {
    let Ok(entries) = fs::read_dir(root.join("JsScript")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() || path.symlink_metadata().ok()?.file_type().is_symlink() {
                return None;
            }
            let text = fs::read_to_string(path.join("manifest.json")).ok()?;
            let manifest: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
            let folder = entry.file_name().to_string_lossy().into_owned();
            let name = manifest["name"].as_str().unwrap_or(&folder);
            if !matches_query(query, name) && !matches_query(query, &folder) {
                return None;
            }
            Some(Candidate {
                path: path.strip_prefix(root).ok()?.to_path_buf(),
                children: vec!["manifest.json".into()],
                kind: "Javascript",
            })
        })
        .collect()
}

fn scan_groups(root: &Path) -> Vec<Group> {
    let directory = root.join("ScriptGroup");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let name = value["name"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        groups.push(group_from_value(root, path, name, &value));
    }
    groups.sort_by(|left, right| left.name.cmp(&right.name));
    groups
}

fn parse_project(root: &Path, value: &Value) -> Option<Project> {
    let folder_name = value["folderName"]
        .as_str()
        .or_else(|| value["path"].as_str())
        .filter(|name| !name.is_empty())?
        .replace('\\', "/");
    let kind = value["type"].as_str().unwrap_or("Pathing").to_owned();
    let mut relative = resource_path(&kind, &folder_name);
    if kind.eq_ignore_ascii_case("Pathing") {
        let file_name = value["name"].as_str().filter(|name| !name.is_empty())?;
        relative = relative.join(file_name);
    }
    let exists = root.join(&relative).exists();
    let status = value["status"].as_str().unwrap_or("Enabled");
    Some(Project {
        name: value["name"].as_str().unwrap_or(&folder_name).to_owned(),
        kind,
        folder_name,
        exists,
        enabled: status.eq_ignore_ascii_case("Enabled"),
    })
}

fn resource_path(kind: &str, folder_name: &str) -> PathBuf {
    let folder = folder_name.replace('\\', "/");
    match kind.to_ascii_lowercase().as_str() {
        "javascript" | "js" => PathBuf::from("JsScript").join(&folder),
        "keymouse" | "keymousescript" => PathBuf::from("KeyMouseScript").join(&folder),
        _ => PathBuf::from("AutoPathing").join(&folder),
    }
}

fn resource_display(kind: &str, folder_name: &str) -> String {
    path_display(&resource_path(kind, folder_name))
}

fn scan_pathing(root: &Path, query: &str) -> Vec<Candidate> {
    let auto = root.join("AutoPathing");
    let mut matches = Vec::new();
    walk_pathing(root, &auto, query, 0, &mut matches);
    matches
}

fn walk_pathing(
    user_root: &Path,
    current: &Path,
    query: &str,
    depth: usize,
    out: &mut Vec<Candidate>,
) {
    if depth > 6 {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    let mut children = Vec::new();
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            children.push(name.clone());
            subdirs.push(path);
        } else {
            children.push(name);
        }
    }
    children.sort();
    let name = current
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_auto_root = current.ends_with("AutoPathing") && current.parent() == Some(user_root);
    if !is_auto_root && matches_query(query, &name) {
        out.push(Candidate {
            path: current
                .strip_prefix(user_root)
                .unwrap_or(current)
                .to_path_buf(),
            children,
            kind: "Pathing",
        });
        return;
    }
    for directory in subdirs {
        walk_pathing(user_root, &directory, query, depth + 1, out);
    }
}

fn matches_query(query: &str, name: &str) -> bool {
    let query = normalize(query);
    let name = normalize(name);
    if query.is_empty() || name.chars().count() < 2 {
        return false;
    }
    query.contains(&name) || name.contains(&query)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmatched_names_expose_bounded_local_groups_without_guessing_a_target() {
        let root = crate::test_root("resolve-group-names");
        fs::create_dir_all(root.join("ScriptGroup")).unwrap();
        fs::write(
            root.join("ScriptGroup/锄地一条龙.json"),
            r#"{"name":"锄地一条龙","projects":[]}"#,
        )
        .unwrap();
        let initial = resolve_local(&root, "锄大地");
        assert_eq!(initial["availableGroups"][0]["name"], "锄地一条龙");
        assert!(
            initial["groupName"].is_null(),
            "a nickname must not select a group implicitly"
        );
        for index in 0..25 {
            let name = format!("组{index:02}");
            fs::write(
                root.join("ScriptGroup").join(format!("{name}.json")),
                json!({"name":name,"projects":[]}).to_string(),
            )
            .unwrap();
        }
        let local = resolve_local(&root, "锄大地");
        assert_eq!(local["verdict"], "notFound");
        assert!(local["groupName"].is_null());
        assert_eq!(local["availableGroups"].as_array().unwrap().len(), 20);
        assert_eq!(local["availableGroupCount"], 26);
        assert_eq!(local["groupListing"]["arguments"]["path"], "ScriptGroup");
        let result = resolve_target(local, |_| Ok(json!({"total":0,"items":[]}))).unwrap();
        assert_eq!(result["references"][0]["skill"], "bgi-operator");
        assert_eq!(result["availableGroupCount"], 26);
    }

    #[test]
    #[ignore = "需要隔离的本机 JS 资源目录"]
    fn installed_custom_js_is_found_without_a_repository_or_group() {
        let root = PathBuf::from(
            std::env::var_os("SLEEPY_DOLL_LOCAL_JS_FIXTURE").expect("缺少隔离资源目录"),
        );
        let result = resolve_local(&root, "运行下仅本机脚本");
        assert_eq!(result["verdict"], "create");
        assert!(
            result["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|candidate| candidate["kind"] == "Javascript"
                    && candidate["path"] == "JsScript/custom-only")
        );
    }

    fn requirement_fixture() -> PathBuf {
        let root = crate::test_root("bgi-party-requirements");
        fs::create_dir_all(root.join("ScriptGroup")).unwrap();
        root
    }

    #[test]
    fn javascript_group_uses_enabled_items_without_inventing_route_party_requirements() {
        let root = requirement_fixture();
        fs::create_dir_all(root.join("JsScript/weekly")).unwrap();
        fs::write(
            root.join("JsScript/weekly/manifest.json"),
            r#"{"name":"战斗名称也不能代替任务类型","settings_ui":"settings.json"}"#,
        )
        .unwrap();
        fs::write(root.join("ScriptGroup/千星.json"),json!({"name":"千星","projects":[
            {"name":"千星成就","type":"Javascript","folderName":"weekly","status":"Enabled","jsScriptSettingsObject":{"runJS":true}},
            {"name":"停用且未安装","type":"Javascript","folderName":"absent","status":"Disabled"},
            {"name":"停用战斗.json","type":"Pathing","folderName":"absent","status":"Disabled"}
        ]}).to_string()).unwrap();
        fs::write(root.join("ScriptGroup/周一.json"),json!({"name":"周一","projects":[{"name":"千星成就","type":"Javascript","folderName":"weekly","status":"Enabled"}]}).to_string()).unwrap();
        let result = resolve_local(&root, "跑下千星");
        assert_eq!(result["verdict"], "run");
        assert_eq!(result["requirements"]["status"], "notApplicable");
        assert_eq!(result["requirements"]["scope"], "pathingOnly");
        assert_eq!(
            result["requirements"]["scriptPreconditions"],
            "scriptDefined"
        );
        assert_eq!(
            result["requirements"]["containsPartySensitiveActions"],
            false
        );
        assert_eq!(result["requirements"]["inspectionRequired"], false);
        assert_eq!(result["requirements"]["unreadableFiles"], 0);
        assert_eq!(result["groups"][0]["projects"][1]["enabled"], false);
        assert!(result["missing"].as_array().unwrap().is_empty());
        assert!(project_requirement_files(&root, &scan_groups(&root)[0]).is_empty());
        assert_eq!(
            serde_json::from_str::<Value>(
                &fs::read_to_string(root.join("ScriptGroup/千星.json")).unwrap()
            )
            .unwrap()["projects"][0]["jsScriptSettingsObject"]["runJS"],
            true
        );
    }

    #[test]
    fn mixed_group_checks_only_enabled_pathing_and_preserves_script_configuration() {
        let root = requirement_fixture();
        fs::create_dir_all(root.join("AutoPathing/routes")).unwrap();
        fs::create_dir_all(root.join("JsScript/combat-script")).unwrap();
        fs::write(
            root.join("AutoPathing/routes/plain.json"),
            r#"{"positions":[{"type":"path","action":"pick_around"}]}"#,
        )
        .unwrap();
        fs::write(
            root.join("AutoPathing/routes/battle.json"),
            r#"{"positions":[{"action":"fight"}]}"#,
        )
        .unwrap();
        let group = json!({"name":"混合","projects":[
            {"name":"有队伍要求的脚本","type":"Javascript","folderName":"combat-script","jsScriptSettingsObject":{"partyName":"已有队伍"}},
            {"name":"plain.json","type":"Pathing","folderName":"routes","status":"Enabled"},
            {"name":"battle.json","type":"Pathing","folderName":"routes","status":"Disabled"}]});
        let file = root.join("ScriptGroup/mixed.json");
        fs::write(&file, group.to_string()).unwrap();
        let result = resolve_local(&root, "混合");
        assert_eq!(result["verdict"], "run");
        assert_eq!(result["requirements"]["scannedFiles"], 1);
        assert_eq!(
            result["requirements"]["containsPartySensitiveActions"],
            false
        );
        assert_eq!(
            serde_json::from_str::<Value>(&fs::read_to_string(&file).unwrap()).unwrap()["projects"]
                [0]["jsScriptSettingsObject"]["partyName"],
            "已有队伍"
        );
        let mut enabled = group;
        enabled["projects"][2]["status"] = json!("Enabled");
        fs::write(file, enabled.to_string()).unwrap();
        let result = resolve_local(&root, "混合");
        assert_eq!(
            result["requirements"]["containsPartySensitiveActions"],
            true
        );
        assert_eq!(result["requirements"]["scannedFiles"], 2);
    }

    #[test]
    fn unreadable_route_requires_inspection_instead_of_a_party_question() {
        let root = requirement_fixture();
        fs::create_dir_all(root.join("AutoPathing/routes")).unwrap();
        fs::write(root.join("AutoPathing/routes/broken.json"), "invalid json").unwrap();
        fs::write(root.join("ScriptGroup/broken.json"),json!({"name":"损坏路线","projects":[{"type":"Pathing","name":"broken.json","folderName":"routes"}]}).to_string()).unwrap();
        let result = resolve_local(&root, "损坏路线");
        assert_eq!(result["verdict"], "inspect");
        assert_eq!(result["requirements"]["status"], "unknown");
        assert_eq!(
            result["requirements"]["containsPartySensitiveActions"],
            false
        );
        assert_eq!(result["requirements"]["inspectionRequired"], true);
    }

    #[test]
    #[ignore = "显式指定真实用户目录的只读判定检查"]
    fn inspect_real_javascript_group_requirements() {
        let root = PathBuf::from(
            std::env::var_os("SLEEPY_DOLL_REQUIREMENTS_USER_ROOT").expect("缺少用户目录"),
        );
        let query = std::env::var("SLEEPY_DOLL_REQUIREMENTS_QUERY").expect("缺少查询目标");
        let result = resolve_local(&root, &query);
        println!(
            "{}",
            json!({"groupName":result["groupName"],"verdict":result["verdict"],"requirements":result["requirements"]})
        );
        assert_eq!(result["verdict"], "run");
        assert_eq!(result["requirements"]["status"], "notApplicable");
        assert_eq!(
            result["requirements"]["containsPartySensitiveActions"],
            false
        );
    }

    #[test]
    fn group_parsing_surfaces_config_slots_purely() {
        // 纯解析测试：不落盘、不删除；group_from_value 是 resolve_local 的真实解析路径。
        let root = Path::new("X:/no-such-user-root");
        let group = group_from_value(
            root,
            PathBuf::from("X:/no-such-user-root/ScriptGroup/g.json"),
            "兽怪暴徒".into(),
            &json!({
                "name": "兽怪暴徒",
                "config": {"pathingConfig": {
                    "enabled": false, "partyName": "蒸发", "hurryOnAvatar": "",
                    "autoFightEnabled": true,
                    "autoFightConfig": {"strategyName": "根据队伍自动选择", "pickDropsAfterFightEnabled": true},
                    "autoFightConfig_default_marker": null
                }},
                "projects": [{"name": "路线A.json", "type": "Pathing", "folderName": "兽怪暴徒"}]
            }),
        );
        assert_eq!(group.config_slots["partyName"], "蒸发");
        assert_eq!(group.config_slots["enabled"], false);
        assert_eq!(group.config_slots["hurryOnAvatar"], "");
        // 未安装的引用（root 不存在）如实进入 missing，不猜测存在。
        assert_eq!(group.missing.len(), 1);
        assert_eq!(group.missing[0], "AutoPathing/兽怪暴徒");
    }

    #[test]
    fn action_classification_separates_battle_special_and_benign() {
        let mut actions = BTreeMap::new();
        for name in [
            "teleport",
            "path",
            "fight",
            "stop_flying",
            "mining",
            "pick_around",
            "force_tp",
        ] {
            actions.insert(name.to_string(), 3);
        }
        let verdict = classify_actions(&actions, false);
        assert!(verdict.battle, "fight action must be classified as battle");
        assert!(
            verdict.special.is_empty(),
            "benign actions must not be special"
        );
        assert!(
            verdict.unrecognized.is_empty(),
            "known benign tokens must not be unrecognized"
        );
        assert_eq!(verdict.status, "preconditionsFound");
        assert!(verdict.party_sensitive_actions);
        actions.insert("linnea_mining".to_string(), 2);
        let verdict = classify_actions(&actions, false);
        assert!(verdict.battle);
        assert_eq!(verdict.special, vec![("linnea_mining".to_string(), 2)]);
        // 只有采集/位移动作的普通路线：不要求配队确认。
        let plain = [
            "teleport",
            "path",
            "mining",
            "stop_flying",
            "四叶印",
            "log_output",
        ]
        .into_iter()
        .map(|name| (name.to_string(), 5))
        .collect();
        let verdict = classify_actions(&plain, false);
        assert!(!verdict.battle && verdict.special.is_empty());
        assert_eq!(verdict.status, "noPreconditions");
        assert!(!verdict.party_sensitive_actions);
        // 未登记动作不冒充良性：unknown 且同样要求确认。
        let odd = {
            let mut map = BTreeMap::new();
            map.insert("some_new_mechanic".to_string(), 1);
            map
        };
        let verdict = classify_actions(&odd, false);
        assert_eq!(verdict.status, "unknown");
        assert!(
            !verdict.party_sensitive_actions,
            "unknown actions require inspection, not an invented party requirement"
        );
        assert!(!verdict.unrecognized.is_empty());
        // 读取不全（文件缺失/超上限）同样不拿 false 冒充无战斗。
        let empty = BTreeMap::new();
        let verdict = classify_actions(&empty, true);
        assert_eq!(verdict.status, "unknown");
        assert!(!verdict.party_sensitive_actions);
        assert!(verdict.inspection_required);
    }

    #[test]
    fn missing_local_target_searches_all_categories_and_continues_with_evidence() {
        for query in ["帮我跑下血斛", "运行下清心", "跑个AAA狗粮批发"] {
            let resolved = resolve_target(json!({"query":query,"verdict":"notFound"}), |args| {
                assert_eq!(args["query"], query);
                assert_eq!(args["category"], "all");
                Ok(json!({"total":1,"items":[{"path":"pathing/地方特产/稻妻/血斛"}],"next":"订阅并运行"}))
            }).unwrap();
            assert_eq!(resolved["verdict"], "resourceFound");
            assert_eq!(resolved["lookupScope"], "localAndCentralRepository");
            assert_eq!(
                resolved["repository"]["items"][0]["path"],
                "pathing/地方特产/稻妻/血斛"
            );
        }
    }

    #[test]
    fn existing_local_target_does_not_refresh_or_repeat_repository_lookup() {
        for verdict in ["run", "repair", "create", "ambiguous"] {
            let resolved = resolve_target(json!({"query":"血斛","verdict":verdict}), |_| {
                panic!("本地已找到资源，不应查仓库")
            })
            .unwrap();
            assert_eq!(resolved["verdict"], verdict);
        }
    }

    #[test]
    fn not_found_requires_a_successful_repository_query() {
        let local = json!({"query":"血斛","verdict":"notFound"});
        let empty = resolve_target(local.clone(), |_| Ok(json!({"total":0,"items":[]}))).unwrap();
        assert_eq!(empty["verdict"], "notFound");
        assert_eq!(empty["lookupScope"], "localAndCentralRepository");
        let failed = resolve_target(local.clone(), |_| Err(Error::Http("断线".into()))).unwrap();
        assert_eq!(failed["verdict"], "lookupFailed");
        assert_eq!(failed["lookupScope"], "local");
        let invalid = resolve_target(local.clone(), |_| Ok(json!({}))).unwrap();
        assert_eq!(invalid["verdict"], "lookupFailed");
        assert!(matches!(
            resolve_target(local, |_| Err(Error::Cancelled)),
            Err(Error::Cancelled)
        ));
    }
}
