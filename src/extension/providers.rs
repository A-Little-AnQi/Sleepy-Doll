//! 技能、工具和快捷任务只认识提供方，不直连某个宿主产品。
//!
//! 宿主桥登记为 `bgi`。插件开关只决定要不要把这个提供方引入产品；连接
//! 仍走提供方自己的设置页。

use std::collections::HashSet;

use serde_json::{Value, json};

/// 随产品提供的宿主提供方。工具名 `bgi.*`、source `core:bgi` 都归这里。
pub const HOST_PROVIDER: &str = "bgi";

pub fn is_host_provider(id: &str) -> bool {
    id == HOST_PROVIDER
}

/// 宿主插件默认开启。只有写进 `plugins.disabled` 才算关掉。
pub fn host_plugin_enabled(disabled: &[String]) -> bool {
    !disabled.iter().any(|id| id == HOST_PROVIDER)
}

/// 从工具名或注册来源推断提供方。Core 工具没有提供方。
pub fn provider_of_tool(name: &str, source: &str) -> Option<String> {
    if source == "core:bgi" || name.starts_with("bgi.") {
        return Some(HOST_PROVIDER.into());
    }
    if let Some(rest) = source.strip_prefix("plugin:") {
        return rest
            .split(':')
            .next()
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
    }
    None
}

/// 工具尚未登记时，仍按命名约定归到提供方，避免把内部工具名露给界面。
pub fn infer_provider(name: &str) -> Option<String> {
    provider_of_tool(name, "")
}

pub fn skill_unavailable_reason(requires_providers: &[String], online: &[&str]) -> String {
    if requires_providers.is_empty()
        || requires_providers
            .iter()
            .all(|provider| online.iter().any(|item| *item == provider))
    {
        String::new()
    } else {
        "需要先启用对应插件".into()
    }
}

/// 快捷任务缺依赖时的用户文案。不列出内部工具名。
pub fn missing_plugin_issue(tool_names: &[String], introduced: &HashSet<String>) -> String {
    if tool_names.is_empty() {
        return String::new();
    }
    if tool_names
        .iter()
        .any(|name| infer_provider(name).is_some_and(|provider| !introduced.contains(&provider)))
    {
        "需要先启用对应插件。".into()
    } else {
        "需要先连接工具才能运行。".into()
    }
}

pub fn host_plugin_view(enabled: bool) -> Value {
    json!({
        "manifest": {
            "id": HOST_PROVIDER,
            "name": "BetterGI",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "游戏自动化宿主",
        },
        "status": if enabled { "enabled" } else { "disabled" },
        "configuredEnabled": enabled,
        "host": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_plugin_is_on_until_disabled() {
        assert!(host_plugin_enabled(&[]));
        assert!(!host_plugin_enabled(&["bgi".into()]));
    }

    #[test]
    fn host_tools_map_to_the_host_provider() {
        assert_eq!(
            provider_of_tool("bgi.state.get", "core:bgi").as_deref(),
            Some(HOST_PROVIDER)
        );
        assert_eq!(
            infer_provider("bgi.user.list").as_deref(),
            Some(HOST_PROVIDER)
        );
    }

    #[test]
    fn plugin_source_maps_to_plugin_id() {
        assert_eq!(
            provider_of_tool("weather.http.get", "plugin:weather:http").as_deref(),
            Some("weather")
        );
    }

    #[test]
    fn user_copy_does_not_name_host_tools() {
        let empty = HashSet::new();
        let introduced = HashSet::from([HOST_PROVIDER.to_owned()]);
        let enable = missing_plugin_issue(&["bgi.api.invoke".into()], &empty);
        let connect = missing_plugin_issue(&["bgi.api.invoke".into()], &introduced);
        assert_eq!(enable, "需要先启用对应插件。");
        assert_eq!(connect, "需要先连接工具才能运行。");
        assert!(!enable.contains("bgi."));
        assert!(!connect.contains("BetterGI"));
        assert_eq!(
            skill_unavailable_reason(&["bgi".into()], &[]),
            "需要先启用对应插件"
        );
    }
}
