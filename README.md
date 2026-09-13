# Sleepy Doll

面向 BetterGI（BGI）的本地桌面 Agent。把能力发现、状态观测、任务恢复和结果验证对齐 BGI 场景。

通过随附的注入桥连接原版 BetterGI，无需分发改版宿主。桥在本机提供 `/bridge/v1` 接口，
连接开关位于界面的「BetterGI」页面。

## 能做什么

- **对话驱动**：说清目标即可，它自己规划步骤；要操作游戏时会先停下来征求同意。
- **五种模型协议**：OpenAI Responses、OpenAI-compatible Chat、Anthropic Messages、
  Google Gemini、Ollama。
- **技能与插件**：技能是按需加载的操作说明；插件可以增加 HTTP 工具和 MCP 子进程。
- **可重跑的流程**：验证成功过的运行可以存下来，之后一键重跑，全程不调用模型。
- **每一步可核对**：工具调用、执行步骤和验证结果都进时间线；成功必须有验证证据，
  结果无法确认时不会被当成成功。

## 构建

需要 Rust stable（edition 2024）、Node.js 22+、.NET 8 SDK、Visual Studio 2022 C++ Build Tools。

```bash
npm ci
build-desktop.cmd
```

产物位于 `dist\Sleepy-Doll\`：

```text
dist\Sleepy-Doll\
  sleepy-doll.exe        主程序
  BgiBridge.*            桥组件，必须与主程序同目录
  bridge.config.json     首次启动由程序填入 token
```

分发时压缩该目录，解压后直接得到上述文件夹。`target\` 是 Cargo 的中间目录，不参与分发。

每次构建都会重建整个 `dist\`，其中的 `user\`（配置、模型密钥、会话数据库）会一并清除。

## 运行

1. 运行 `dist\Sleepy-Doll\sleepy-doll.exe`，接受启动时的 Windows 管理员权限提示。
   该提示仅在首次出现，之后注入 BetterGI 不再提权。
2. 启动 BetterGI。
3. 在界面「BetterGI」页面点击「连接 BetterGI」。

地址与凭据自动配置。关闭开关后桥拒绝新操作，已启动的 BetterGI 任务可能继续运行；
BetterGI 重启后点击「重新连接」。

配置、模型密钥和会话数据库位于 `dist\Sleepy-Doll\user\`，解析顺序见
[配置与数据存放](./docs/configuration.md)。

## 离线开发

```bash
npm run mock                 # 模拟模型 API 与 BGI Bridge
npm run dev                  # Vite 开发服务器
```

## 注意事项

- 仓库里没有 BGI 本体，因此无法进行实机键鼠、截图或路线验收。
- 界面里填写的模型密钥会**明文**写入 `user/config.json`，该文件不做权限加固。
  长期使用建议改用 `${ENV:...}` 引用，并确认目录不对其他账户开放。
- 插件自述的 `readOnly` 被当作可信输入，会跳过授权询问。不要启用来源不明的插件。

其余边界见 [架构与实施边界](./docs/architecture.md)。

## 文档

| 文档 | 内容 |
|---|---|
| [配置与数据存放](./docs/configuration.md) | 配置解析、模型配置、密钥 |
| [Skill 与 Plugin 格式](./docs/extensions.md) | 扩展的开发格式与执行策略 |
| [开发环境](./docs/development.md) | 构建、测试、离线 Mock Backend |
| [Agent Runtime v2](./docs/runtime-v2.md) | 执行、存储与恢复语义 |
| [Agent Kernel](./docs/agent-kernel.md) | 领域 Plugin 的 Adapter 协议 |
| [架构与实施边界](./docs/architecture.md) | 进程模型、决策执行、能力边界 |
| [实施总览](./docs/bgi-implementation-plan.md) | BGI 侧需要提供的接口 |
| [BGI 源码观察](./docs/bgi-source-observations.md) | BetterGI 源码静态观察记录 |
| [验证记录](./docs/runtime-validation.md) | 自动化验证结果 |

## 许可

MIT
