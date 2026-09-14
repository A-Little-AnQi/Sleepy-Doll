# 资料来源与更新时间

本技能的领域知识提炼自 2026-09-14 获取的以下公开仓库。它记录稳定概念和决策规则，不复制会频繁变化的完整接口清单。

- BetterGI 用户文档：<https://github.com/huiyadanli/bettergi-docs>，提交 `26dbdf91584934a71768e85cccffb1412a425045`。重点参考快速上手、常见问题、实时任务、独立任务、调度器、地图追踪、脚本仓库和开发脚本文档。
- BetterGI 本体：<https://github.com/babalae/better-genshin-impact>，提交 `fca9e5cbe763661e0d51576fdbdb5ca87d07f07e`。重点核对 `GameTask`、`Service`、`ViewModel`、`ScriptRepoUpdater`、`RepoWebBridge`、页面导航、任务调度与配置结构。
- BetterGI 脚本仓库：<https://github.com/babalae/bettergi-scripts-list>，提交 `6c2899f36776566eb7151336e25567d9eee1e10d`。重点核对地图追踪路线 `info` 元数据、JavaScript 的 `manifest.json`/`settings.json`/README/源码、战斗策略、七圣策略和 `bettergi.d.ts` 中的高层脚本能力。

当官方行为变化时，优先更新 [bgi-domain.md](bgi-domain.md) 与 [troubleshooting.md](troubleshooting.md)。具体接口始终由运行时能力目录提供，不在本技能中手工追版本。
