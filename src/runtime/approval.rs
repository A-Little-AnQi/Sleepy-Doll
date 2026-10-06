//! 通用审批呈现纯 helper：把工具调用的 method/args 变成用户可读的
//! {title, summary, changes:[{label,value}]}，不读磁盘、不做外部调用。

use serde_json::Value;

const MAX_CHANGES: usize = 40;
const MAX_VALUE_CHARS: usize = 300;

/// 叶字段中文映射；未命中的保留完整 path。
const FIELD_LABELS: &[(&str, &str)] = &[
    ("enableCombatTargeting", "战斗中持续索敌"),
    ("lockLostWaitTime", "脱锁等待时间（秒）"),
    ("fastCheckEnabled", "更快触发战斗结束检查"),
    ("fastCheckParams", "快速检查触发条件"),
    ("checkEndDelay", "检查战斗结束的延时"),
    ("beforeDetectDelay", "按键触发后检查延时"),
    (
        "blockCheckBeforeBattleSeconds",
        "战斗开始后禁止结束检查时长（秒）",
    ),
    ("partyName", "队伍"),
    ("hurryAvatar", "赶路角色"),
    ("combatScriptName", "战斗策略"),
];

const SENSITIVE_MARKERS: &[&str] = &[
    "token",
    "password",
    "secret",
    "apikey",
    "cookie",
    "authorization",
    "key",
];

/// generic 参数名中文映射；未命中的保留原始参数名。
const GENERIC_LABELS: &[(&str, &str)] = &[
    ("path", "路径"),
    ("file", "文件"),
    ("file_path", "文件路径"),
    ("groupName", "组名"),
    ("command", "命令"),
    ("backup", "备份"),
    ("jobId", "任务 ID"),
    ("title", "标题"),
    ("name", "名称"),
    ("recursive", "递归删除"),
];

/// 不参与中文映射、原样保留的展示 label。
fn is_pseudo_label(label: &str) -> bool {
    label == "写入内容" || label.ends_with("片段")
}

/// 基线校验通过的读取结果（read 内容含 sha256/truncated/text）；
/// 仅 expected 存在、sha256 EXACT 匹配、truncated==false 且 text 存在才返回 Some。
pub fn checked_previous<'a>(read: &'a Value, expected: Option<&str>) -> Option<&'a str> {
    let expected = expected?;
    let obj = read.as_object()?;
    let sha = obj.get("sha256").and_then(Value::as_str)?;
    if sha != expected {
        return None;
    }
    if obj.get("truncated").and_then(Value::as_bool) != Some(false) {
        return None;
    }
    obj.get("text").and_then(Value::as_str)
}

pub fn presentation(
    method: &str,
    args: &Value,
    label: &str,
    _description: &str,
    previous: Option<&str>,
) -> Value {
    match method {
        "bgi.user.write" | "workspace.write" => write_presentation(args, previous),
        "workspace.shell" => shell_presentation(args),
        _ => generic_presentation(method, args, label),
    }
}

/// 末尾句号规范：已完整结束的句子不补第二个句号。
fn end_with_period(s: &str) -> String {
    let t = s.trim_end();
    if t.ends_with('。') || t.ends_with('！') || t.ends_with('？') {
        t.to_string()
    } else {
        format!("{}。", t)
    }
}

// ===== workspace.shell：真实 cwd + 命令 =====

fn shell_presentation(args: &Value) -> Value {
    let command = args.get("command").and_then(Value::as_str).unwrap_or("");
    let cwd = args.get("cwd").and_then(Value::as_str);
    let summary = match cwd {
        Some(cwd) => format!("在{}运行以下命令。", cwd),
        None => "运行以下命令。".to_string(),
    };
    let mut changes = vec![("命令".to_string(), clip_text(command))];
    if let Some(cwd) = cwd {
        changes.push(("工作目录".to_string(), clip_text(cwd)));
    }
    serde_json::json!({
        "title": "运行命令",
        "summary": summary,
        "changes": changes_json(&changes),
    })
}

fn is_sensitive(path: &str) -> bool {
    let lower = path.to_lowercase();
    SENSITIVE_MARKERS.iter().any(|m| lower.contains(m))
}

fn str_arg<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
}

/// 仅展示用途的 JSON 解析：容忍 BOM 前缀，不修改原文本（args 与 sha 不受影响）。
fn parse_json_bom(text: &str) -> Option<Value> {
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn short_len_label(text: &str) -> String {
    format!("{} 字符", text.chars().count())
}

/// 值文本：超过 MAX_VALUE_CHARS 个 unicode 字符截断并注明省略。
fn clip_text(text: &str) -> String {
    if text.chars().count() <= MAX_VALUE_CHARS {
        text.to_string()
    } else {
        let head: String = text.chars().take(MAX_VALUE_CHARS).collect();
        format!("{}…（已省略其余内容）", head)
    }
}

fn scalar_repr(v: &Value) -> String {
    match v {
        Value::Null => "空值".to_string(),
        Value::Bool(true) => "开".to_string(),
        Value::Bool(false) => "关".to_string(),
        Value::String(s) if s.is_empty() => "（空）".to_string(),
        Value::String(s) => clip_text(s),
        other => clip_text(&other.to_string()),
    }
}

/// 把 path 拆成段：`a.projects[2].c` → ["a", "projects", "[2]", "c"]。
fn split_path(path: &str) -> Vec<String> {
    let mut segs = Vec::new();
    for part in path.split('.') {
        let mut rest = part;
        while let Some(i) = rest.find('[') {
            let (head, tail) = rest.split_at(i);
            if !head.is_empty() {
                segs.push(head.to_string());
            }
            if let Some(close) = tail.find(']') {
                segs.push(format!("[{}]", &tail[1..close]));
                rest = &tail[close + 1..];
            } else {
                segs.push(tail.to_string());
                rest = "";
                break;
            }
        }
        if !rest.is_empty() {
            segs.push(rest.to_string());
        }
    }
    segs
}

/// 展示层 label：叶字段命中中文映射时只保留 projects[i]（任务 N）上下文，
/// 不保留 config.pathingConfig 等技术前缀；未知路径完整保留。
fn field_label(path: &str) -> String {
    let segs = split_path(path);
    let leaf = match segs.last() {
        Some(l) => l.as_str(),
        None => return path.to_string(),
    };
    let zh = match FIELD_LABELS.iter().find(|(k, _)| *k == leaf) {
        Some((_, zh)) => *zh,
        None => return path.to_string(),
    };
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i + 1 < segs.len() {
        if segs[i] == "projects" {
            if let Some(idx) = segs[i + 1]
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .and_then(|s| s.parse::<usize>().ok())
            {
                parts.push(format!("任务{}", idx + 1));
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    parts.push(zh.to_string());
    parts.join(" · ")
}

/// 单个叶项值：敏感 path 遮值，保留由外层拼接的新增/删除/修改语义。
fn display_leaf(path: &str, repr: &str) -> String {
    if is_sensitive(path) {
        "******（已遮蔽）".to_string()
    } else {
        repr.to_string()
    }
}

/// 敏感字段遮值；不修改原 args，只改展示层。
fn mask_sensitive(changes: Vec<(String, String)>) -> Vec<(String, String)> {
    changes
        .into_iter()
        .map(|(label, value)| {
            if is_sensitive(&label) {
                (label, "******（已遮蔽）".to_string())
            } else {
                (label, value)
            }
        })
        .collect()
}

/// 把 JSON 值递归展开到叶项：(path, scalar_repr)。
/// 空对象/空数组自身作为叶项记为「（空）」。
fn collect_leaves(v: &Value, path: String, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(m) => {
            if m.is_empty() {
                out.push((path, "（空）".to_string()));
                return;
            }
            for (k, child) in m {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                collect_leaves(child, p, out);
            }
        }
        Value::Array(a) => {
            if a.is_empty() {
                out.push((path, "（空）".to_string()));
                return;
            }
            for (i, child) in a.iter().enumerate() {
                collect_leaves(child, format!("{}[{}]", path, i), out);
            }
        }
        scalar => out.push((path, scalar_repr(scalar))),
    }
}

/// 递归 diff：含删除/新增/数组逐索引/null ≠ missing；
/// 新增或删除的父对象/数组也递归到叶项，避免整份 JSON 原文泄露。
fn diff_json(old: &Value, new: &Value, path: String, out: &mut Vec<(String, String)>) {
    match (old, new) {
        (Value::Object(o), Value::Object(n)) => {
            let mut keys: Vec<&String> = o.keys().chain(n.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", path, k)
                };
                match (o.get(k), n.get(k)) {
                    (Some(a), Some(b)) => diff_json(a, b, child, out),
                    (Some(a), None) => {
                        let mut leaves = Vec::new();
                        collect_leaves(a, child, &mut leaves);
                        for (p, r) in leaves {
                            let r = display_leaf(&p, &r);
                            out.push((p, format!("{} →（删除）", r)));
                        }
                    }
                    (None, Some(b)) => {
                        let mut leaves = Vec::new();
                        collect_leaves(b, child, &mut leaves);
                        for (p, r) in leaves {
                            let r = display_leaf(&p, &r);
                            out.push((p, format!("（无）→ {}", r)));
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            let len = a.len().max(b.len());
            for i in 0..len {
                let child = format!("{}[{}]", path, i);
                match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => diff_json(x, y, child, out),
                    (Some(x), None) => {
                        let mut leaves = Vec::new();
                        collect_leaves(x, child, &mut leaves);
                        for (p, r) in leaves {
                            let r = display_leaf(&p, &r);
                            out.push((p, format!("{} →（删除）", r)));
                        }
                    }
                    (None, Some(y)) => {
                        let mut leaves = Vec::new();
                        collect_leaves(y, child, &mut leaves);
                        for (p, r) in leaves {
                            let r = display_leaf(&p, &r);
                            out.push((p, format!("（无）→ {}", r)));
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (a, b) => {
            if a.is_object() || a.is_array() || b.is_object() || b.is_array() {
                // 类型改变（如字符串 → 对象）：旧值叶项记删除、新值叶项记新增，
                // 不把整个对象/数组当字符串泄露原文。
                let mut old_leaves = Vec::new();
                collect_leaves(a, path.clone(), &mut old_leaves);
                for (p, r) in old_leaves {
                    let p = if p.is_empty() {
                        "（根）".to_string()
                    } else {
                        p
                    };
                    let r = display_leaf(&p, &r);
                    out.push((p, format!("{} →（删除）", r)));
                }
                let mut new_leaves = Vec::new();
                collect_leaves(b, path, &mut new_leaves);
                for (p, r) in new_leaves {
                    let p = if p.is_empty() {
                        "（根）".to_string()
                    } else {
                        p
                    };
                    let r = display_leaf(&p, &r);
                    out.push((p, format!("（无）→ {}", r)));
                }
            } else if a != b {
                let label = if path.is_empty() {
                    "（根）".to_string()
                } else {
                    path
                };
                let old_r = display_leaf(&label, &scalar_repr(a));
                let new_r = display_leaf(&label, &scalar_repr(b));
                out.push((label, format!("{} → {}", old_r, new_r)));
            }
        }
    }
}

fn changes_json(changes: &[(String, String)]) -> Value {
    serde_json::json!(
        changes
            .iter()
            .map(|(label, value)| serde_json::json!({"label": label, "value": value}))
            .collect::<Vec<_>>()
    )
}

// ===== write 类方法（bgi.user.write / workspace.write）=====

fn write_presentation(args: &Value, previous: Option<&str>) -> Value {
    let path = str_arg(args, &["path", "file", "file_path"]).unwrap_or("");
    let title = "修改配置文件".to_string();

    let content = args.get("content").and_then(|v| v.as_str());

    let (summary, changes) = match (previous, content) {
        (Some(prev), Some(new_text)) => {
            let old_value = parse_json_bom(prev);
            let new_value = parse_json_bom(new_text);
            match (old_value, new_value) {
                (Some(o), Some(n)) => {
                    // diff_json 已按真实分支逐叶遮蔽并保留新增/删除语义，不再整 value 二次覆盖
                    let mut changes = Vec::new();
                    diff_json(&o, &n, String::new(), &mut changes);
                    let total = changes.len();
                    changes.truncate(MAX_CHANGES);
                    let summary = format!(
                        "写入 {}：更新配置内容，共 {} 处变化{}。",
                        path,
                        total,
                        if total > MAX_CHANGES {
                            format!("，另有 {} 项省略", total - MAX_CHANGES)
                        } else {
                            String::new()
                        }
                    );
                    (summary, changes)
                }
                // previous 可靠但不是 JSON：展示真实前后片段，不假称未核对旧值。
                _ => {
                    let summary = format!(
                        "写入 {}：以新内容（{}）替换原内容（{}），如下展示替换前后片段。",
                        path,
                        short_len_label(new_text),
                        short_len_label(prev)
                    );
                    let changes = vec![
                        ("原内容片段".to_string(), clip_text(prev)),
                        ("新内容片段".to_string(), clip_text(new_text)),
                    ];
                    (summary, changes)
                }
            }
        }
        (None, Some(new_text)) => {
            if new_text.trim().is_empty() {
                (format!("写入 {}：清空文件内容。", path), Vec::new())
            } else if let Some(v) = parse_json_bom(new_text) {
                // 无基线时递归到叶项，不把整份 JSON 当字符串展示。
                let mut changes = Vec::new();
                collect_leaves(&v, String::new(), &mut changes);
                let total = changes.len();
                changes = mask_sensitive(changes);
                // 每项标注「将写入：」前缀，无箭头（没有旧值可对比）
                changes = changes
                    .into_iter()
                    .map(|(l, v)| (l, format!("将写入：{}", v)))
                    .collect();
                changes.truncate(MAX_CHANGES);
                let summary = format!(
                    "写入 {}：待写入以下 JSON 配置内容，共 {} 项{}。",
                    path,
                    total,
                    if total > MAX_CHANGES {
                        format!("，另有 {} 项省略", total - MAX_CHANGES)
                    } else {
                        String::new()
                    }
                );
                (summary, changes)
            } else {
                let summary = format!(
                    "写入 {}：待写入以下内容（{}）。",
                    path,
                    short_len_label(new_text)
                );
                let changes = mask_sensitive(vec![("写入内容".to_string(), clip_text(new_text))]);
                (summary, changes)
            }
        }
        (_, None) => {
            let summary = format!("写入 {}：调用未提供 content，无法展示写入影响。", path);
            (summary, Vec::new())
        }
    };

    // 中文映射作为展示 label（遮蔽判定基于原始 path，已在 mask 前完成）
    let changes: Vec<(String, String)> = changes
        .into_iter()
        .map(|(label, value)| {
            if is_pseudo_label(&label) {
                (label, value)
            } else {
                (field_label(&label), value)
            }
        })
        .collect();

    serde_json::json!({
        "title": title,
        "summary": summary,
        "changes": changes_json(&changes),
    })
}

// ===== 其它方法的通用 fallback =====

fn generic_presentation(method: &str, args: &Value, label: &str) -> Value {
    let mut changes: Vec<(String, String)> = Vec::new();

    // groupNames 数组单独逐项展开，不再以原始形式重复列出。
    let group_names: Option<Vec<String>> =
        args.get("groupNames").and_then(|v| v.as_array()).map(|a| {
            a.iter()
                .filter_map(|n| n.as_str().map(str::to_string))
                .collect()
        });

    let mut covered: Vec<&str> = Vec::new();
    if group_names.is_some() {
        covered.push("groupNames");
    }

    // 未知名参数保持原始 path，遮蔽与中文映射都基于完整 path 进行。
    let mut raw_path_labels: Vec<String> = Vec::new();

    // 先列已知参数（中文 label），再列其余未覆盖参数，风险相关参数保持可见。
    let known: Vec<&str> = GENERIC_LABELS.iter().map(|(k, _)| *k).collect();
    for key in known.iter().copied().chain(
        args.as_object()
            .map(|m| m.keys().map(|k| k.as_str()).collect::<Vec<_>>())
            .unwrap_or_default(),
    ) {
        if covered.contains(&key) {
            continue;
        }
        if let Some(v) = args.get(key) {
            let zh = GENERIC_LABELS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, zh)| *zh)
                .unwrap_or(key);
            if (v.is_object() || v.is_array()) && zh == key {
                // 未知名对象/数组：递归到叶项，避免整份原文泄露嵌套敏感字段
                let mut leaves = Vec::new();
                collect_leaves(v, key.to_string(), &mut leaves);
                for (p, r) in leaves {
                    raw_path_labels.push(p.clone());
                    changes.push((p, r));
                }
            } else {
                changes.push((zh.to_string(), scalar_repr(v)));
            }
            covered.push(key);
        }
    }

    if let Some(names) = &group_names {
        for (i, n) in names.iter().enumerate() {
            changes.push((format!("组名[{}]", i), clip_text(n)));
        }
    }

    // 先按完整 path 遮蔽，再对未知名叶项做中文映射（未命中映射保留完整 path）
    changes = mask_sensitive(changes);
    changes = changes
        .into_iter()
        .map(|(l, v)| {
            if raw_path_labels.contains(&l) {
                (field_label(&l), v)
            } else {
                (l, v)
            }
        })
        .collect();
    let total = changes.len();
    changes.truncate(MAX_CHANGES);

    // summary 不使用 ToolDefinition.description：那是给模型的内部说明，
    // 可能含开发指令，不能出现在用户面前；只用真实 label 与对象信息。
    let title = {
        let t = label.trim();
        if t.is_empty() {
            format!("调用 {}", method)
        } else {
            t.to_string()
        }
    };
    let core = title.trim().to_string();
    let object = str_arg(
        args,
        &[
            "path",
            "file",
            "file_path",
            "groupName",
            "name",
            "title",
            "jobId",
        ],
    );
    let mut s = match object {
        Some(obj) => format!("{}，目标：{}", core, clip_text(obj)),
        None => core,
    };
    if total > MAX_CHANGES {
        s = format!("{}，另有 {} 项省略", s, total - MAX_CHANGES);
    }
    let summary = end_with_period(&s);

    serde_json::json!({
        "title": title,
        "summary": summary,
        "changes": changes_json(&changes),
    })
}

// ===== tests =====

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn changes_of(v: &Value) -> Vec<(String, String)> {
        v["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["label"].as_str().unwrap().to_string(),
                    c["value"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn json_diff_change_add_delete_null() {
        let old = json!({"partyName": "一队", "hurryAvatar": null, "gone": 1, "same": true});
        let new = json!({"partyName": "二队", "hurryAvatar": false, "added": 5, "same": true});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "a.json", "content": new.to_string()}),
            "写入",
            "写入文件",
            Some(&old.to_string()),
        );
        assert_eq!(out["title"], "修改配置文件");
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "队伍" && v == "一队 → 二队"));
        // null（空值）与 false（关）语义不同，各自有中文展示
        assert!(ch.iter().any(|(l, v)| l == "赶路角色" && v == "空值 → 关"));
        assert!(ch.iter().any(|(l, v)| l == "gone" && v == "1 →（删除）"));
        assert!(ch.iter().any(|(l, v)| l == "added" && v == "（无）→ 5"));
        assert!(!ch.iter().any(|(l, _)| l == "same"));
    }

    #[test]
    fn array_indexwise_diff() {
        let old = json!({"list": [1, 2, 3]});
        let new = json!({"list": [1, 9]});
        let out = presentation(
            "workspace.write",
            &json!({"path": "b.json", "content": new.to_string()}),
            "写入",
            "",
            Some(&old.to_string()),
        );
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "list[1]" && v == "2 → 9"));
        assert!(ch.iter().any(|(l, v)| l == "list[2]" && v == "3 →（删除）"));
    }

    #[test]
    fn multibyte_truncation_is_char_safe() {
        let long = "战斗".repeat(200); // 400 个多字节字符
        let new = json!({"combatScriptName": long});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "c.json", "content": new.to_string()}),
            "写入",
            "",
            None,
        );
        let s = serde_json::to_string(&out).unwrap();
        // 截断必须发生在字符边界（无效 UTF-8 会导致 to_string panic 或 replacement char）
        assert!(!s.contains('\u{FFFD}'));
        let ch = changes_of(&out);
        let v = &ch.iter().find(|(l, _)| l == "战斗策略").unwrap().1;
        assert!(
            v.chars().count()
                <= "将写入：".chars().count()
                    + MAX_VALUE_CHARS
                    + "…（已省略其余内容）".chars().count()
        );
        assert!(v.starts_with("将写入："));
        assert!(v.ends_with("…（已省略其余内容）"));
    }

    #[test]
    fn sensitive_field_is_masked_and_args_untouched() {
        let old = json!({"apiToken": "abc", "nested": {"password": "x"}});
        let new = json!({"apiToken": "secret-new", "nested": {"password": "y"}});
        let args = json!({"path": "settings.json", "content": new.to_string()});
        let args_snapshot = args.clone();
        let out = presentation("bgi.user.write", &args, "写入", "", Some(&old.to_string()));
        assert_eq!(args, args_snapshot);
        let s = serde_json::to_string(&out).unwrap();
        assert!(s.contains("已遮蔽"));
        assert!(!s.contains("secret-new"));
    }

    // diff 新增父对象必须递归到叶并遮蔽敏感字段，不整份原文泄露
    #[test]
    fn diff_added_parent_object_recurses_and_masks() {
        let old = json!({});
        let new = json!({"creds": {"password": "hunter2", "apiKey": "k-123"}});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "s.json", "content": new.to_string()}),
            "写入",
            "",
            Some(&old.to_string()),
        );
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("hunter2"));
        assert!(!s.contains("k-123"));
        let ch = changes_of(&out);
        assert!(
            ch.iter()
                .any(|(l, v)| l == "creds.password" && v == "（无）→ ******（已遮蔽）")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "creds.apiKey" && v == "（无）→ ******（已遮蔽）")
        );
    }

    // 无基线时新增父对象里的敏感字段同样遮蔽
    #[test]
    fn no_baseline_sensitive_leaf_masked() {
        let new = json!({"auth": {"token": "t0ps3cret", "enabled": true}});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "n.json", "content": new.to_string()}),
            "写入",
            "",
            None,
        );
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("t0ps3cret"));
        let ch = changes_of(&out);
        assert!(
            ch.iter()
                .any(|(l, v)| l == "auth.token" && v == "将写入：******（已遮蔽）")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "auth.enabled" && v == "将写入：开")
        );
    }

    // null 与 missing 是两种差异，都必须可见
    #[test]
    fn null_versus_missing_distinction() {
        let old = json!({"a": null, "b": 1});
        let new = json!({"a": 2});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "m.json", "content": new.to_string()}),
            "写入",
            "",
            Some(&old.to_string()),
        );
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "a" && v == "空值 → 2"));
        assert!(ch.iter().any(|(l, v)| l == "b" && v == "1 →（删除）"));

        let out2 = presentation(
            "bgi.user.write",
            &json!({"path": "m.json", "content": json!({"a": null}).to_string()}),
            "写入",
            "",
            Some(&json!({}).to_string()),
        );
        let ch2 = changes_of(&out2);
        assert!(ch2.iter().any(|(l, v)| l == "a" && v == "（无）→ 空值"));
    }

    // 无基线但 content 是 JSON：递归到具体叶项，不整份 JSON 当字符串
    #[test]
    fn no_baseline_json_lists_leaf_items() {
        let new = json!({"checkEndDelay": 300, "enableCombatTargeting": true});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "d.json", "content": new.to_string()}),
            "写入",
            "",
            None,
        );
        let ch = changes_of(&out);
        assert!(out["summary"].as_str().unwrap().contains("待写入"));
        assert!(
            ch.iter()
                .any(|(l, v)| l == "检查战斗结束的延时" && v == "将写入：300")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "战斗中持续索敌" && v == "将写入：开")
        );
        // 不是整份 JSON 字符串
        assert!(!ch.iter().any(|(_, v)| v.contains("{\"")));
    }

    // 超过 40 项：summary 给出准确省略数
    #[test]
    fn no_baseline_over_limit_reports_exact_omission() {
        let mut obj = serde_json::Map::new();
        for i in 0..50 {
            obj.insert(format!("k{:02}", i), json!(i));
        }
        let new = Value::Object(obj);
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "big.json", "content": new.to_string()}),
            "写入",
            "",
            None,
        );
        assert!(out["summary"].as_str().unwrap().contains("另有 10 项省略"));
        assert_eq!(out["changes"].as_array().unwrap().len(), MAX_CHANGES);
    }

    #[test]
    fn projects_index_and_prefix_labels() {
        // 已知字段不保留 config.pathingConfig 前缀；projects[i] 用「任务 i+1 · 中文」
        let old = json!({"config": {"pathingConfig": {"partyName": "一队"}}});
        let new = json!({"config": {"pathingConfig": {"partyName": "二队"}}});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "p.json", "content": new.to_string()}),
            "写入",
            "",
            Some(&old.to_string()),
        );
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "队伍" && v == "一队 → 二队"));

        let old2 = json!({"projects": [{"partyName": "一队"}]});
        let new2 = json!({"projects": [{"partyName": "二队"}]});
        let out2 = presentation(
            "bgi.user.write",
            &json!({"path": "p.json", "content": new2.to_string()}),
            "写入",
            "",
            Some(&old2.to_string()),
        );
        let ch2 = changes_of(&out2);
        assert!(
            ch2.iter()
                .any(|(l, v)| l == "任务1 · 队伍" && v == "一队 → 二队")
        );

        // 未知路径完整保留
        let out3 = presentation(
            "bgi.user.write",
            &json!({"path": "p.json", "content": json!({"weirdPath": 1}).to_string()}),
            "写入",
            "",
            None,
        );
        assert!(
            changes_of(&out3)
                .iter()
                .any(|(l, v)| l == "weirdPath" && v == "将写入：1")
        );
    }

    // previous 可靠但不是 JSON：展示真实前后片段，不假称未核对旧值
    #[test]
    fn non_json_previous_shows_real_snippets() {
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "t.txt", "content": "new plain body"}),
            "写入",
            "",
            Some("old plain body"),
        );
        let summary = out["summary"].as_str().unwrap();
        assert!(!summary.contains("未核对"));
        assert!(summary.contains("替换原内容"));
        let ch = changes_of(&out);
        assert!(
            ch.iter()
                .any(|(l, v)| l == "原内容片段" && v == "old plain body")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "新内容片段" && v == "new plain body")
        );
    }

    #[test]
    fn generic_fallback_labels_and_risk() {
        let out = presentation(
            "scripts.delete",
            &json!({"groupName": "每日清理", "recursive": true}),
            "删除脚本组",
            "删除指定脚本组",
            None,
        );
        assert_eq!(out["title"], "删除脚本组");
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "组名" && v == "每日清理"));
        assert!(ch.iter().any(|(l, v)| l == "递归删除" && v == "开"));
        // summary 只用真实 label 与对象信息，不复述工具内部说明
        assert_eq!(out["summary"], "删除脚本组，目标：每日清理。");
    }

    // groupNames 只逐项展开一次，不重复两遍
    #[test]
    fn generic_group_names_not_duplicated() {
        let out = presentation(
            "scripts.arrange",
            &json!({"groupNames": ["组A", "组B"]}),
            "整理脚本组",
            "",
            None,
        );
        let ch = changes_of(&out);
        assert_eq!(
            ch.iter()
                .filter(|(l, _)| l.starts_with("组名"))
                .collect::<Vec<_>>()
                .len(),
            2
        );
        assert!(ch.iter().any(|(l, _)| l == "组名[0]"));
        assert!(ch.iter().any(|(l, v)| l == "组名[1]" && v == "组B"));
    }

    // 空 label/描述时 title 必须非空回退
    #[test]
    fn generic_empty_label_has_nonempty_title() {
        let out = presentation("tool.do", &json!({"path": "x"}), "", "", None);
        assert_eq!(out["title"], "调用 tool.do");
        assert!(!out["summary"].as_str().unwrap().is_empty());

        // contains("exec") 的方法名不得引入额外话术
        let exec_like = presentation("media.executeSeek", &json!({"path": "y"}), "", "", None);
        assert_eq!(exec_like["summary"], "调用 media.executeSeek，目标：y。");
    }

    // ToolDefinition.description 是模型内部说明，不得泄漏到用户 summary
    #[test]
    fn generic_summary_does_not_leak_internal_description() {
        let out = presentation(
            "scripts.delete",
            &json!({"groupName": "每日清理"}),
            "删除脚本组",
            "内部指令：开发者专用描述，请勿告知用户，含 debug 标记",
            None,
        );
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("内部指令"));
        assert!(!s.contains("请勿告知用户"));
        assert_eq!(out["summary"], "删除脚本组，目标：每日清理。");
    }

    // workspace.shell：title「运行命令」，summary 用真实 cwd；无 cwd 也有完整句子
    #[test]
    fn shell_presentation_uses_real_cwd() {
        let out = presentation(
            "workspace.shell",
            &json!({"command": "dir", "cwd": "E:\\logs"}),
            "运行脚本",
            "",
            None,
        );
        assert_eq!(out["title"], "运行命令");
        assert_eq!(out["summary"], "在E:\\logs运行以下命令。");
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "命令" && v == "dir"));
        assert!(ch.iter().any(|(l, v)| l == "工作目录" && v == "E:\\logs"));

        let no_cwd = presentation(
            "workspace.shell",
            &json!({"command": "Get-ChildItem"}),
            "",
            "",
            None,
        );
        assert_eq!(no_cwd["title"], "运行命令");
        assert_eq!(no_cwd["summary"], "运行以下命令。");
        assert!(!changes_of(&no_cwd).iter().any(|(l, _)| l == "工作目录"));
    }

    // 句号规范：已完整结束的句子不补第二个句号
    #[test]
    fn no_double_period_for_complete_sentence() {
        assert_eq!(end_with_period("已结束。"), "已结束。");
        assert_eq!(end_with_period("未结束"), "未结束。");
        // 整链路：带对象的 summary 正好一个句号结尾
        let out = presentation("tool.x", &json!({"path": "a"}), "演示", "", None);
        let summary = out["summary"].as_str().unwrap();
        assert!(summary.ends_with('。'));
        assert!(!summary.ends_with("。。"));
    }

    #[test]
    fn checked_previous_requires_exact_sha_and_untruncated_text() {
        let expected = "aa".to_string();
        let read = json!({
            "sha256": "aa",
            "truncated": false,
            "text": "{\"a\":1}",
        });
        assert_eq!(checked_previous(&read, Some(&expected)), Some("{\"a\":1}"));

        // sha 不匹配
        assert_eq!(checked_previous(&read, Some("bb")), None);
        // truncated 缺失 / true
        assert_eq!(
            checked_previous(&json!({"sha256": "aa", "text": "x"}), Some(&expected)),
            None
        );
        assert_eq!(
            checked_previous(
                &json!({"sha256": "aa", "truncated": true, "text": "x"}),
                Some(&expected)
            ),
            None
        );
        // text 缺失
        assert_eq!(
            checked_previous(
                &json!({"sha256": "aa", "truncated": false}),
                Some(&expected)
            ),
            None
        );
        // expected 缺失
        assert_eq!(checked_previous(&read, None), None);
    }

    // generic 未知名对象参数：递归到叶并按完整 path 遮蔽，嵌套 password 不泄露
    #[test]
    fn generic_unknown_object_param_masks_nested_secret() {
        let out = presentation(
            "tool.deploy",
            &json!({"path": "x", "options": {"password": "p@ss", "retries": 3}}),
            "部署",
            "",
            None,
        );
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("p@ss"));
        let ch = changes_of(&out);
        assert!(
            ch.iter()
                .any(|(l, v)| l == "options.password" && v == "******（已遮蔽）")
        );
        assert!(ch.iter().any(|(l, v)| l == "options.retries" && v == "3"));
    }

    // diff 类型改变（字符串 → 对象）：拆成旧叶删除、新叶新增，秘密值不进输出
    #[test]
    fn diff_type_change_splits_leaves_without_leak() {
        let old = json!({"auth": "plain-string"});
        let new = json!({"auth": {"password": "s3cret", "enabled": true}});
        let out = presentation(
            "bgi.user.write",
            &json!({"path": "t.json", "content": new.to_string()}),
            "写入",
            "",
            Some(&old.to_string()),
        );
        let s = serde_json::to_string(&out).unwrap();
        assert!(!s.contains("s3cret"));
        let ch = changes_of(&out);
        assert!(
            ch.iter()
                .any(|(l, v)| l == "auth" && v == "plain-string →（删除）")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "auth.password" && v == "（无）→ ******（已遮蔽）")
        );
        assert!(
            ch.iter()
                .any(|(l, v)| l == "auth.enabled" && v == "（无）→ 开")
        );
    }

    // BOM 前缀的 JSON 仍按结构化 diff 展示（仅解析层面，不改原 args 与 sha）
    #[test]
    fn bom_prefixed_json_parses_for_display() {
        let old = format!("\u{feff}{}", json!({"partyName": "一队"}).to_string());
        let new = json!({"partyName": "二队"});
        let args = json!({"path": "bom.json", "content": new.to_string()});
        let snapshot = args.clone();
        let out = presentation("bgi.user.write", &args, "写入", "", Some(&old));
        assert_eq!(args, snapshot);
        let ch = changes_of(&out);
        assert!(ch.iter().any(|(l, v)| l == "队伍" && v == "一队 → 二队"));

        // 无基线 + BOM：同样叶项化并带「将写入：」前缀、无箭头
        let out2 = presentation(
            "bgi.user.write",
            &json!({"path": "bom.json", "content": format!("\u{feff}{}", new.to_string())}),
            "写入",
            "",
            None,
        );
        let ch2 = changes_of(&out2);
        assert!(ch2.iter().any(|(l, v)| l == "队伍" && v == "将写入：二队"));
        assert!(!ch2.iter().any(|(_, v)| v.contains("→")));
    }
}
