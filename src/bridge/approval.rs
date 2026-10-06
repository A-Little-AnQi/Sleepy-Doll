//! 审批卡片的人话呈现：把待审批调用译成用户能看懂的标题、摘要与变更项。
//! 只用真实参数生成目标与效果，不虚构队伍、条数或成功结果；
//! 契约 description 是给模型的内部说明，绝不进入 presentation。

use serde_json::{Value, json};

/// 字段名到普通人可读名；未收录的保留原名，不假造中文。
const FIELD_LABELS: &[(&str, &str)] = &[("AutoFightConfig", "战斗设置")];

fn field_label(name: &str) -> &str {
    FIELD_LABELS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, label)| *label)
        .unwrap_or(name)
}

fn nonempty_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// 路径末段：普通行只展示业务名，完整路径留在原始参数详情里。
fn basename(path: &str) -> &str {
    path.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
}

/// 为一次待审批调用生成 presentation；None 表示无专门呈现，交上层回退处理。
pub fn presentation(method_id: &str, arguments: &Value) -> Option<Value> {
    match method_id {
        "bgi.set_pathing_party" => pathing_party(arguments),
        "bgi.prepare_pathing_group" => prepare_pathing_group(arguments),
        "bgi.run_script_group" => run_script_group(arguments),
        "bgi.run_script_groups" => run_script_groups(arguments),
        "bgi.sync_group_effective_config" => sync_group_effective(arguments),
        "bgi.start_game" => Some(json!({
            "title": "启动原神",
            "summary": "将启动原神。",
            "changes": [],
        })),
        "bgi.exit_game" => Some(json!({
            "title": "关闭原神",
            "summary": "将关闭当前运行的原神。",
            "changes": [],
        })),
        _ => None,
    }
}

fn pathing_party(arguments: &Value) -> Option<Value> {
    let group = nonempty_str(arguments, "groupName")?;
    let mut changes = vec![];
    // partyName 空串在该接口的写入端被当作未提供（不会清空已有队伍），
    // 因此只有非空值才作为一条变更展示。
    if let Some(party) = nonempty_str(arguments, "partyName") {
        changes.push(json!({"label": "队伍", "value": party}));
    }
    if let Some(hurry) = arguments.get("hurryAvatar").and_then(Value::as_str) {
        let value = if hurry.trim().is_empty() {
            "不使用技能赶路"
        } else {
            hurry.trim()
        };
        changes.push(json!({"label": "赶路角色", "value": value}));
    }
    if changes.is_empty() {
        return None;
    }
    Some(json!({
        "title": "修改任务设置",
        "summary": format!("修改「{group}」的以下设置。"),
        "changes": changes,
    }))
}

fn prepare_pathing_group(arguments: &Value) -> Option<Value> {
    let path = nonempty_str(arguments, "path")?;
    let dir = basename(path);
    if dir.is_empty() {
        return None;
    }
    // groupName 给出时按原文 trim 照用；缺省时宿主会按目录名自动取名
    //（去作者/版本/日期、截断等），卡片不编造精确任务名。
    match nonempty_str(arguments, "groupName") {
        Some(name) => Some(json!({
            "title": "创建路线任务",
            "summary": format!("将所选路线保存为「{name}」任务，沿用 BetterGI 当前设置。"),
            "changes": [
                {"label": "任务名", "value": name},
                {"label": "路线目录", "value": dir},
            ],
        })),
        None => Some(json!({
            "title": "创建路线任务",
            "summary": "将所选路线保存为新任务，名称按路线名称自动生成，沿用 BetterGI 当前设置。",
            "changes": [
                {"label": "任务名", "value": "按路线名称自动生成"},
                {"label": "路线目录", "value": dir},
            ],
        })),
    }
}

fn run_script_group(arguments: &Value) -> Option<Value> {
    // 调用方可能用不同键名传组名，取真实存在的那一个，不猜。
    let name = ["groupName", "name", "scriptGroupName"]
        .iter()
        .find_map(|key| nonempty_str(arguments, key))?;
    Some(json!({
        "title": "运行任务",
        "summary": format!("将开始运行任务「{name}」，运行情况可在任务记录中查看。"),
        "changes": [{"label": "任务", "value": name}],
    }))
}

fn run_script_groups(arguments: &Value) -> Option<Value> {
    let groups = string_list(arguments, "groupNames");
    if groups.is_empty() {
        return None;
    }
    let mut changes = vec![json!({
        "label": "执行顺序",
        "value": groups.join(" → "),
    })];
    // closeGameAfter 只在整个计划成功核验后才会关游戏；未提供就不显示，
    // 不虚构收尾。
    if arguments.get("closeGameAfter") == Some(&Value::Bool(true)) {
        changes.push(json!({"label": "结束后", "value": "全部成功后关闭原神"}));
    }
    Some(json!({
        "title": "运行任务",
        "summary": format!("将按顺序运行 {} 个任务。", groups.len()),
        "changes": changes,
    }))
}

fn sync_group_effective(arguments: &Value) -> Option<Value> {
    let group = nonempty_str(arguments, "groupName")?;
    let fields = string_list(arguments, "fields");
    if fields.is_empty() {
        return None;
    }
    // 已识别字段译中文；未识别的不把英文代码名堆进正文，
    // 只标注还有多少项（完整字段列表仍在原始参数详情里）。
    let mapped: Vec<&str> = fields
        .iter()
        .filter(|f| FIELD_LABELS.iter().any(|(key, _)| key == *f))
        .map(|f| field_label(f))
        .collect();
    let others = fields.len() - mapped.len();
    let mut value = mapped.join("、");
    if others > 0 {
        let tail = format!("其它 {others} 项设置");
        value = if value.is_empty() {
            tail
        } else {
            format!("{value}、{tail}")
        };
    }
    Some(json!({
        "title": "同步任务设置",
        "summary": format!("将把「{group}」中列出的设置改为与 BetterGI 当前设置一致。"),
        "changes": [
            {"label": "任务名", "value": group},
            {"label": "同步设置", "value": value},
        ],
    }))
}

/// 兜底呈现里允许进正文的具体参数（中文标签）；未列出的英文代码字段、
/// 秘钥与哈希一律只留在详情。
const FALLBACK_PARAM_LABELS: &[(&str, &str)] = &[
    ("partyName", "队伍"),
    ("hurryAvatar", "赶路角色"),
    ("width", "宽"),
    ("height", "高"),
    ("enabled", "启用"),
];

/// 专门呈现之外的兜底：只用 C# 契约的真实 displayName 与效果级别生成
/// 安全业务文案。display_name 缺失或含内部术语时退回通用标题；
/// 绝不读取 description/Purpose，也不从方法名推断"删除"等动作。
pub fn fallback_presentation(display_name: &str, arguments: &Value, effect: &str) -> Value {
    let raw = display_name.trim();
    // 内部说明里偶尔会带出实现术语；命中即整段弃用，只保留通用标题。
    let leaked = [
        "宿主",
        "原生",
        "深拷贝",
        "构造器",
        "CAS",
        "Sha256",
        "pathingConfig",
        "策略面",
    ];
    let title = if raw.is_empty() || leaked.iter().any(|word| raw.contains(word)) {
        "执行操作".to_string()
    } else {
        raw.replace("配置组", "任务")
    };
    // 摘要保留真实 displayName 动作，效果级别只做事实性补充。
    let summary = match effect {
        "configurationWrite" => format!("将执行「{title}」操作，会修改 BetterGI 的设置。"),
        "gameWrite" => format!("将执行「{title}」操作，会在游戏中执行动作。"),
        "hostCommand" => format!("将执行「{title}」操作，由 BetterGI 完成。"),
        _ => format!("将执行「{title}」操作。"),
    };
    let mut changes = vec![];
    for key in ["groupName", "name", "scriptGroupName", "folderName"] {
        if let Some(group) = nonempty_str(arguments, key) {
            changes.push(json!({"label": "任务", "value": group}));
            break;
        }
    }
    let groups = string_list(arguments, "groupNames");
    if !groups.is_empty() {
        changes.push(json!({"label": "任务", "value": groups.join("、")}));
    }
    for key in ["path", "file"] {
        if let Some(path) = nonempty_str(arguments, key) {
            let dir = basename(path);
            if !dir.is_empty() {
                changes.push(json!({"label": "目标", "value": dir}));
                break;
            }
        }
    }
    let paths = string_list(arguments, "paths");
    if !paths.is_empty() {
        let items: Vec<&str> = paths.iter().map(|p| basename(p)).collect();
        changes.push(json!({"label": "目标", "value": items.join("、")}));
    }
    for (key, label) in FALLBACK_PARAM_LABELS {
        if let Some(value) = arguments.get(*key) {
            let text = match value {
                Value::Bool(b) => if *b { "开" } else { "关" }.to_string(),
                Value::String(s) if !s.trim().is_empty() => s.trim().to_string(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            changes.push(json!({"label": label, "value": text}));
        }
    }
    json!({
        "title": title,
        "summary": summary,
        "changes": changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party(arguments: Value) -> Value {
        presentation("bgi.set_pathing_party", &arguments).unwrap()
    }

    fn plain(value: &Value) -> String {
        serde_json::to_string(value).unwrap()
    }

    #[test]
    fn only_group_and_party_shows_single_change() {
        let card = party(json!({"groupName": "挖矿讨伐", "partyName": "双岩双火"}));
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["label"], "队伍");
        assert_eq!(changes[0]["value"], "双岩双火");
    }

    #[test]
    fn all_present_fields_appear_with_actual_values() {
        let card = party(json!({
            "groupName": " 挖矿讨伐 ",
            "partyName": "国家队",
            "hurryAvatar": "玛薇卡"
        }));
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[1]["label"], "赶路角色");
        assert_eq!(changes[1]["value"], "玛薇卡");
        assert!(card["summary"].as_str().unwrap().contains("挖矿讨伐"));
    }

    #[test]
    fn empty_hurry_avatar_reads_as_no_skill() {
        let card = party(json!({"groupName": "采集", "hurryAvatar": ""}));
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["value"], "不使用技能赶路");
        // 空队伍名不构成变更，也不得显示成空值。
        let card = party(json!({"groupName": "采集", "partyName": "", "hurryAvatar": ""}));
        assert_eq!(card["changes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn unknown_method_or_no_changeable_fields_yield_none() {
        assert!(presentation("bgi.run_script_group", &json!({"groupName": "x"})).is_some());
        assert!(presentation("bgi.set_pathing_party", &json!({"groupName": "x"})).is_none());
        assert!(presentation("bgi.prepare_pathing_group", &json!({"path": " "})).is_none());
        assert!(presentation("bgi.run_script_groups", &json!({"groupNames": []})).is_none());
        assert!(presentation("bgi.some_unknown", &json!({"path": "x"})).is_none());
    }

    /// 截图场景：prepare_pathing_group 不得出现契约内部说明词汇，
    /// 也不得声称已创建或已成功。
    #[test]
    fn prepare_pathing_group_reads_like_plain_language() {
        let card = presentation(
            "bgi.prepare_pathing_group",
            &json!({
                "path": "pathing/地图追踪/锄地/兽怪暴徒",
                "groupName": "兽怪暴徒"
            }),
        )
        .unwrap();
        assert_eq!(card["title"], "创建路线任务");
        let summary = card["summary"].as_str().unwrap();
        assert!(summary.contains("「兽怪暴徒」"));
        assert!(summary.contains("沿用 BetterGI 当前设置"));
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes[0]["label"], "任务名");
        assert_eq!(changes[0]["value"], "兽怪暴徒");
        assert_eq!(changes[1]["label"], "路线目录");
        assert_eq!(changes[1]["value"], "兽怪暴徒");
        let text = plain(&card);
        for banned in [
            "宿主",
            "原生构造器",
            "同名同类型",
            "深拷贝",
            "策略面",
            "已创建",
            "已保存",
            "成功",
            "地图追踪/锄地",
        ] {
            assert!(!text.contains(banned), "出现内部/越权表述：{banned}");
        }
    }

    /// groupName 缺省时不编造精确任务名（宿主会去作者/版本/截断改名），
    /// 目录里的 @作者 等后缀不得被当成实际任务名。
    #[test]
    fn prepare_pathing_group_without_name_does_not_invent_one() {
        let card = presentation(
            "bgi.prepare_pathing_group",
            &json!({"path": "pathing/地方特产/稻妻/血斛/"}),
        )
        .unwrap();
        assert_eq!(
            card["summary"],
            "将所选路线保存为新任务，名称按路线名称自动生成，沿用 BetterGI 当前设置。"
        );
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes[0]["value"], "按路线名称自动生成");
        assert_eq!(changes[1]["value"], "血斛");

        let authored = presentation(
            "bgi.prepare_pathing_group",
            &json!({"path": "pathing/锄地/血斛@某作者"}),
        )
        .unwrap();
        let text = plain(&authored);
        assert!(!text.contains("「血斛@某作者」"));
        assert_eq!(authored["changes"][0]["value"], "按路线名称自动生成");
    }

    /// 单组运行接受不同键名，且不说"已运行成功"。
    #[test]
    fn run_script_group_accepts_alternate_name_keys() {
        for key in ["groupName", "name", "scriptGroupName"] {
            let card = presentation("bgi.run_script_group", &json!({key: "每日采集"})).unwrap();
            assert_eq!(card["title"], "运行任务");
            assert_eq!(card["changes"][0]["value"], "每日采集");
            assert!(!plain(&card).contains("成功"));
        }
    }

    /// 多组运行展示真实顺序与收尾；closeGameAfter 未开时不虚构关游戏。
    #[test]
    fn run_script_groups_shows_order_and_optional_close() {
        let card = presentation(
            "bgi.run_script_groups",
            &json!({"groupNames": ["采集", "挖矿", "采集"], "closeGameAfter": true}),
        )
        .unwrap();
        assert_eq!(card["summary"], "将按顺序运行 3 个任务。");
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes[0]["value"], "采集 → 挖矿 → 采集");
        assert_eq!(changes[1]["value"], "全部成功后关闭原神");

        let plain_card = presentation(
            "bgi.run_script_groups",
            &json!({"groupNames": ["采集"], "closeGameAfter": false}),
        )
        .unwrap();
        assert!(!plain(&plain_card).contains("关闭原神"));
        assert!(!plain(&plain_card).contains("结束后"));
    }

    /// 同步任务设置：已识别字段译中文，未识别的不把英文名堆进正文，
    /// 只标注其余项数；CAS/哈希不出现。
    #[test]
    fn sync_group_effective_maps_known_fields_only() {
        let card = presentation(
            "bgi.sync_group_effective_config",
            &json!({
                "groupName": "锄地",
                "fields": ["AutoFightConfig", "SomethingElse"],
                "expectedSha256": "0123abcd"
            }),
        )
        .unwrap();
        assert_eq!(card["title"], "同步任务设置");
        let changes = card["changes"].as_array().unwrap();
        assert_eq!(changes[1]["label"], "同步设置");
        assert_eq!(changes[1]["value"], "战斗设置、其它 1 项设置");
        let text = plain(&card);
        assert!(!text.contains("SomethingElse"));
        assert!(!text.contains("CAS"));
        assert!(!text.contains("Sha256"));
        assert!(!text.contains("0123abcd"));

        // 全部未识别：只说其它 N 项，不显示任何英文代码名。
        let all_unknown = presentation(
            "bgi.sync_group_effective_config",
            &json!({"groupName": "锄地", "fields": ["AutoXyzConfig", "AutoAbcConfig"]}),
        )
        .unwrap();
        assert_eq!(all_unknown["changes"][1]["value"], "其它 2 项设置");
    }

    /// 兜底呈现：摘要保留真实 displayName 动作，不泄漏内部术语；
    /// 常见参数显示中文标签与真实值，英文代码字段/哈希不进正文。
    #[test]
    fn fallback_presentation_is_safe_for_unknown_methods() {
        let card = fallback_presentation(
            "提交设置修改",
            &json!({"planId": "内部计划标识", "expectedSha256": "0123abcd"}),
            "configurationWrite",
        );
        assert_eq!(card["title"], "提交设置修改");
        assert_eq!(
            card["summary"],
            "将执行「提交设置修改」操作，会修改 BetterGI 的设置。"
        );
        assert!(!plain(&card).contains("planId"));
        assert!(!plain(&card).contains("内部计划标识"));
        assert!(!plain(&card).contains("0123abcd"));

        // displayName 带内部术语时整体弃用，退回通用标题。
        let leaked = fallback_presentation(
            "用宿主原生构造器包装配置组",
            &json!({"groupName": "采集"}),
            "hostCommand",
        );
        assert_eq!(leaked["title"], "执行操作");
        assert_eq!(
            leaked["summary"],
            "将执行「执行操作」操作，由 BetterGI 完成。"
        );
        assert_eq!(leaked["changes"][0]["value"], "采集");

        // 未登记的效果级别用通用影响说明，不编造具体动作。
        let unknown =
            fallback_presentation("清理", &json!({"paths": ["pathing/稻妻/血斛"]}), "unknown");
        assert_eq!(unknown["summary"], "将执行「清理」操作。");
        let text = plain(&unknown);
        assert!(!text.contains("删除"));
        assert!(text.contains("血斛"));
    }

    /// 兜底呈现的普通参数行：中文标签 + 真实值；未收录英文键不进正文。
    #[test]
    fn fallback_presentation_shows_common_params_in_chinese() {
        let card = fallback_presentation(
            "修改捕获设置",
            &json!({
                "name": "采集",
                "width": 1920,
                "height": 1080,
                "enabled": true,
                "partyName": "国家队",
                "hurryAvatar": "玛薇卡",
                "someUnknownCodeField": "raw-code"
            }),
            "configurationWrite",
        );
        let changes = card["changes"].as_array().unwrap();
        let get = |label: &str| {
            changes
                .iter()
                .find(|c| c["label"] == label)
                .map(|c| c["value"].clone())
                .unwrap_or(Value::Null)
        };
        assert_eq!(get("任务"), "采集");
        assert_eq!(get("宽"), "1920");
        assert_eq!(get("高"), "1080");
        assert_eq!(get("启用"), "开");
        assert_eq!(get("队伍"), "国家队");
        assert_eq!(get("赶路角色"), "玛薇卡");
        assert!(!plain(&card).contains("someUnknownCodeField"));
        assert!(!plain(&card).contains("raw-code"));
    }

    /// 启停游戏的固定文案不声称业务完成。
    #[test]
    fn game_lifecycle_cards_stay_factual() {
        let start = presentation("bgi.start_game", &json!({})).unwrap();
        assert_eq!(start["title"], "启动原神");
        let exit = presentation("bgi.exit_game", &json!({})).unwrap();
        assert_eq!(exit["title"], "关闭原神");
        assert!(!plain(&start).contains("成功"));
    }
}
