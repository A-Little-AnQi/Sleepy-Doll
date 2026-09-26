//! 用本机 User 目录判定目标是否可运行。
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{Error, Result};
use serde_json::{Value, json};

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
            return report(
                query,
                "run",
                Some(&group.name),
                &[],
                &candidates,
                &matched_groups,
            );
        }
        if runnable.len() > 1 {
            return report(query, "ambiguous", None, &[], &candidates, &matched_groups);
        }
        let missing = matched_groups
            .iter()
            .flat_map(|group| group.missing.iter().cloned())
            .collect::<Vec<_>>();
        return report(
            query,
            "repair",
            matched_groups.first().map(|group| group.name.as_str()),
            &missing,
            &candidates,
            &matched_groups,
        );
    }

    if !candidates.is_empty() {
        return report(query, "create", None, &[], &candidates, &[]);
    }
    report(query, "notFound", None, &[], &[], &[])
}

pub fn missing_paths_for_group(root: &Path, group_name: &str) -> Option<Vec<String>> {
    let needle = normalize(group_name);
    scan_groups(root)
        .into_iter()
        .find(|group| normalize(&group.name) == needle)
        .map(|group| group.missing)
}

fn report(
    query: &str,
    verdict: &str,
    group_name: Option<&str>,
    missing: &[String],
    candidates: &[Candidate],
    groups: &[Group],
) -> Value {
    json!({
        "query": query,
        "verdict": verdict,
        "groupName": group_name,
        "missing": missing,
        "candidates": candidates.iter().map(|candidate| json!({
            "path": path_display(&candidate.path),
            "children": candidate.children,
            "kind": candidate.kind,
        })).collect::<Vec<_>>(),
        "groups": groups.iter().map(|group| json!({
            "name": group.name,
            "file": path_display(&group.file),
            "missing": group.missing,
            "projects": group.projects.iter().map(|project| json!({
                "name": project.name,
                "type": project.kind,
                "folderName": project.folder_name,
                "exists": project.exists,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "next": next_action(verdict),
    })
}

fn next_action(verdict: &str) -> &'static str {
    match verdict {
        "run" => "直接运行该配置组，不要再搜索或读取路线文件",
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

#[derive(Clone)]
struct Group {
    name: String,
    file: PathBuf,
    projects: Vec<Project>,
    missing: Vec<String>,
}

#[derive(Clone)]
struct Project {
    name: String,
    kind: String,
    folder_name: String,
    exists: bool,
}

struct Candidate {
    path: PathBuf,
    children: Vec<String>,
    kind: &'static str,
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
        groups.push(Group {
            name,
            file: path,
            projects,
            missing,
        });
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
    Some(Project {
        name: value["name"].as_str().unwrap_or(&folder_name).to_owned(),
        kind,
        folder_name,
        exists,
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
