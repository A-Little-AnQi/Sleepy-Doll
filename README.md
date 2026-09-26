# Sleepy Doll

用于使用其他游戏脚本工具的通用本地桌面 Agent。

用户用自然语言提出目标，Sleepy Doll 负责理解请求、检索资料、规划步骤、调用工具和核对结果。
具体的游戏操作由接入的脚本工具执行，Sleepy Doll 管理这些工具的资源、配置和任务。

**目前已适配的工具是 BetterGI（BGI）。** 通用 Agent 运行时与游戏工具的适配层分开：
模型协议、对话、规划、权限、执行记录和恢复由运行时提供；工具特有的术语、脚本资料、配置格式
与操作流程由对应插件和适配层提供。接入其他游戏脚本工具需要相应适配，目前不宣称已有其他工具的完整支持。

## 能做什么

- **对话驱动**：给出目标，由 Agent 查找可用资源、完成配置并调用接入工具执行。
- **五种模型协议**：OpenAI Responses、OpenAI-compatible Chat、Anthropic Messages、
  Google Gemini、Ollama。
- **技能与插件**：技能提供领域操作说明，插件通过 HTTP 工具、MCP 或 Adapter 接入其他工具。
- **权限控制**：支持只读、请求审批与完全控制；写操作按当前权限模式处理。
- **可重复执行的流程**：可将符合条件的运行保存为快捷任务；确定性任务再次执行时无需调用模型。
- **运行记录**：保存对话、工具调用、任务状态与结果证据，支持取消、恢复及结果核对。

## 当前的 BetterGI 适配

BGI 适配包含 `plugins/bgi/` 中的领域技能、`src/bridge/` 中的客户端与工具接入，以及
`bgi-bridge/` 中的进程内桥。桥连接原版 BetterGI，无需分发改版宿主，在本机提供 `/bridge/v1` 接口。

这层适配用于读取脚本说明和源码、检索本机及中央仓库资源、管理配置、执行任务与读取日志。
具体操作能否完成，以当前桥的接口契约、资源和运行状态为准。BetterGI 的页面、目录结构和配置规则
属于这层适配，不应成为通用 Agent 的业务规则。

## 构建

当前完整发行构建面向 Windows，需要 Rust stable（edition 2024）、Node.js 22.13+、
.NET 8 SDK、Visual Studio 2022 C++ Build Tools。运行桌面程序需要 WebView2 Runtime。

```bash
npm ci
npm run dist
```

产物位于 `dist\Sleepy-Doll\`：

```text
dist\Sleepy-Doll\
  sleepy-doll.exe        主程序
  bridge\                桥组件：9 个 BgiBridge.* 文件与 bridge.config.json
  plugins\bgi\           随产品分发的插件：BetterGI 的领域说明
  user\                  用户数据，构建不碰它
```

`bridge\` 必须整目录保持在一起，主程序按同样的相对位置查找桥组件。`bridge.config.json`
首次连接时由程序写入 token。桥的运行期数据（日志、配置改动记录）不落在 `bridge\` 下，
统一在 `user\` 里，重装后仍然可用。

构建同时生成 `dist\Sleepy-Doll-<版本>-setup.exe`。分发使用安装包；本机工作目录可能已有
`user\`、桥缓存与 WebView2 数据，不应整目录压缩给其他用户。`target\` 是中间产物目录，不参与分发。

组装是**原地覆盖写入**，不先清空目录：`user\` 下是配置、模型密钥、会话数据库与日志，
重新构建不会动它。

## 安装

安装程序与主程序是同一套外壳，外观一致。它由 `build-desktop.cmd` 一并产出，没有单独的命令：

```cmd
build-desktop.cmd
```

输出 `dist\Sleepy-Doll-<版本>-setup.exe`。`D:` 是固定磁盘时默认装到 `D:\Sleepy Doll`，否则装到
`%LOCALAPPDATA%\Programs\Sleepy Doll`；用户选的目录若不以产品名结尾，安装程序会补上
`Sleepy Doll`。安装程序只打包上面列出的产品文件：`user\` 不在其列，覆盖安装不会动它，
卸载时会另行询问是否连同它一起删除，默认保留。

## 运行

1. 运行安装程序创建的快捷方式，或直接运行 `dist\Sleepy-Doll\sleepy-doll.exe`。
   当前 Windows 发行版启动时请求管理员权限，用于连接 BetterGI。
2. 在设置中添加模型，选择需要的权限模式，并启用要使用的插件。
3. 在对话里说明目标，由 Agent 通过已适配的工具处理。

使用当前 BGI 适配时，可在「BetterGI」页面连接或重新连接。程序可在已定位安装目录后自动启动
BetterGI；找不到安装位置时，先手动启动一次。地址与凭据自动配置。关闭连接后桥拒绝新操作，
已启动的 BetterGI 任务可能继续运行。

配置、模型密钥、会话数据库和日志位于 `dist\Sleepy-Doll\user\`，解析顺序见
[配置与数据存放](./docs/configuration.md)。

## 注意事项

- 本仓库不包含所接入游戏脚本工具的本体；真实游戏操作需要在对应工具和游戏环境中验证。
- 界面里填写的模型密钥会**明文**写入 `user/config.json`，该文件不做权限加固。
  长期使用建议改用 `${ENV:...}` 引用，并确认目录不对其他账户开放。
- 插件自述的 `readOnly` 被当作可信输入，会跳过授权询问。不要启用来源不明的插件，
  详见 [Skill 与 Plugin 格式](./docs/extensions.md)。

进程模型、执行与恢复语义见 [运行内核](./docs/internals.md)。

## 文档

| 文档 | 内容 |
|---|---|
| [配置与数据存放](./docs/configuration.md) | 配置解析、模型配置、密钥 |
| [Skill 与 Plugin 格式](./docs/extensions.md) | 扩展的开发格式与执行策略 |
| [开发环境](./docs/development.md) | 构建、测试、桥契约回归 |
| [运行内核](./docs/internals.md) | 进程模型、决策执行、存储与恢复 |
| [BGI 宿主契约](./docs/bgi/host-contracts.md) | 宿主约束、Job 与错误协议、并发与取消 |
| [BGI 全量功能清单](./docs/bgi/feature-coverage.md) | 源码命令、设置、页面、脚本 API 与资源模型字段 |
| [BGI 适配链路审计](./docs/bgi/integration-audit.md) | 桥与 skill 的链路、已修复缺口、验证范围与限制 |

## 许可

MIT
