//! 通用公开网页读取；不携带模型鉴权、Cookie或本机文件。
use crate::{
    error::{Error, Result},
    model::web::public_url,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::SocketAddr, time::Duration};
use tokio_util::sync::CancellationToken;

const MAX_BYTES: usize = 10 * 1024 * 1024;

fn approved_address(url: &url::Url, address: &SocketAddr) -> bool {
    let synthetic = match address.ip() {
        std::net::IpAddr::V4(ip) => ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]),
        _ => false,
    };
    // TUN 的域名解析可返回保留 Fake-IP，仍使用原域名的 TLS/SNI 和证书校验。
    // 不接受直接指定该网段，也不放行真正的私网或元数据地址。
    if synthetic
        && url.scheme() == "https"
        && url
            .host_str()
            .is_some_and(|host| host.parse::<std::net::IpAddr>().is_err())
    {
        return true;
    }
    public_url(&format!("https://{}/", SocketAddr::new(address.ip(), 443))).is_ok()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page {
    pub url: String,
    pub final_url: String,
    pub title: String,
    pub content_type: String,
    pub text: String,
    pub links: Vec<String>,
    pub fetched_at: i64,
}

fn clean_html(html: &str, base: &url::Url) -> Result<(String, String, Vec<String>)> {
    let cfg = html2text::config::plain()
        .allow_width_overflow()
        .link_footnotes(true);
    let dom = cfg
        .parse_html(html.as_bytes())
        .map_err(|_| Error::Tool("网页HTML无法解析".into()))?;
    let mut stack = vec![dom.document.clone()];
    let mut links = BTreeSet::new();
    while let Some(node) = stack.pop() {
        node.children.borrow_mut().retain(|child| {
            if let html2text::Element { name, attrs, .. } = &child.data {
                if matches!(
                    &*name.local,
                    "script"
                        | "style"
                        | "noscript"
                        | "iframe"
                        | "object"
                        | "embed"
                        | "nav"
                        | "footer"
                ) {
                    return false;
                }
                if attrs.borrow().iter().any(|attr| {
                    &*attr.name.local == "hidden"
                        || &*attr.name.local == "aria-hidden" && &*attr.value == "true"
                }) {
                    return false;
                }
            }
            true
        });
        if let html2text::Element { name, attrs, .. } = &node.data
            && &*name.local == "a"
        {
            for attr in attrs
                .borrow()
                .iter()
                .filter(|attr| &*attr.name.local == "href")
            {
                if let Ok(url) = base.join(&attr.value)
                    && let Ok(url) = public_url(url.as_str())
                {
                    links.insert(url.to_string());
                }
            }
        }
        stack.extend(node.children.borrow().iter().cloned());
    }
    let tree = cfg
        .dom_to_render_tree(&dom)
        .map_err(|_| Error::Tool("网页正文无法转换".into()))?;
    let text = cfg
        .render_to_string(tree, 120)
        .map_err(|_| Error::Tool("网页正文无法转换".into()))?;
    let title = regex::Regex::new(r"(?is)<title\b[^>]*>(.*?)</title>")
        .unwrap()
        .captures(html)
        .and_then(|capture| html2text::from_read(capture[1].as_bytes(), 120).ok())
        .unwrap_or_default()
        .trim()
        .chars()
        .take(500)
        .collect();
    Ok((title, text, links.into_iter().collect()))
}

pub(crate) fn page_from_bytes(
    requested: &url::Url,
    final_url: &url::Url,
    content_type: &str,
    bytes: &[u8],
) -> Result<Page> {
    let is_text = content_type.starts_with("text/")
        || content_type.contains("json")
        || content_type.contains("xml")
        || content_type.is_empty();
    if !is_text {
        return Err(Error::Tool(
            "该链接不是可直接读取的文本网页；PDF等内容可显式选择provider后端，未把二进制当作正文。"
                .into(),
        ));
    }
    let decoded = String::from_utf8_lossy(bytes);
    let (title, text, links) = if content_type.contains("html")
        || content_type.is_empty() && decoded.trim_start().starts_with('<')
    {
        clean_html(&decoded, final_url)?
    } else {
        let text = if content_type.contains("json") {
            serde_json::from_str::<serde_json::Value>(&decoded)
                .ok()
                .and_then(|value| serde_json::to_string_pretty(&value).ok())
                .unwrap_or_else(|| decoded.into_owned())
        } else {
            decoded.into_owned()
        };
        (String::new(), text, vec![])
    };
    Ok(Page {
        url: requested.to_string(),
        final_url: final_url.to_string(),
        title,
        content_type: content_type.into(),
        text,
        links,
        fetched_at: time::OffsetDateTime::now_utc().unix_timestamp(),
    })
}

async fn response(
    url: &url::Url,
    peers: &[SocketAddr],
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<reqwest::Response> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::Tool("网页地址缺少域名".into()))?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(20))
        .timeout(timeout)
        .resolve_to_addrs(host, peers)
        .build()?;
    let request = client
        .get(url.as_str())
        .header("user-agent", "Sleepy-Doll/0.1 WebFetch")
        .header(
            "accept",
            "text/markdown,text/plain;q=0.9,text/html;q=0.8,application/json;q=0.7",
        );
    tokio::select! {_ = cancel.cancelled()=>Err(Error::Cancelled),result=request.send()=>result.map_err(Error::from)}
}

pub(crate) async fn fetch(
    text: &str,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Page> {
    let requested = public_url(text)?;
    let mut target = requested.clone();
    for _ in 0..10 {
        let host = target.host_str().unwrap();
        let port = target.port_or_known_default().unwrap();
        let addresses = tokio::select! {
            _=cancel.cancelled()=>return Err(Error::Cancelled),
            result=tokio::net::lookup_host((host,port))=>result?,
        }
        .collect::<Vec<_>>();
        if addresses.is_empty()
            || addresses
                .iter()
                .any(|address| !approved_address(&target, address))
        {
            return Err(Error::Tool(
                "网页域名解析到非公开地址，未访问本机或内网。".into(),
            ));
        }
        let result = response(&target, &addresses, timeout, cancel).await?;
        if result.status().is_redirection() {
            let location = result
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| Error::Tool("网页重定向没有目标地址".into()))?;
            target = public_url(
                target
                    .join(location)
                    .map_err(|_| Error::Tool("网页重定向地址无效".into()))?
                    .as_str(),
            )?;
            continue;
        }
        if !result.status().is_success() {
            return Err(Error::Tool(format!(
                "网页读取失败（HTTP {}），未自动切换收费后端。",
                result.status().as_u16()
            )));
        }
        let content_type = result
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if result
            .content_length()
            .is_some_and(|len| len > MAX_BYTES as u64)
        {
            return Err(Error::Tool("网页超过10MiB传输上限".into()));
        }
        let mut bytes = Vec::new();
        let mut stream = result.bytes_stream();
        while let Some(chunk) = tokio::select! {_=cancel.cancelled()=>return Err(Error::Cancelled),chunk=stream.next()=>chunk}
        {
            let chunk = chunk?;
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err(Error::Tool("网页超过10MiB传输上限".into()));
            }
            bytes.extend_from_slice(&chunk);
        }
        let requested = requested.clone();
        return tokio::task::spawn_blocking(move || {
            page_from_bytes(&requested, &target, &content_type, &bytes)
        })
        .await
        .map_err(|_| Error::Tool("网页解析任务中断".into()))?;
    }
    Err(Error::Tool("网页重定向次数过多，未继续访问。".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_ip_tls_domain_route_is_distinct_from_private_or_literal_targets() {
        let public = url::Url::parse("https://docs.example.org/").unwrap();
        assert!(approved_address(
            &public,
            &"198.18.7.41:443".parse().unwrap()
        ));
        assert!(!approved_address(
            &url::Url::parse("http://docs.example.org/").unwrap(),
            &"198.18.7.41:80".parse().unwrap()
        ));
        assert!(!approved_address(
            &url::Url::parse("https://198.18.7.41/").unwrap(),
            &"198.18.7.41:443".parse().unwrap()
        ));
        for ip in ["127.0.0.1:443", "10.0.0.1:443", "169.254.169.254:443"] {
            assert!(!approved_address(&public, &ip.parse().unwrap()));
        }
    }
    #[test]
    fn html_cleanup_keeps_document_structure_and_resolves_links() {
        let url = public_url("https://docs.example.org/guide/page").unwrap();
        let page=page_from_bytes(&url,&url,"text/html",br#"<title>Guide</title><nav>Menu spam</nav><main><h1>API</h1><p>First paragraph</p><script>evil()</script><pre>fn run() {}</pre><a href="../api">Contract</a><table><tr><th>key</th><th>value</th></tr><tr><td>mode</td><td>safe</td></tr></table></main><footer>Promotional footer</footer>"#).unwrap();
        assert!(!page.text.contains("Menu spam"));
        assert!(!page.text.contains("evil()"));
        assert!(!page.text.contains("Promotional footer"));
        assert!(page.text.contains("First paragraph"));
        assert!(page.text.contains("fn run()"));
        assert!(page.text.contains("safe"));
        assert!(page.links.contains(&"https://docs.example.org/api".into()));
    }
}
