//! 用本机 User 目录判定「能不能跑」，避免模型自己搜接口、读遍路线。
use std::{
    fs,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

/// 采集/跑配置组的入口：一次扫描配置组和 AutoPathing 目录名，不打开路线 JSON。
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
    let candidates = scan_pathing(root, query);

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
        "create" => "用 candidates 的父节点建配置组，不要读取叶子 JSON",
        "ambiguous" => "只问真正不同的配置组，不要并列所有路线文件",
        _ => "目标在本地不存在，再考虑更新仓库或向用户确认名称",
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
    let relative = resource_path(&kind, &folder_name);
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
    use std::fs;

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn user_tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("ScriptGroup/霜仙花.json"),
            r#"{"name":"霜仙花","index":1,"projects":[{"name":"霜仙花","type":"Pathing","folderName":"地方特产/霜仙花"}]}"#,
        );
        write(
            &dir.path()
                .join("AutoPathing/地方特产/霜仙花/霜仙花-蒙德.json"),
            r#"{"info":{"name":"should-not-be-read"}}"#,
        );
        dir
    }

    #[test]
    fn existing_group_with_files_is_runnable() {
        let dir = user_tree();
        let result = resolve_local(dir.path(), "帮我跑下霜仙花");
        assert_eq!(result["verdict"], "run");
        assert_eq!(result["groupName"], "霜仙花");
        assert_eq!(result["missing"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn group_without_pathing_files_is_repair() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("ScriptGroup/霜仙花.json"),
            r#"{"name":"霜仙花","index":1,"projects":[{"name":"霜仙花","type":"Pathing","folderName":"地方特产/霜仙花"}]}"#,
        );
        let result = resolve_local(dir.path(), "霜仙花");
        assert_eq!(result["verdict"], "repair");
        assert_eq!(result["missing"][0], "AutoPathing/地方特产/霜仙花");
        assert!(
            missing_paths_for_group(dir.path(), "霜仙花")
                .unwrap()
                .iter()
                .any(|path| path.contains("霜仙花"))
        );
    }

    #[test]
    fn local_pathing_without_group_is_create() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path()
                .join("AutoPathing/地方特产/霜仙花/霜仙花@白白喵/a.json"),
            "{}",
        );
        let result = resolve_local(dir.path(), "霜仙花");
        assert_eq!(result["verdict"], "create");
        assert_eq!(
            result["candidates"][0]["path"],
            "AutoPathing/地方特产/霜仙花"
        );
        assert_eq!(result["candidates"][0]["children"][0], "霜仙花@白白喵");
    }

    #[test]
    fn resolve_does_not_open_route_json() {
        let dir = tempfile::tempdir().unwrap();
        let route = dir.path().join("AutoPathing/地方特产/清心/清心.json");
        write(&route, "this is not json and must not be parsed");
        let result = resolve_local(dir.path(), "清心");
        assert_eq!(result["verdict"], "create");
    }
}
