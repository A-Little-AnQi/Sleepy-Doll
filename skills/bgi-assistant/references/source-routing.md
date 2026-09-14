# 三个 BetterGI 仓库的检索路由

这些路径用于接入本地或远程知识检索。路径可能随版本微调，应先搜索文件名或标题，不因某个固定路径不存在就停止。

## 用户文档 `bettergi-docs`

| 问题领域 | 优先路径 |
| --- | --- |
| 安装、启动、截图、系统要求 | `src/quickstart.md`、`src/faq.md`、`src/download.md` |
| 功能总览 | `src/doc.md`、`src/feats/README.md` |
| 实时任务 | `src/feats/timer/` |
| 独立任务 | `src/feats/task/` |
| 调度器、JS、地图追踪、键鼠、仓库 | `src/feats/autos/` |
| 操控辅助与宏 | `src/feats/macro/` |
| 命令行、桌面分身 | `src/feats/command/` |
| 分辨率、文件、按键码 | `src/feats/append/` |
| 通知、远程和专项教程 | `src/tutorial/` |
| JS API、路径制作、识别、任务调用 | `src/dev/` |

先搜索标题和章节，再读取完整小节。FAQ 用于已知问题，不应压过某项功能自己的文档。

## 本体 `better-genshin-impact`

| 要核对的内容 | 优先路径 |
| --- | --- |
| 当前页面和可见选项 | `BetterGenshinImpact/View/Pages/`、`View/Windows/` |
| 页面命令和联动行为 | `BetterGenshinImpact/ViewModel/Pages/`、`ViewModel/Windows/` |
| 全局设置字段与默认值 | `BetterGenshinImpact/Core/Config/AllConfig.cs` 及同目录具体配置类 |
| 独立/实时任务行为和参数 | `BetterGenshinImpact/GameTask/` |
| 一条龙 | `View/Pages/OneDragonFlowPage.xaml`、`ViewModel/Pages/OneDragonFlowViewModel.cs`、`Core/Config/OneDragonFlowConfig.cs` |
| 调度器和配置组 | `View/Pages/ScriptControlPage.xaml`、`ViewModel/Pages/ScriptControlViewModel.cs` |
| 仓库更新、订阅和读取 | `Core/Script/ScriptRepoUpdater.cs`、`Core/Script/WebView/RepoWebBridge.cs` |
| 页面导航 | `App.xaml.cs`、`Service/ApplicationHostService.cs`、对应 ViewModel |
| 服务和通知渠道 | `BetterGenshinImpact/Service/` |

用户文档没有覆盖新版功能时，先看 XAML 中的真实标签和提示，再看 ViewModel 命令、配置默认值与任务实现。不要只凭类名推断用途。

## 脚本仓库 `bettergi-scripts-list`

| 内容 | 优先路径 |
| --- | --- |
| 仓库树、节点、描述和更新时间 | `repo.json` |
| 地图追踪 | `repo/pathing/<分类>/<目标>/` |
| JavaScript 脚本 | `repo/js/<脚本>/manifest.json`、`settings.json`、`README.md`、`main.js` 与引用模块 |
| 战斗策略 | `repo/combat/` |
| 七圣召唤策略 | `repo/tcg/` |
| BetterGI JS 可用 API | `bettergi.d.ts` |

地图追踪选择先看 `repo.json` 的目录树，再读取完整候选父节点下的 README 与叶子元数据。叶子文件用于判断要求和内容，默认执行仍是目标目录或作者包父节点。
