//! 与模型协议无关的公开搜索；只发送查询，不携带模型凭据或完整会话。
use super::web::{domains, download, public_url};
use crate::{
    error::{Error, Result},
    extension::{mcp::McpClient, plugins::McpServerManifest},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Duration};
use tokio_util::sync::CancellationToken;

const EXA: &str = "https://mcp.exa.ai/mcp";

fn item(title: &str, url: &str, text: &str, date: &str) -> Option<Value> {
    let url = public_url(url.trim()).ok()?;
    if title.trim().is_empty() {
        return None;
    }
    Some(
        json!({"title":title.trim(),"url":url.as_str(),"excerpt":text.trim(),
        "publishedAt":if date.is_empty() || date=="N/A" {None}else{Some(date)}}),
    )
}

fn exa_items(value: &Value) -> Result<Vec<Value>> {
    if let Some(rows) = value["results"].as_array() {
        let parsed = rows
            .iter()
            .filter_map(|row| {
                item(
                    row["title"].as_str().unwrap_or(""),
                    row["url"].as_str().unwrap_or(""),
                    row["text"]
                        .as_str()
                        .or_else(|| row["content"].as_str())
                        .unwrap_or(""),
                    row["publishedDate"].as_str().unwrap_or(""),
                )
            })
            .collect::<Vec<_>>();
        if !rows.is_empty() && parsed.is_empty() {
            return Err(Error::Tool("搜索服务没有返回有效的公开来源".into()));
        }
        return Ok(parsed);
    }
    let text = value["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    let mut rows = Vec::new();
    for (index, record) in text.split("\n\n---\n\nTitle: ").enumerate() {
        let block = if index == 0 {
            let Some(block) = record.trim_start().strip_prefix("Title: ") else {
                continue;
            };
            block
        } else {
            record
        };
        let (header, body) = block
            .split_once("\nHighlights:\n")
            .or_else(|| block.split_once("\nText:\n"))
            .or_else(|| block.split_once("\nContent:\n"))
            .unwrap_or((block, ""));
        let mut lines = header.lines();
        let title = lines.next().unwrap_or("");
        let url = lines
            .clone()
            .find_map(|line| line.strip_prefix("URL: "))
            .unwrap_or("");
        let date = lines
            .find_map(|line| line.strip_prefix("Published: "))
            .unwrap_or("");
        if let Some(row) = item(title, url, body.trim_end_matches("\n---\n"), date) {
            rows.push(row);
        }
    }
    if rows.is_empty() && !text.to_lowercase().contains("no results") {
        return Err(Error::Tool("搜索服务没有返回可核对的来源".into()));
    }
    Ok(rows)
}

fn exa_arguments(schema: &Value, query: &str, count: usize, objective: &str) -> Result<Value> {
    let mut arguments = json!({"query":query});
    let properties = &schema["properties"];
    for (key, value) in [
        ("objective", json!(objective)),
        ("numResults", json!(count)),
        ("contextMaxCharacters", json!(24_000)),
    ] {
        if properties.get(key).is_some() {
            arguments[key] = value;
        }
    }
    if !crate::extension::validate(&arguments, schema, "$").is_empty() {
        return Err(Error::Tool("搜索服务的参数契约已变化".into()));
    }
    Ok(arguments)
}

async fn exa(
    query: &str,
    count: usize,
    objective: String,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Vec<Value>> {
    let manifest: McpServerManifest =
        serde_json::from_value(json!({"id":"web-search","transport":"streamableHttp",
        "url":EXA,"timeoutMs":timeout.as_millis() as u64}))?;
    let starting = tokio::task::spawn_blocking(move || McpClient::start(&manifest));
    let client = tokio::select! {
        _ = cancel.cancelled() => return Err(Error::Cancelled),
        result = starting => result.map_err(|_|Error::Tool("搜索连接任务中断".into()))??,
    };
    let schema = client.tool_schema("web_search_exa", cancel.clone()).await?;
    let arguments = exa_arguments(&schema, query, count, &objective)?;
    let response = client
        .call_tool("web_search_exa", arguments, cancel.clone())
        .await?;
    exa_items(&response)
}

fn descendants(root: &html2text::Handle) -> Vec<html2text::Handle> {
    let mut stack = vec![root.clone()];
    let mut nodes = Vec::new();
    while let Some(node) = stack.pop() {
        stack.extend(node.children.borrow().iter().rev().cloned());
        nodes.push(node);
    }
    nodes
}

fn has_class(node: &html2text::Handle, class: &str) -> bool {
    matches!(&node.data,html2text::Element{attrs,..} if attrs.borrow().iter().any(|a| &*a.name.local=="class" && a.value.split_whitespace().any(|v|v==class)))
}

fn plain_node(node: &html2text::Handle) -> String {
    let cfg = html2text::config::with_decorator(html2text::render::TrivialDecorator::new())
        .allow_width_overflow();
    let dom = html2text::RcDom {
        document: node.clone(),
        ..Default::default()
    };
    cfg.dom_to_render_tree(&dom)
        .and_then(|tree| cfg.render_to_string(tree, 120))
        .unwrap_or_default()
        .trim()
        .into()
}

fn ddg_items(bytes: &[u8]) -> Result<Vec<Value>> {
    let cfg = html2text::config::plain();
    let dom = cfg
        .parse_html(bytes)
        .map_err(|_| Error::Tool("搜索页面无法解析".into()))?;
    let mut rows = Vec::new();
    for node in descendants(&dom.document)
        .into_iter()
        .filter(|node| has_class(node, "result") && !has_class(node, "result--ad"))
    {
        let nodes = descendants(&node);
        let Some(anchor) = nodes.iter().find(|n| has_class(n, "result__a")) else {
            continue;
        };
        let html2text::Element { attrs, .. } = &anchor.data else {
            continue;
        };
        let href = attrs
            .borrow()
            .iter()
            .find(|a| &*a.name.local == "href")
            .map(|a| a.value.to_string())
            .unwrap_or_default();
        let link = url::Url::parse(&if href.starts_with("//") {
            format!("https:{href}")
        } else {
            href
        })
        .ok()
        .map(|url| {
            if url
                .host_str()
                .is_some_and(|host| host == "duckduckgo.com" || host.ends_with(".duckduckgo.com"))
            {
                url.query_pairs()
                    .find(|(key, _)| key == "uddg")
                    .map(|(_, url)| url.into_owned())
                    .unwrap_or_default()
            } else {
                url.to_string()
            }
        })
        .unwrap_or_default();
        let excerpt = nodes
            .iter()
            .find(|n| has_class(n, "result__snippet"))
            .map(plain_node)
            .unwrap_or_default();
        if let Some(row) = item(&plain_node(anchor), &link, &excerpt, "") {
            rows.push(row)
        }
    }
    if rows.is_empty()
        && !String::from_utf8_lossy(bytes)
            .to_lowercase()
            .contains("no results")
    {
        return Err(Error::Tool("搜索页面未提供结果，可能需要人工验证".into()));
    }
    Ok(rows)
}

pub(crate) async fn search(
    args: &Value,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Value> {
    let query = args["query"]
        .as_str()
        .filter(|query| !query.trim().is_empty())
        .ok_or_else(|| Error::Tool("请提供搜索内容".into()))?;
    let allowed = domains(args, "allowedDomains")?;
    let blocked = domains(args, "blockedDomains")?;
    if !allowed.is_empty() && !blocked.is_empty() {
        return Err(Error::Tool("允许与排除域名不能同时使用".into()));
    }
    let count = args["limit"].as_u64().unwrap_or(8).clamp(1, 30) as usize;
    let objective = format!(
        "查找查询所需的原始资料与具体事实，优先官方文档、原始数据和资料站；保留真实来源、日期与限定条件，不用广告或转载数量判断真假。允许域名：{}；排除域名：{}",
        json!(allowed),
        json!(blocked)
    );
    let mut warnings = Vec::new();
    let (backend, mut items) = match tokio::time::timeout(
        timeout.min(Duration::from_secs(45)),
        exa(
            query,
            count,
            objective,
            timeout.min(Duration::from_secs(40)),
            cancel,
        ),
    )
    .await
    {
        Ok(Ok(rows)) => ("exaMcp", rows),
        _ => {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            warnings.push(json!({"source":"exaMcp","status":"unavailable"}));
            let mut url = url::Url::parse("https://html.duckduckgo.com/html/").unwrap();
            let mut filtered = query.to_owned();
            if !allowed.is_empty() {
                filtered.push_str(&format!(
                    " ({})",
                    allowed
                        .iter()
                        .map(|d| format!("site:{d}"))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                ));
            }
            for domain in &blocked {
                filtered.push_str(&format!(" -site:{domain}"));
            }
            url.query_pairs_mut().append_pair("q", &filtered);
            let page = download(url.as_str(), timeout.min(Duration::from_secs(25)), cancel)
                .await
                .map_err(|error| {
                    if cancel.is_cancelled() {
                        return error;
                    }
                    Error::Tool(
                        "公开搜索暂时不可用，尚未取得资料。重复改关键词不能解决服务连接问题。"
                            .into(),
                    )
                })?;
            let rows = tokio::task::spawn_blocking(move || ddg_items(&page.bytes))
                .await
                .map_err(|_| Error::Tool("搜索页面解析中断".into()))??;
            ("duckduckgoHtml", rows)
        }
    };
    let mut seen = BTreeSet::new();
    let original = items.len();
    items.retain(|row| {
        let Ok(url) = public_url(row["url"].as_str().unwrap_or("")) else {
            return false;
        };
        let host = url.host_str().unwrap_or("");
        let matches = |d: &String| host == d || host.ends_with(&format!(".{d}"));
        (allowed.is_empty() || allowed.iter().any(matches))
            && !blocked.iter().any(matches)
            && seen.insert(url.to_string())
    });
    let filtered = original != items.len();
    items.truncate(count);
    Ok(
        json!({"backend":backend,"operation":"search","searchExecuted":true,"searchVerified":true,
        "untrusted":true,"authority":"externalReferenceOnly","verified":false,"complete":!(filtered && items.is_empty()),
        "found":if filtered && items.is_empty() {None}else{Some(!items.is_empty())},"items":items,"providerSummary":"","sourceCount":seen.len(),
        "independenceVerified":false,"scopeFiltered":filtered,"warnings":warnings,
        "retrievedAt":time::OffsetDateTime::now_utc().unix_timestamp()}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exa_current_and_legacy_contracts_use_only_declared_arguments() {
        let current = json!({"type":"object","properties":{"query":{"type":"string"},"objective":{"type":"string"}},"required":["query","objective"],"additionalProperties":false});
        assert_eq!(
            exa_arguments(&current, "a query", 8, "find primary sources").unwrap(),
            json!({"query":"a query","objective":"find primary sources"})
        );
        let rows=exa_items(&json!({"content":[{"type":"text","text":"Title: Original\nURL: https://docs.example.org/a\nPublished: N/A\nHighlights:\nA precise fact\n\n---\n\nTitle: Other\nURL: https://other.example.org/b\nPublished: 2026-01-01\nText:\nAnother fact"}]})).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(
            rows[0]["excerpt"]
                .as_str()
                .unwrap()
                .contains("precise fact")
        );
        assert!(
            exa_items(&json!({"content":[{"text":"invented answer without source"}]})).is_err()
        );
    }
    #[test]
    fn ddg_parser_excludes_ads_and_decodes_only_real_destination_links() {
        let rows=ddg_items(br#"<div class="result result--ad"><a class="result__a" href="https://ad.example.org">Ad</a></div><div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.example.org%2Fguide">Official &amp; current</a><div class="result__snippet">A useful &lt;fact&gt;</div></div>"#).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["url"], "https://docs.example.org/guide");
        assert_eq!(rows[0]["title"], "Official & current");
        assert!(rows[0]["excerpt"].as_str().unwrap().contains("useful"));
    }
}
