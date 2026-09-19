//! 技能、工具和快捷任务只认识提供方，不直连某个宿主产品。
//!
//! 宿主桥登记为 `bgi`。连接走提供方自己的设置页。

use std::collections::HashSet;

/// 随产品提供的宿主提供方：工具名 `bgi.*` 与 source `core:bgi` 都归它。
pub const HOST_PROVIDER: &str = "bgi";

pub fn is_host_provider(id: &str) -> bool {
    id == HOST_PROVIDER
}

/// 宿主插件默认开启，写进 `plugins.disabled` 才算关掉。
pub fn host_plugin_enabled(disabled: &[String]) -> bool {
    !disabled.iter().any(|id| is_host_provider(id))
}

/// 插件是否被配置为启用：宿主插件默认开启，其余插件要写进 `plugins.enabled` 才装载。
pub fn plugin_enabled(id: &str, enabled: &[String], disabled: &[String]) -> bool {
    if is_host_provider(id) {
        return host_plugin_enabled(disabled);
    }
    enabled.iter().any(|entry| entry == id)
}

/// 从工具名或注册来源推断提供方，Core 工具没有提供方。
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

/// 工具尚未登记时，按命名约定推断提供方。
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

/// 快捷任务缺依赖时的用户文案，不列出内部工具名。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_plugin_is_on_until_disabled() {
        assert!(host_plugin_enabled(&[]));
        assert!(!host_plugin_enabled(&["bgi".into()]));
    }

    #[test]
    fn only_the_host_plugin_is_on_without_being_listed() {
        let none = Vec::new();
        assert!(plugin_enabled(HOST_PROVIDER, &none, &none));
        assert!(!plugin_enabled("weather", &none, &none));
        assert!(plugin_enabled("weather", &["weather".into()], &none));
        assert!(!plugin_enabled(HOST_PROVIDER, &none, &["bgi".into()]));
        // 列进 enabled 不能把一个被显式关掉的宿主插件重新打开。
        assert!(!plugin_enabled(
            HOST_PROVIDER,
            &["bgi".into()],
            &["bgi".into()]
        ));
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
