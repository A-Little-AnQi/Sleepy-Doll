//! 直接读取 BetterGI 当前接口目录；业务检索顺序由插件技能决定。
use crate::{error::Result, runtime::host::bridge::Bridge};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

pub async fn search_interface(
    arguments: &Value,
    bridge: &Bridge,
    cancel: &CancellationToken,
) -> Result<Value> {
    bridge.get(&catalog_path(arguments), cancel).await
}

fn catalog_path(arguments: &Value) -> String {
    let query = {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        query
            .append_pair("q", arguments["query"].as_str().unwrap_or(""))
            .append_pair(
                "limit",
                &arguments["limit"].as_u64().unwrap_or(8).to_string(),
            )
            .append_pair(
                "offset",
                &arguments["offset"].as_u64().unwrap_or(0).to_string(),
            );
        if let Some(group) = arguments["group"].as_str() {
            query.append_pair("group", group);
        }
        query.finish()
    };
    format!("/bridge/v1/catalog?{query}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn interface_query_preserves_unicode_pagination_and_group() {
        let path =
            catalog_path(&json!({"query":"兑换码 & 今日","offset":3,"limit":2,"group":"commands"}));
        let url = url::Url::parse(&format!("http://localhost{path}")).unwrap();
        let pairs = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(url.path(), "/bridge/v1/catalog");
        for (key, value) in [
            ("q", "兑换码 & 今日"),
            ("offset", "3"),
            ("limit", "2"),
            ("group", "commands"),
        ] {
            assert_eq!(pairs[key], value);
        }
    }
}
