/** 技能、工具和快捷任务只认提供方。插件开关只负责引入，不负责连接。 */

import type { Bootstrap } from "./types";

export const HOST_PROVIDER = "bgi";

export function isHostProvider(id: string) {
  return id === HOST_PROVIDER;
}

/** 宿主插件默认开启。后端没带回条目时也按已引入处理。 */
export function hostPluginEnabled(bootstrap: Pick<Bootstrap, "plugins">) {
  const host = bootstrap.plugins.find(
    (plugin) => plugin.host || isHostProvider(plugin.manifest.id),
  );
  return host ? host.configuredEnabled !== false : true;
}

export function providerOfTool(name: string, source: string) {
  if (source === "core:bgi" || name.startsWith("bgi.")) return HOST_PROVIDER;
  if (source.startsWith("plugin:")) {
    const id = source.slice("plugin:".length).split(":")[0];
    return id || undefined;
  }
}
