//! 用本机 User 目录判定目标是否可运行。
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// 明确的战斗动作：出现即认为路线含战斗，运行前需要配队确认。
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
            local["next"] = if found {
                repository["next"].clone()
            } else {
                json!(
                    "本机与当前全仓索引均未命中。核对目标名称；索引需要更新时只刷新一次再查询，不扫描程序集，不把查询失败当成不存在。"
                )
            };
            local["repository"] = repository;
        }
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(error) => {
            local["verdict"] = json!("lookupFailed");
            local["repositoryError"] = json!(error.to_string());
            local["next"] = json!(
                "仅本机未命中，中央仓库查询失败，目标是否存在尚未确定。处理 repositoryError 后重试，不声称仓库没有资源。"
            );
        }
    }
    Ok(local)
}

/// 判定采集或运行目标的入口：一次扫描配置组与 AutoPathing 目录名，不打开路线 JSON。
pub fn resolve_local(root: &Path, query: &str) -> Value {
    let query = query.trim();
    let groups = scan_groups(root);
    let matched_groups = groups
        .iter()
        .filter(|group| {
            matches_query(query, &group.name)
                || group
                    .projects
                    .iter()
                    .any(|project| matches_query(query, &project.name))
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut candidates = scan_pathing(root, query);
    candidates.extend(scan_javascript(root, query));

    if !matched_groups.is_empty() {
        let runnable = matched_groups
            .iter()
            .filter(|group| group.missing.is_empty() && !group.projects.is_empty())
            .collect::<Vec<_>>();
        if runnable.len() == 1 {
            let group = runnable[0];
            // 需求聚合按组内已启用项目真实引用的文件计算（JS 项目指向其目录，
            // 不拿中文 manifest 名当文件名）。
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
        let requirements = scan_requirements(root, &files);
        return report_requirements(query, "create", None, &[], &candidates, &[], requirements);
    }
    report_requirements(query, "notFound", None, &[], &[], &[], Value::Null)
}

/// 组内已启用项目的需求文件清单：Pathing 指向路线 JSON；Javascript 指向
/// 脚本目录（manifest 在目录里），不把项目显示名当文件名。
fn project_requirement_files(root: &Path, group: &Group) -> Vec<PathBuf> {
    group
        .projects
        .iter()
        .filter(|project| project.enabled)
        .map(|project| {
            let base = root.join(resource_path(&project.kind, &project.folder_name));
            if project.kind.eq_ignore_ascii_case("Pathing") {
                base.join(&project.name)
            } else {
                base
            }
        })
        .collect()
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
/// unknown，不拿 false 冒充无战斗；unknown 同样要求配队确认（fail closed）。
fn scan_requirements(root: &Path, files: &[PathBuf]) -> Value {
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
    let required = verdict.party_confirmation_required;
    json!({
        "status": verdict.status,
        "battle": verdict.battle,
        "specialActions": verdict.special.iter().map(|(name, count)| json!({"action":name,"count":count})).collect::<Vec<_>>(),
        "unrecognizedActions": verdict.unrecognized.iter().map(|(name, count)| json!({"action":name,"count":count})).collect::<Vec<_>>(),
        "partyConfirmationRequired": required,
        "scannedFiles": scanned,
        "unreadableFiles": unreadable,
        "truncated": truncated,
        "sources": sources,
        "note": if required {
            "由路线文件动作聚合的程序侧事实；battle/special/unknown 任一成立都要求运行前用 user.ask 问清缺的配队信息（用户已给的队名/队员不重复问），未答复不得启动。"
        } else {
            "由路线文件动作聚合的程序侧事实；全部文件已读且未出现战斗/特殊/未识别动作。"
        },
    })
}

/// 纯分类：battle 命中即战斗；已知特殊机制单列；未登记动作不冒充良性，
/// 与读取不全一起归入 unknown，同样要求配队确认。
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
    let required = battle || !special.is_empty() || !unrecognized.is_empty() || incomplete;
    let status = if battle || !special.is_empty() {
        "preconditionsFound"
    } else if required {
        "unknown"
    } else {
        "noPreconditions"
    };
    ActionVerdict {
        status: status.to_string(),
        battle,
        special,
        unrecognized,
        party_confirmation_required: required,
    }
}

struct ActionVerdict {
    status: String,
    battle: bool,
    special: Vec<(String, usize)>,
    unrecognized: Vec<(String, usize)>,
    party_confirmation_required: bool,
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
        .filter(|project| !project.exists)
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
    let party_required = requirements["partyConfirmationRequired"] == true;
    let mut next = next_action(verdict).to_string();
    if party_required && matches!(verdict, "run" | "create") {
        next.push_str(
            " 该目标含战斗/特殊机制前提，按序执行：缺配队信息先用 user.ask 问清（已给的队名/队员不重复问，未答复不启动）；配队已明确后用 bgi.inspect_group_effective 读实际生效配置来源与策略（file 策略读原文要求，auto 策略按该队确认可用策略），按角色与策略要求适配现成组并回读核验；以上完成才 bgi.run_script_group。普通采集路线不额外发问。",
        );
    }
    json!({
        "query": query,
        "verdict": verdict,
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
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "next": next,
    })
}

fn next_action(verdict: &str) -> &'static str {
    match verdict {
        "run" => {
            "组引用完整；先比对用户目标与返回的 config 槽位（目标含队伍/赶路/参数变更时先用 bgi.set_pathing_party 写入回读）。不重复搜索或读取路线叶子文件。"
        }
        "repair" => "只补 missing 里的路径（更新/订阅），不要重写无关配置，补完后再 resolve",
        "create" => {
            "本机已有资源；Pathing 用完整父目录准备配置组，不读取叶子 JSON；Javascript 先 inspect_script 读取 manifest 指定的设置定义，再用 bgi.prepare_js_group，使用返回的 groupName 运行。"
        }
        "ambiguous" => "只问真正不同的配置组，不要并列所有路线文件",
        _ => {
            "仅本机未安装，不代表仓库不存在。采集/地图追踪用 bgi.repo.search category=pathing（或 all）查询完整父节点；已有索引先查询，不先反复刷新。选择作者包后直接 describe/invoke bgi.subscribe_script_resources、bgi.prepare_pathing_group；游戏就绪再 bgi.run_script_group。不要扫描软件目录或桥程序集。"
        }
    }
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
        // run 只是"组完整可运行"；next 必须要求先比对目标与配置，不能跳回"直接运行"。
        let next = next_action("run");
        assert!(
            next.contains("比对"),
            "next must ask for goal/config comparison: {next}"
        );
        assert!(
            !next.starts_with("直接运行"),
            "next must not tell the model to run directly"
        );
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
        assert!(verdict.party_confirmation_required);
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
        assert!(!verdict.party_confirmation_required);
        // 未登记动作不冒充良性：unknown 且同样要求确认。
        let odd = {
            let mut map = BTreeMap::new();
            map.insert("some_new_mechanic".to_string(), 1);
            map
        };
        let verdict = classify_actions(&odd, false);
        assert_eq!(verdict.status, "unknown");
        assert!(
            verdict.party_confirmation_required,
            "unrecognized action must require confirmation"
        );
        assert!(!verdict.unrecognized.is_empty());
        // 读取不全（文件缺失/超上限）同样不拿 false 冒充无战斗。
        let empty = BTreeMap::new();
        let verdict = classify_actions(&empty, true);
        assert_eq!(verdict.status, "unknown");
        assert!(verdict.party_confirmation_required);
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
