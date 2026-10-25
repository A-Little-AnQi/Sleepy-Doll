//! 通用公开网页读取；不携带模型鉴权、Cookie或本机文件。
use crate::error::{Error, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::IpAddr;
use std::{collections::BTreeSet, net::SocketAddr, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) fn public_url(text: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(text)
        .map_err(|_| Error::Tool("需要完整的公开网页 HTTP(S) 地址。".into()))?;
    let host = url.host_str().unwrap_or("").trim_matches(['[', ']']);
    let private = host.is_empty()
        || !host.contains('.') && host.parse::<IpAddr>().is_err()
        || host == "localhost"
        || [".localhost", ".local", ".internal"]
            .iter()
            .any(|s| host.ends_with(s))
        || host.parse::<IpAddr>().is_ok_and(|ip| match ip {
            IpAddr::V4(ip) => {
                let o = ip.octets();
                ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_broadcast()
                    || ip.is_documentation()
                    || o[0] == 0
                    || o[0] >= 224
                    || o[0] == 100 && (64..=127).contains(&o[1])
                    || o[0] == 198 && (18..=19).contains(&o[1])
            }
            IpAddr::V6(ip) => {
                // 只接受全球单播地址；映射/兼容地址及保留网段不委托给远端读取。
                (ip.segments()[0] & 0xe000) != 0x2000
                    || ip.segments()[0] == 0x2001 && ip.segments()[1] == 0xdb8
            }
        });
    let sensitive = url.query_pairs().any(|(key, _)| {
        let key = key.to_ascii_lowercase();
        [
            "token",
            "key",
            "secret",
            "password",
            "signature",
            "credential",
            "session",
            "auth",
        ]
        .iter()
        .any(|word| key.contains(word))
    });
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || private
        || sensitive
        || url.port().is_some_and(|p| p != 80 && p != 443)
    {
        return Err(Error::Tool(
            "联网工具只接受无凭据的公开 HTTP(S) 网页；不发送本机、私网、非常用端口或带凭据的地址。"
                .into(),
        ));
    }
    url.set_fragment(None);
    Ok(url)
}

pub(crate) fn domains(args: &Value, key: &str) -> Result<Vec<String>> {
    args[key]
        .as_array()
        .into_iter()
        .flatten()
        .map(|domain| {
            let raw = domain.as_str().unwrap_or("");
            let url = public_url(&format!("https://{raw}"))?;
            if url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
                || raw.contains(['/', ':', '#', '?', '@'])
            {
                return Err(Error::Tool(
                    "域名筛选填写纯域名，不包含协议、端口或路径。".into(),
                ));
            }
            Ok(url.host_str().unwrap().to_owned())
        })
        .collect()
}

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
            "该链接不是可直接读取的文本页面，未把二进制当作正文。".into(),
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

pub(crate) struct Download {
    pub requested: url::Url,
    pub final_url: url::Url,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

pub(crate) async fn download(
    text: &str,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Download> {
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
                "网页当前无法读取（HTTP {}）。",
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
        return Ok(Download {
            requested,
            final_url: target,
            content_type,
            bytes,
        });
    }
    Err(Error::Tool("网页重定向次数过多，未继续访问。".into()))
}

pub(crate) async fn fetch(
    text: &str,
    timeout: Duration,
    cancel: &CancellationToken,
) -> Result<Page> {
    let data = download(text, timeout, cancel).await?;
    tokio::task::spawn_blocking(move || {
        page_from_bytes(
            &data.requested,
            &data.final_url,
            &data.content_type,
            &data.bytes,
        )
    })
    .await
    .map_err(|_| Error::Tool("网页解析任务中断".into()))?
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
