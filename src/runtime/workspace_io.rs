//! 通用工作区文件 IO 工具（list / read / read_many / search）。
//!
//! 契约说明（与 workspace.rs 的接线方约定）：
//! - `resolve` 不限制在软件根目录内：绝对路径（含 `D:/file` 等其他盘）合法，
//!   相对路径基于 `root` 解析后做词法规范化；存在的路径返回 canonical 结果，
//!   不存在的返回规范化路径。含 NUL 的输入直接报错。
//! - 只依赖 `crate::error::{Error, Result}` 与 serde_json；不与 BridgeRPC
//!   重复，也不对任何游戏或日志名称做特例。
//! - 目录遍历用 canonical 集合防 junction/symlink 循环；symlink 本身一律
//!   跳过并记录在 `skipped` 中，不会被当作文件打开。
//! - read 的 `text` 是原文的行切片：CRLF、末尾换行原样保留；预算截断可能
//!   切在半行，此时 `nextLine` 指向该半行所在行号（重读有重叠，不跳不漏）。

use crate::error::{Error, Result};
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

const DEFAULT_READ_MAX_CHARS: usize = 524_288;
const MAX_READ_CHARS: usize = 8 * 1024 * 1024;
const DEFAULT_BATCH_BUDGET: usize = 524_288;
const MAX_BATCH_BUDGET: usize = 8 * 1024 * 1024;
const DEFAULT_LIST_LIMIT: usize = 2000;
const READ_MANY_MAX_PATHS: usize = 64;
const SEARCH_MAX_PATTERNS: usize = 64;
const DEFAULT_SAMPLE_LIMIT: usize = 20;
const MAX_SAMPLE_LIMIT: usize = 2000;
const SAMPLE_TEXT_CHARS: usize = 500;

fn tool_err(msg: impl Into<String>) -> Error {
    Error::Tool(msg.into())
}

fn arg_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(Value::as_u64).map(|v| v as usize)
}

/// 把 `root` + `path` 解析为一个规范化的绝对路径。
pub fn resolve(root: &Path, path: &str) -> Result<PathBuf> {
    if path.contains('\0') {
        return Err(tool_err("路径中包含 NUL 字符"));
    }
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Ok(root.to_path_buf());
    }
    let raw = Path::new(trimmed);
    let base = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        root.join(raw)
    };
    let mut out = PathBuf::new();
    for comp in base.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(tool_err(format!("路径 {path:?} 中 .. 越出了文件系统根")));
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(tool_err(format!("路径 {path:?} 解析后为空")));
    }
    match out.canonicalize() {
        Ok(canon) => Ok(canon),
        Err(_) => Ok(out),
    }
}

fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// 按原文切行：每项保留自身的 `\r\n` / `\n` / 无换行结尾。
fn raw_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find('\n') {
            Some(i) => {
                out.push(&rest[..=i]);
                rest = &rest[i + 1..];
            }
            None => {
                out.push(rest);
                break;
            }
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// 读单个已解析文件并组装 JSON 结果；预算与分页参数由调用方给出。
/// `max_chars` 允许为 0（batch 预算耗尽时）：返回空 text 且 truncated。
fn read_file_value(
    path_label: &str,
    abs: &Path,
    start_line: usize,
    max_lines: Option<usize>,
    tail_lines: Option<usize>,
    max_chars: usize,
) -> Result<Value> {
    let bytes = fs::read(abs).map_err(|e| tool_err(format!("读取 {} 失败：{e}", abs.display())))?;
    let sha256 = sha256_hex(&bytes);
    let text_full = decode_bytes(&bytes);
    let chars = text_full.chars().count();
    let lines = raw_lines(&text_full);
    let total_lines = lines.len();

    let mut start = start_line.max(1);
    if let Some(tail) = tail_lines.filter(|t| *t > 0) {
        start = total_lines.saturating_sub(tail) + 1;
    }
    let end_limit = match max_lines {
        Some(n) => start.saturating_add(n).saturating_sub(1),
        None => usize::MAX,
    };

    let mut out_text = String::new();
    let mut used = 0usize;
    let mut last_returned = 0usize;
    let mut char_resume_line = 0usize;
    let mut chars_truncated = false;
    let mut lines_truncated = false;
    for (idx, line) in lines.iter().enumerate() {
        let no = idx + 1;
        if no < start {
            continue;
        }
        if no > end_limit {
            lines_truncated = true;
            break;
        }
        let need = line.chars().count();
        if used + need > max_chars {
            let room = max_chars.saturating_sub(used);
            if room > 0 {
                out_text.push_str(&line.chars().take(room).collect::<String>());
                last_returned = no;
            }
            // 无论截半还是整行装不下，续读都从当前行 no 开始：
            // 截半时重叠重读该行，预算恰好耗尽时从这条未读整行继续。
            char_resume_line = no;
            chars_truncated = true;
            break;
        }
        out_text.push_str(line);
        used += need;
        last_returned = no;
    }

    let returned_lines = if last_returned >= start {
        last_returned - start + 1
    } else {
        0
    };
    // 半行截断/预算耗尽时 nextLine 指向当前行 no（截半时重叠重读，整行未读时
    // 正好是下一条完整行）；maxLines 截断时指向下一条未读的完整行；
    // 读完全部（含空文件、start 越过 EOF）则为 null。
    let next_line = if chars_truncated {
        json!(char_resume_line)
    } else if lines_truncated {
        json!(last_returned + 1)
    } else if last_returned == total_lines || total_lines == 0 || last_returned == 0 {
        Value::Null
    } else {
        json!(last_returned + 1)
    };

    Ok(json!({
        "ok": true,
        "path": path_label,
        "absolutePath": abs.display().to_string(),
        "bytes": bytes.len(),
        "sha256": sha256,
        "chars": chars,
        "totalLines": total_lines,
        "returnedStartLine": start,
        "returnedLines": returned_lines,
        "nextLine": next_line,
        "truncated": chars_truncated || lines_truncated,
        "text": out_text,
    }))
}

pub fn read(root: &Path, args: &Value) -> Result<Value> {
    let path_arg = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| tool_err("read 需要非空 path 参数"))?;
    let max_chars = arg_usize(args, "maxChars").unwrap_or(DEFAULT_READ_MAX_CHARS);
    if max_chars == 0 || max_chars > MAX_READ_CHARS {
        return Err(tool_err(format!(
            "maxChars 必须在 1..={MAX_READ_CHARS} 之间"
        )));
    }
    let abs = resolve(root, path_arg)?;
    read_file_value(
        path_arg,
        &abs,
        arg_usize(args, "startLine").unwrap_or(1).max(1),
        arg_usize(args, "maxLines"),
        arg_usize(args, "tailLines"),
        max_chars,
    )
}

pub fn read_many(root: &Path, args: &Value) -> Result<Value> {
    let paths: Vec<String> = args
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| tool_err("read_many 需要 paths 数组"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| tool_err("paths 中每一项必须是字符串"))
        })
        .collect::<Result<_>>()?;
    if paths.is_empty() || paths.len() > READ_MANY_MAX_PATHS {
        return Err(tool_err(format!(
            "paths 数量必须在 1..={READ_MANY_MAX_PATHS}"
        )));
    }
    let budget = arg_usize(args, "textBudget").unwrap_or(DEFAULT_BATCH_BUDGET);
    if budget == 0 || budget > MAX_BATCH_BUDGET {
        return Err(tool_err(format!(
            "textBudget 必须在 1..={MAX_BATCH_BUDGET} 之间"
        )));
    }
    let per_file_default = arg_usize(args, "maxChars").unwrap_or(DEFAULT_READ_MAX_CHARS);
    if per_file_default == 0 || per_file_default > MAX_READ_CHARS {
        return Err(tool_err(format!(
            "maxChars 必须在 1..={MAX_READ_CHARS} 之间"
        )));
    }

    let mut remaining = budget;
    let mut results = Vec::with_capacity(paths.len());
    for p in &paths {
        let entry = match resolve(root, p) {
            Ok(abs) => {
                let cap = per_file_default.min(remaining);
                match read_file_value(p, &abs, 1, None, None, cap) {
                    Ok(mut v) => {
                        let used = v["text"].as_str().map(|t| t.chars().count()).unwrap_or(0);
                        remaining = remaining.saturating_sub(used);
                        v["budgetExhausted"] = Value::Bool(remaining == 0);
                        v
                    }
                    Err(e) => json!({ "path": p, "ok": false, "error": e.to_string() }),
                }
            }
            Err(e) => json!({ "path": p, "ok": false, "error": e.to_string() }),
        };
        results.push(entry);
    }
    let complete = results.iter().all(|r| {
        r.get("ok").and_then(Value::as_bool).unwrap_or(false)
            && !r.get("truncated").and_then(Value::as_bool).unwrap_or(false)
    });
    Ok(json!({
        "root": root.display().to_string(),
        "paths": paths,
        "textBudget": budget,
        "budgetUsed": budget - remaining,
        "complete": complete,
        "results": results,
    }))
}

fn glob_to_regex(glob: &str) -> Result<Regex> {
    // 支持 * ** ?（** 跨目录，* 和 ? 不跨）。不支持字符类 []。
    let mut re = String::from("(?s)^");
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') {
                        chars.next();
                    }
                    re.push_str(".*");
                } else {
                    re.push_str("[^/]*");
                }
            }
            '?' => re.push_str("[^/]"),
            other => re.push_str(&regex::escape(&other.to_string())),
        }
    }
    re.push('$');
    Regex::new(&re).map_err(|e| tool_err(format!("include 通配符 {glob:?} 无效：{e}")))
}

fn parse_include(args: &Value) -> Result<Option<Vec<Regex>>> {
    let raw = match args.get("include") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| tool_err("include 数组中每一项必须是字符串"))
            })
            .collect::<Result<Vec<_>>>()?,
        Some(_) => return Err(tool_err("include 必须是字符串或字符串数组")),
    };
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        raw.iter()
            .map(|g| glob_to_regex(g))
            .collect::<Result<Vec<_>>>()?,
    ))
}

fn include_matches(include: &[Regex], abs: &Path) -> bool {
    if include.is_empty() {
        return true;
    }
    let normalized = abs.display().to_string().replace('\\', "/");
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    include
        .iter()
        .any(|re| re.is_match(&normalized) || re.is_match(&name))
}

/// 单个遍历项：路径 + 是否目录。
struct WalkItem {
    path: PathBuf,
    is_dir: bool,
}

/// 遍历目录。symlink/junction 一律跳过并记入 `skipped`（不会被当作文件
/// 打开，也不会跟进造成循环）；某子目录 read_dir 失败只记录错误，已收集
/// 的其余项全部保留。`collect_dirs` 控制目录本身是否进入结果（list 需要，
/// search 不需要）。
fn walk(
    dir: &Path,
    recursive: bool,
    collect_dirs: bool,
    out: &mut Vec<WalkItem>,
    visited: &mut HashSet<PathBuf>,
    skipped: &mut Vec<String>,
    errors: &mut Vec<Value>,
) {
    let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if !visited.insert(canon) {
        return;
    }
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            errors.push(json!({
                "path": dir.display().to_string(),
                "error": format!("读取目录失败：{e}"),
            }));
            return;
        }
    };
    let mut subdirs = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        let ft = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                errors.push(json!({
                    "path": p.display().to_string(),
                    "error": format!("读取条目类型失败：{e}"),
                }));
                continue;
            }
        };
        if ft.is_symlink() {
            skipped.push(p.display().to_string());
            continue;
        }
        if ft.is_dir() {
            if collect_dirs {
                out.push(WalkItem {
                    path: p.clone(),
                    is_dir: true,
                });
            }
            if recursive {
                subdirs.push(p);
            }
        } else {
            out.push(WalkItem {
                path: p,
                is_dir: false,
            });
        }
    }
    for sub in subdirs {
        walk(&sub, recursive, collect_dirs, out, visited, skipped, errors);
    }
}

pub fn list(root: &Path, args: &Value) -> Result<Value> {
    let path_arg = args
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    let recursive = args
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let limit = arg_usize(args, "limit").unwrap_or(DEFAULT_LIST_LIMIT);
    let include = parse_include(args)?.unwrap_or_default();

    let abs = resolve(root, path_arg)?;
    if !abs.exists() {
        return Err(tool_err(format!("路径 {} 不存在", abs.display())));
    }
    if !abs.is_dir() {
        return Err(tool_err(format!("路径 {} 不是目录", abs.display())));
    }

    let mut items: Vec<WalkItem> = Vec::new();
    let mut visited = HashSet::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();
    walk(
        &abs,
        recursive,
        true,
        &mut items,
        &mut visited,
        &mut skipped,
        &mut errors,
    );

    // total 是符合 include 的全部文件与目录数，不受 limit 影响；排序稳定。
    let mut matched: Vec<WalkItem> = items
        .into_iter()
        .filter(|it| include_matches(&include, &it.path))
        .collect();
    matched.sort_by(|a, b| {
        a.path
            .display()
            .to_string()
            .cmp(&b.path.display().to_string())
    });

    let total = matched.len();
    let truncated = total > limit;
    let entries: Vec<Value> = matched
        .into_iter()
        .take(limit)
        .map(|it| {
            let bytes = if it.is_dir {
                0
            } else {
                fs::metadata(&it.path).map(|m| m.len()).unwrap_or(0)
            };
            json!({
                "name": it.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                "directory": it.is_dir,
                "bytes": bytes,
                "absolutePath": it.path.display().to_string(),
            })
        })
        .collect();

    Ok(json!({
        "root": root.display().to_string(),
        "path": path_arg,
        "absolutePath": abs.display().to_string(),
        "entries": entries,
        "truncated": truncated,
        "total": total,
        "skipped": skipped,
        "errors": errors,
    }))
}

struct PatternStat {
    regex: Regex,
    name: String,
    total_matches: u64,
    groups: HashMapish,
    invalid_numeric: u64,
    samples: Vec<Value>,
    sample_truncated: bool,
}

/// value -> (count, sum)
struct HashMapish(std::collections::HashMap<String, (u64, f64)>);

#[allow(clippy::type_complexity)]
struct ScanCtx<'a> {
    capture: &'a Option<String>,
    sum_capture: &'a Option<String>,
    sample_limit: usize,
}

fn scan_line(
    line: &str,
    file_label: &str,
    line_no: u64,
    stats: &mut [PatternStat],
    ctx: &ScanCtx<'_>,
) {
    for st in stats.iter_mut() {
        let mut sampled = st.samples.len() < ctx.sample_limit;
        for caps in st.regex.captures_iter(line) {
            st.total_matches += 1;
            let key = ctx
                .capture
                .as_ref()
                .and_then(|cn| caps.name(cn))
                .map(|m| m.as_str().to_owned())
                .unwrap_or_else(|| "(all)".to_owned());
            let entry = st.groups.0.entry(key).or_insert((0u64, 0f64));
            entry.0 += 1;
            if let Some(sn) = ctx.sum_capture {
                let raw = caps.name(sn).map(|m| m.as_str());
                match raw.and_then(|s| s.parse::<f64>().ok()) {
                    // NaN/Inf 解析成功但非有限值，按非法数值计数，不默默算 0。
                    Some(n) if n.is_finite() => entry.1 += n,
                    Some(_) => st.invalid_numeric += 1,
                    None => st.invalid_numeric += 1,
                }
            }
            if sampled {
                st.samples.push(json!({
                    "file": file_label,
                    "line": line_no,
                    "text": truncate_chars(line, SAMPLE_TEXT_CHARS),
                }));
                sampled = false;
            } else if !sampled && st.samples.len() >= ctx.sample_limit {
                st.sample_truncated = true;
            }
        }
    }
}

pub fn search(root: &Path, args: &Value) -> Result<Value> {
    let case_sensitive = args
        .get("caseSensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut raw_patterns: Vec<(String, String)> = Vec::new();
    if let Some(p) = args.get("pattern").and_then(Value::as_str) {
        raw_patterns.push(("pattern".to_owned(), p.to_owned()));
    }
    if let Some(list) = args.get("patterns").and_then(Value::as_array) {
        for item in list {
            let obj = item
                .as_object()
                .ok_or_else(|| tool_err("patterns 中每一项必须是 {name, pattern} 对象"))?;
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| tool_err("patterns 项缺少 name"))?
                .to_owned();
            let pat = obj
                .get("pattern")
                .and_then(Value::as_str)
                .ok_or_else(|| tool_err("patterns 项缺少 pattern"))?
                .to_owned();
            raw_patterns.push((name, pat));
        }
    }
    if raw_patterns.is_empty() || raw_patterns.len() > SEARCH_MAX_PATTERNS {
        return Err(tool_err(format!(
            "必须提供 1..={SEARCH_MAX_PATTERNS} 个 pattern/patterns"
        )));
    }
    let capture = args
        .get("capture")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let sum_capture = args
        .get("sumCapture")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let sample_limit = arg_usize(args, "sampleLimit")
        .unwrap_or(DEFAULT_SAMPLE_LIMIT)
        .min(MAX_SAMPLE_LIMIT);
    let recursive = args
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let include = parse_include(args)?.unwrap_or_default();

    let mut stats: Vec<PatternStat> = raw_patterns
        .into_iter()
        .map(|(name, pat)| {
            let re = RegexBuilder::new(&pat)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| tool_err(format!("正则 {name:?}（{pat:?}）无效：{e}")))?;
            for opt in [&capture, &sum_capture].into_iter().flatten() {
                if re.capture_names().all(|n| n != Some(opt.as_str())) {
                    return Err(tool_err(format!(
                        "命名分组 {opt:?} 在 pattern {name:?} 中不存在"
                    )));
                }
            }
            Ok(PatternStat {
                regex: re,
                name,
                total_matches: 0,
                groups: HashMapish(std::collections::HashMap::new()),
                invalid_numeric: 0,
                samples: Vec::new(),
                sample_truncated: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut root_paths: Vec<String> = Vec::new();
    if let Some(list) = args.get("paths").and_then(Value::as_array) {
        for v in list {
            root_paths.push(
                v.as_str()
                    .ok_or_else(|| tool_err("paths 中每一项必须是字符串"))?
                    .to_owned(),
            );
        }
    }
    if let Some(p) = args.get("path").and_then(Value::as_str) {
        root_paths.push(p.to_owned());
    }
    if root_paths.is_empty() {
        return Err(tool_err("search 需要 paths（文件或目录数组）或 path"));
    }

    let mut files: Vec<PathBuf> = Vec::new();
    // canonical 去重：同一文件经目录展开与直接路径、或大小写不同的写法只扫一次。
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut errors: Vec<Value> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for rp in &root_paths {
        let abs = match resolve(root, rp) {
            Ok(a) => a,
            Err(e) => {
                errors.push(json!({ "path": rp, "error": e.to_string() }));
                continue;
            }
        };
        let meta = match fs::symlink_metadata(&abs) {
            Ok(m) => m,
            Err(e) => {
                errors.push(
                    json!({ "path": rp, "error": format!("{} 访问失败：{e}", abs.display()) }),
                );
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            skipped.push(abs.display().to_string());
            continue;
        }
        if meta.is_dir() {
            let mut items: Vec<WalkItem> = Vec::new();
            let mut visited = HashSet::new();
            walk(
                &abs,
                recursive,
                false,
                &mut items,
                &mut visited,
                &mut skipped,
                &mut errors,
            );
            for it in items {
                if !it.is_dir && include_matches(&include, &it.path) {
                    let key = it.path.canonicalize().unwrap_or_else(|_| it.path.clone());
                    if seen.insert(key) {
                        files.push(it.path);
                    }
                }
            }
        } else if !include_matches(&include, &abs) {
            continue;
        } else {
            let key = abs.canonicalize().unwrap_or_else(|_| abs.clone());
            if seen.insert(key) {
                files.push(abs);
            }
        }
    }

    let ctx = ScanCtx {
        capture: &capture,
        sum_capture: &sum_capture,
        sample_limit,
    };
    let mut files_scanned = 0u64;
    let mut bytes_scanned = 0u64;
    for file in &files {
        match scan_file(file, &mut stats, &ctx, &mut bytes_scanned) {
            Ok(()) => files_scanned += 1,
            Err(e) => errors.push(json!({
                "file": file.display().to_string(),
                "error": e.to_string(),
            })),
        }
    }

    let patterns_out: Vec<Value> = stats
        .into_iter()
        .map(|st| {
            let mut groups: Vec<(String, u64, f64)> = st
                .groups
                .0
                .into_iter()
                .map(|(value, (count, sum))| (value, count, sum))
                .collect();
            groups.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            json!({
                "name": st.name,
                "totalMatches": st.total_matches,
                "groups": groups.into_iter().map(|(value, count, sum)| json!({
                    "value": value, "count": count, "sum": sum,
                })).collect::<Vec<_>>(),
                "invalidNumeric": st.invalid_numeric,
                "samples": st.samples,
                "sampleTruncated": st.sample_truncated,
            })
        })
        .collect();

    Ok(json!({
        "root": root.display().to_string(),
        "filesScanned": files_scanned,
        "bytesScanned": bytes_scanned,
        "complete": errors.is_empty(),
        "errors": errors,
        "skipped": skipped,
        "patterns": patterns_out,
    }))
}

/// 单文件扫描：UTF-16 BOM 文件整体读入解码后按行扫描；其余文件
/// BufRead 流式逐行扫描（无 4Mi 尾截断），UTF-8 BOM 从首行剥除。
fn scan_file(
    file: &Path,
    stats: &mut [PatternStat],
    ctx: &ScanCtx<'_>,
    bytes_scanned: &mut u64,
) -> Result<()> {
    let label = file.display().to_string();
    let mut f = fs::File::open(file).map_err(|e| tool_err(format!("打开失败：{e}")))?;
    let mut head = [0u8; 4];
    let mut hn = 0usize;
    while hn < 4 {
        let n = f
            .read(&mut head[hn..])
            .map_err(|e| tool_err(format!("读取失败：{e}")))?;
        if n == 0 {
            break;
        }
        hn += n;
    }
    let utf16 =
        hn >= 2 && ((head[0] == 0xFF && head[1] == 0xFE) || (head[0] == 0xFE && head[1] == 0xFF));
    if utf16 {
        let bytes = fs::read(file).map_err(|e| tool_err(format!("读取失败：{e}")))?;
        *bytes_scanned += bytes.len() as u64;
        let text = decode_bytes(&bytes);
        let mut pieces: Vec<&str> = text.split('\n').collect();
        if text.ends_with('\n') {
            pieces.pop();
        }
        for (i, raw) in pieces.iter().enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            scan_line(line, &label, (i + 1) as u64, stats, ctx);
        }
        return Ok(());
    }
    f.seek(SeekFrom::Start(0))
        .map_err(|e| tool_err(format!("seek 失败：{e}")))?;
    let mut reader = BufReader::new(f);
    let mut buf: Vec<u8> = Vec::new();
    let mut line_no = 0u64;
    let mut first = true;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(tool_err(format!("读取失败：{e}"))),
        }
        *bytes_scanned += buf.len() as u64;
        line_no += 1;
        let mut lossy = String::from_utf8_lossy(&buf).into_owned();
        if first {
            first = false;
            if lossy.starts_with('\u{feff}') {
                lossy.remove(0);
            }
            if lossy.is_empty() {
                line_no -= 1;
                continue;
            }
        }
        if lossy.ends_with('\n') {
            lossy.pop();
            if lossy.ends_with('\r') {
                lossy.pop();
            }
        }
        scan_line(&lossy, &label, line_no, stats, ctx);
    }
    Ok(())
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture(sub: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/.tmp/general-tools/fixtures")
            .join(sub);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_supports_relative_dotdot_and_outside_absolute() {
        let dir = fixture("resolve");
        let root = dir.join("root");
        fs::create_dir_all(&root).unwrap();
        fs::write(dir.join("outside.txt"), "x").unwrap();

        let inside = resolve(&root, "../outside.txt").unwrap();
        assert_eq!(inside, dir.join("outside.txt").canonicalize().unwrap());
        let missing = resolve(&root, "no/such/../file.txt").unwrap();
        assert_eq!(missing, root.join("no").join("file.txt"));
        let abs_outside = resolve(&root, &dir.join("outside.txt").to_string_lossy()).unwrap();
        assert!(abs_outside.ends_with("outside.txt"));
    }

    #[test]
    fn list_total_includes_dirs_and_ignores_limit() {
        let dir = fixture("list");
        for i in 0..5 {
            fs::write(dir.join(format!("f{i}.txt")), "x").unwrap();
        }
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub").join("inner.txt"), "x").unwrap();

        // include 用受支持的 ? 通配（本实现不支持 [] 字符类）。
        let filtered = list(&dir, &json!({ "include": "f?.txt" })).unwrap();
        assert_eq!(filtered["total"], 5);
        assert_eq!(filtered["entries"].as_array().unwrap().len(), 5);

        let flat = list(&dir, &json!({})).unwrap();
        // 非递归：5 个文件 + 1 个目录
        assert_eq!(flat["total"], 6);
        let dirs: Vec<_> = flat["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["directory"] == Value::Bool(true))
            .collect();
        assert_eq!(dirs.len(), 1);

        let limited = list(&dir, &json!({ "limit": 2 })).unwrap();
        assert_eq!(limited["entries"].as_array().unwrap().len(), 2);
        assert_eq!(limited["total"], 6, "total 不受 limit 影响");
        assert_eq!(limited["truncated"], true);

        let rec = list(&dir, &json!({ "recursive": true })).unwrap();
        assert_eq!(rec["total"], 7); // 6 文件 + sub 目录
    }

    #[test]
    fn read_preserves_crlf_and_partial_line_resume() {
        let dir = fixture("read-crlf");
        let file = dir.join("crlf.txt");
        let body = "aaa\r\nbbb\r\nccc\r\n";
        fs::write(&file, body).unwrap();

        let full = read(&dir, &json!({ "path": "crlf.txt" })).unwrap();
        assert_eq!(full["text"], body, "完整读取必须原样保留 CRLF 与末尾换行");
        assert_eq!(full["truncated"], false);
        assert_eq!(full["totalLines"], 3);

        // 预算 5 字符：整行 "aaa\r\n"(5) 恰好装下，预算对下一行 room=0。
        // nextLine 必须指向下一行（旧实现回指 1 会无限重复整行）。
        let page = read(&dir, &json!({ "path": "crlf.txt", "maxChars": 5 })).unwrap();
        assert_eq!(page["text"], "aaa\r\n");
        assert_eq!(page["nextLine"], 2);
        assert_eq!(page["returnedLines"], 1);
        // maxChars=6：第二行装不下（还需 5），截半出 "b"
        let part = read(&dir, &json!({ "path": "crlf.txt", "maxChars": 6 })).unwrap();
        assert_eq!(part["text"], "aaa\r\nb");
        assert_eq!(part["truncated"], true);
        assert_eq!(
            part["nextLine"], 2,
            "半行截断 nextLine 指向该半行（重叠续读）"
        );
        assert_eq!(part["returnedLines"], 2);
        // 从 nextLine 续读，无跳漏
        let resume = read(&dir, &json!({ "path": "crlf.txt", "startLine": 2 })).unwrap();
        assert_eq!(resume["text"], "bbb\r\nccc\r\n");
        assert_eq!(resume["nextLine"], Value::Null);

        // startLine 越过 EOF：空且非截断
        let over = read(&dir, &json!({ "path": "crlf.txt", "startLine": 99 })).unwrap();
        assert_eq!(over["text"], "");
        assert_eq!(over["nextLine"], Value::Null);
        assert_eq!(over["truncated"], false);
    }

    #[test]
    fn read_many_budget_exhaustion_and_complete() {
        let dir = fixture("read-many");
        fs::write(dir.join("a.txt"), "aaaa\n").unwrap();
        fs::write(dir.join("b.txt"), "bbbb\n").unwrap();

        let tight = read_many(
            &dir,
            &json!({ "paths": ["a.txt", "b.txt"], "textBudget": 5 }),
        )
        .unwrap();
        let rs = tight["results"].as_array().unwrap();
        assert_eq!(rs[0]["text"], "aaaa\n");
        assert_eq!(rs[0]["ok"], Value::Bool(true));
        assert_eq!(rs[1]["text"], "", "预算耗尽必须为空，不得偷读");
        assert_eq!(rs[1]["truncated"], true);
        assert_eq!(rs[1]["returnedLines"], 0, "0 字符输出时行数为 0");
        assert_eq!(rs[1]["nextLine"], 1, "cap=0 时续读从首行开始");
        assert_eq!(tight["budgetUsed"], 5);
        assert_eq!(tight["complete"], Value::Bool(false));

        let mixed = read_many(
            &dir,
            &json!({ "paths": ["a.txt", "missing.txt"], "textBudget": 100 }),
        )
        .unwrap();
        let rs = mixed["results"].as_array().unwrap();
        assert_eq!(rs[0]["ok"], Value::Bool(true));
        assert!(rs[1]["error"].is_string());
        assert_eq!(mixed["complete"], Value::Bool(false));

        let all = read_many(
            &dir,
            &json!({ "paths": ["a.txt", "b.txt"], "textBudget": 100 }),
        )
        .unwrap();
        assert_eq!(all["complete"], Value::Bool(true));
        assert_eq!(all["budgetUsed"], 10);
    }

    #[test]
    fn read_decodes_utf16_with_bom() {
        let dir = fixture("utf16");
        let file = dir.join("u16.txt");
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        bytes.extend(
            "你好utf16\r\nsecond\r\n"
                .encode_utf16()
                .flat_map(|u| u.to_le_bytes()),
        );
        fs::write(&file, bytes).unwrap();
        let out = read(&dir, &json!({ "path": "u16.txt" })).unwrap();
        assert!(out["text"].as_str().unwrap().contains("utf16"));
        assert_eq!(out["bytes"], fs::read(&file).unwrap().len());
        assert_eq!(out["totalLines"], 2);
    }

    #[test]
    fn search_groups_sum_invalid_and_case() {
        let dir = fixture("search");
        fs::write(
            dir.join("s.log"),
            "INFO hit count=3\nERROR hit count=abc\nINFO hit count=4\nINFO hit count=NaN\n",
        )
        .unwrap();
        let out = search(
            &dir,
            &json!({
                "path": ".",
                "pattern": "(?P<level>INFO|ERROR) hit count=(?P<n>[0-9]+|abc|NaN)",
                "capture": "level",
                "sumCapture": "n",
            }),
        )
        .unwrap();
        assert_eq!(out["complete"], Value::Bool(true));
        let info = out.to_string();
        assert!(info.contains("\"totalMatches\":4"), "{info}");
        assert!(info.contains("\"invalidNumeric\":2"), "{info}"); // abc + NaN
        assert!(info.contains("\"sum\":7.0"), "{info}");

        // caseSensitive 默认 false：大小写混排都能命中
        fs::write(dir.join("case.txt"), "Row one\nrow two\n").unwrap();
        let ci = search(&dir, &json!({ "path": "case.txt", "pattern": "ROW" })).unwrap();
        assert_eq!(ci["patterns"][0]["totalMatches"], 2);
        let cs = search(
            &dir,
            &json!({ "path": "case.txt", "pattern": "Row", "caseSensitive": true }),
        )
        .unwrap();
        assert_eq!(cs["patterns"][0]["totalMatches"], 1);
    }

    #[test]
    fn search_large_file_full_scan_no_tail_truncation() {
        let dir = fixture("search-large");
        let file = dir.join("big.log");
        let mut body = String::from("NEEDLE at start\n");
        // 每行 ~900 字节，5000 行约 4.5MiB，越过 4MiB。
        let filler = "x".repeat(900);
        for i in 0..5000 {
            body.push_str(&format!("filler {i} {filler}\n"));
        }
        body.push_str("NEEDLE at end\n");
        fs::write(&file, body).unwrap();
        assert!(fs::metadata(&file).unwrap().len() > 4 * 1024 * 1024);

        let out = search(
            &dir,
            &json!({ "path": "big.log", "pattern": "NEEDLE at \\w+" }),
        )
        .unwrap();
        assert_eq!(out["complete"], Value::Bool(true));
        assert_eq!(out["filesScanned"], 1);
        assert_eq!(
            out["patterns"][0]["totalMatches"], 2,
            "末尾匹配必须命中，证明无尾截断"
        );
        let samples = out["patterns"][0]["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 2);
    }

    #[test]
    fn search_dedups_dir_and_direct_path_and_utf16_scan() {
        let dir = fixture("search-dedup");
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        bytes.extend(
            "alpha tagged one\ntagged two\n"
                .encode_utf16()
                .flat_map(|u| u.to_le_bytes()),
        );
        fs::write(dir.join("u16.log"), bytes).unwrap();

        let out = search(
            &dir,
            &json!({
                "paths": [".", "u16.log"],
                "recursive": true,
                "pattern": "tagged",
            }),
        )
        .unwrap()
        .to_string();
        assert!(
            out.contains("\"filesScanned\":1"),
            "目录展开与直接路径应去重：{out}"
        );
        assert!(
            out.contains("\"totalMatches\":2"),
            "UTF-16 内容必须被解码扫描：{out}"
        );
        assert!(out.contains("\"complete\":true"), "{out}");
    }
}
