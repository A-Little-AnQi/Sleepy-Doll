# Sleepy Doll

面向 BetterGI（BGI）的本地桌面 Agent。借鉴 Codex 的运行方式，把能力发现、状态观测、任务恢复
和结果验证对齐 BGI 场景。

本仓库只实现外部 Agent，不修改 BetterGI 本体。BGI 侧需要按
[实施总览](./docs/bgi-implementation-plan.md) 提供 `/bridge/v1` HTTP/JSON 接口。

## 能做什么

- **对话驱动**：说清目标即可，它自己规划步骤；要操作游戏时会先停下来征求同意。
- **五种模型协议**：OpenAI Responses、OpenAI-compatible Chat、Anthropic Messages、
  Google Gemini、Ollama。
- **技能与插件**：技能是按需加载的操作说明；插件可以增加 HTTP 工具和 MCP 子进程。
- **可重跑的流程**：验证成功过的运行可以存下来，之后一键重跑，全程不调用模型。
- **每一步可核对**：工具调用、执行步骤和验证结果都进时间线；成功必须有验证证据，
  结果无法确认时不会被当成成功。

## 快速开始

需要 Rust stable（edition 2024）与 Node.js 22+。

```bash
npm ci
npm run check

cargo run --release          # 使用 <可执行文件目录>/user/config.json
```

首次启动会把 `sleepy-doll.config.example.json` 写成 `user/config.json`，模型密钥、会话
数据库和运行事件都只写在这个目录下。解析顺序、字段说明和密钥处理见
[配置与数据存放](./docs/configuration.md)。

要在浏览器里离线调试，用 Mock Backend：

```bash
npm run mock                 # 模拟模型 API 与 BGI Bridge
npm run dev                  # Vite 开发服务器
```

## 注意事项

- 仓库里没有 BGI 本体，因此无法进行实机键鼠、截图或路线验收。
- 界面里填写的模型密钥会**明文**写入 `user/config.json`，且该文件不做权限加固。
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
