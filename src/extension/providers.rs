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
