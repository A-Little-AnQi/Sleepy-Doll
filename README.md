# Sleepy Doll

Sleepy Doll 是一个面向 BetterGI（BGI）的本地桌面 Agent。它借鉴 Codex 的 Agent 运行方式，但把能力发现、状态观测、Job 恢复和结果验证优先对齐 BGI 场景。

当前仓库只实现外部 Agent，不修改 BetterGI 本体。BGI 侧需要按根目录的 [实施总览](./BGI-Agent-Implementation-Plan.md) 提供 `/bridge/v1` HTTP/JSON 接口。
Agent Framework 契约见 [Agent Kernel](./docs/agent-kernel.md)；当前 BetterGI 源码静态观察记录见 [BGI source observations](./docs/bgi-source-observations.md)。

## 当前能力

运行时已升级为持久化 Supervisor，完整接口、恢复语义和限制见 [Runtime v2](./docs/runtime-v2.md)。模型入口为纯文本，不包含图片上传或视觉模型能力。

- 原生桌面壳：`tao` 窗口 + `wry` WebView；Vite + React 页面通过 Wry IPC 调用 Rust，不暴露本机管理端口。
- 单模型入口：配置文件可保存多个模型，但 `activeModel` 必须且只能指向一个模型；界面切换后，新任务立即使用新模型。
- 模型协议：OpenAI Responses、OpenAI-compatible Chat Completions、Anthropic Messages、Google Gemini `generateContent`、Ollama Chat。
- Agent 执行：版本化计划、Schema 校验、对话内澄清/审批、单会话调度、可取消模型流、持久化事件和故障恢复；普通计划步骤由程序执行。
- Skills：递归发现 `SKILL.md`，读取 frontmatter，按显式 `$skill`、名称、描述与标签检索并按需装入上下文。
- Plugins：`.sleepy-doll-plugin/plugin.json` 可提供 HTTP 工具、Skills 和 MCP stdio Server；只有配置中显式启用的插件才会启动。
- Agent Kernel：领域 Plugin 可通过 `sleepy-adapter/1` 描述不透明资源、诊断与 MutationPlan；Core 统一完成快照、授权、提交、验证、回退和恢复，不内置领域文件格式。
- Verified Workflow：已验证计划可固化为带 Tool Contract、Provider/Resource 版本绑定的流程，之后由前端手动或调度执行，全程不调用模型。
- BGI 工具：状态读取、能力搜索/描述/调用、Job 查询/取消；Bridge 关闭或不可达时明确失败，不产生模拟成功结果。
- 持久化：SQLite 保存会话、运行、计划、尝试、审批、证据和事件；游戏写锁跨重启保留，成功必须有验证证据。
- 可视化控制台：对话与历史会话是一等入口；工具调用、运行/取消/失败状态直接进入时间线；模型、Skills、Plugins 和 Bridge 均有独立配置界面。

## 技术结构

```text
React UI ── Wry IPC ── Rust AppController
                         ├─ Supervisor + versioned plans + event journal
                         ├─ one active Model gateway ── reqwest stream ── model API
                         ├─ Skill registry
                         ├─ Plugin manager ── HTTP tools / MCP stdio
                         ├─ BGI execution / verification ── reqwest ── BGI Bridge
                         └─ SQLite store
```

Tokio 负责可取消网络请求、模型流、Job 等待、事件订阅和 MCP 进程。HTTP 插件保留兼容适配器；无法确认其停止时记录未知结果。

## 开发环境

- Rust stable，edition 2024
- Node.js 22+
- Windows 10/11：WebView2 Runtime（通常随系统或 Edge 安装）
- Linux 桌面开发：Wry 所需的 GTK/WebKitGTK 开发包

安装前端依赖并构建嵌入资源：

```bash
npm ci
npm run check
```

### 配置与数据存放

用户配置和数据固定存放在**安装目录下的 `user/`**：首次启动会把
`sleepy-doll.config.example.json` 作为模板写入 `user/config.json`，并创建
`user/skills`、`user/plugins`、`user/catalog`、`user/.sleepy-doll`。模型密钥、
会话数据库、运行事件都只写在这里，不会写进源码目录、工作目录或构建产物。

解析顺序：命令行第一个参数 → `SLEEPY_DOLL_CONFIG` → `<可执行文件目录>/user/config.json`。
安装目录不可写时（例如系统级安装）才退回 `%APPDATA%\Sleepy Doll\user`。

构建并运行：

```bash
npm ci
npm run check

cargo test --lib --no-default-features
cargo run --release                                  # 使用 <exe>/user/config.json
cargo run --release -- sleepy-doll.dev.config.json   # 使用仓库内的本地副本
```

要在仓库里调试本地 Mock 模型，先把 fixture 复制成不受版本控制的本地副本：

```bash
cp sleepy-doll.mock.config.json sleepy-doll.dev.config.json
```

`sleepy-doll.mock.config.json` 是纯离线 fixture：不含任何密钥，全部端点指向
`127.0.0.1`，既供 Mock 进程读取，也作为上面本地副本的来源。

桌面打包前必须先执行 `npm run build`，因为 release 二进制会通过 `rust-embed`
编译进 `ui-dist/` 的静态资源。

## 离线 Mock Backend

Mock Backend 是独立 Rust 进程，模拟模型 API、BGI Bridge 和浏览器开发模式下的 IPC 网关。
它只监听 `127.0.0.1`，不访问外网，模型用量固定为 0。

```bash
npm run mock
npm run dev
```

Mock 进程启动时会打印实际读取的配置路径与当前 `activeModel`。浏览器开发模式的 IPC 会把
模型请求转发给配置里的 `activeModel`，因此要完全离线，请让 `activeModel` 指向 Mock 模型。

开发 Mock 会模拟真实 LLM 的节奏：首个 Token 前约 220–510ms，流式输出按
1–5 个字符一块推进，块间延迟在约 50–110ms 内抖动；句子标点处停顿更久，偶发
260–640ms 的上游停顿。可通过 `SLEEPY_DOLL_MOCK_MODEL_DELAY_MS` 和
`SLEEPY_DOLL_MOCK_CHUNK_DELAY_MS` 调整两个基准值，设为 `0` 可完全关闭对应延迟与抖动。

浏览器开发模式会自动连接 `http://127.0.0.1:47124/ipc`；Wry 桌面模式仍使用原生 IPC，不受影响。

内置交流场景：

- 包含“短回复”：返回单句，用于检查紧凑消息布局。
- 包含“长回复”：返回多段、编号和长行，用于检查换行、滚动、固定输入区与长会话标题。
- 包含“状态”或“连接”：模型调用 `bgi.state.get`，再根据 Mock Bridge 返回生成总结。
- 包含“路线”：模型调用 `bgi.capability.search`，再生成能力目录总结。
- Bridge 能力 `mock.success`、`mock.unknown`、`mock.failure`、`mock.busy`、`mock.denied` 分别覆盖成功、结果未知、失败、忙碌和权限拒绝；Job 还支持运行中与取消。

运行全部离线测试：

```bash
cargo test --no-default-features
```

## 模型配置

`models` 是可选模型列表，`activeModel` 是唯一入口。例如：

```json
{
  "activeModel": "primary",
  "models": [
    {
      "id": "primary",
      "name": "Main model",
      "protocol": "openai-responses",
      "model": "your-model-id",
      "baseUrl": "https://api.openai.com/v1",
      "apiKey": "${ENV:OPENAI_API_KEY}",
      "headers": {},
      "options": { "maxOutputTokens": 8192 }
    }
  ]
}
```

OpenAI-compatible、代理网关和私有部署可通过 `baseUrl` 与 `headers` 配置。密钥字段支持完整的 `${ENV:VARIABLE_NAME}` 引用；IPC 的 bootstrap 响应不会返回密钥或自定义请求头。界面中填写的密钥写入 `user/config.json`，该文件不在源码仓库内，也不会随构建产物分发，因此长期使用建议只保留 `${ENV:...}` 引用，不落盘明文密钥。

`options.timeoutMs` 是空闲超时：连接建立、以及流式响应中相邻数据块之间的最长间隔。整轮总时长由
任务时限（`runtime.durationSec`）约束，因此长回复不会被请求级超时截断。

## Skill 格式

```markdown
---
name: my-skill
description: 何时应该使用这个 Skill
tags: bgi, route
---

# 完整操作说明
...
```

Skill 文件上限为 128 KiB，默认最多自动装入 4 个。目录中存在不代表全部内容会进入每次模型上下文。

## Plugin 格式

插件目录必须包含 `.sleepy-doll-plugin/plugin.json`：

```json
{
  "schemaVersion": 1,
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "skills": ["./skills"],
  "httpTools": [
    {
      "name": "lookup",
      "description": "查询本地数据",
      "inputSchema": { "type": "object" },
      "outputSchema": { "type": "object" },
      "method": "GET",
      "url": "http://127.0.0.1:9000/lookup",
      "execution": {
        "effect": "readOnly",
        "concurrencySafe": true,
        "maxResultChars": 12000,
        "deferred": true,
        "alwaysLoad": false,
        "searchHint": "查询本地索引"
      }
    }
  ],
  "mcpServers": [
    {
      "id": "local",
      "command": "my-mcp-server",
      "args": [],
      "toolExecution": {
        "lookup": {
          "effect": "readOnly",
          "concurrencySafe": true
        }
      }
    }
  ]
}
```

插件安装和启用分离：放入目录只是“已安装”，还需将插件 ID 加入 `plugins.enabled` 才会注册工具或启动子进程。未提供 `execution` 的工具按可能写入处理：串行、需要授权且取消结果保守判为未知。只有明确声明 `readOnly` 与 `concurrencySafe` 的工具才能进入最多四路的只读批次。可选 `outputSchema` 会在结果进入模型上下文前校验，`maxResultChars` 超出后完整结果保存在运行附件中，模型只接收有界预览。MCP 使用 newline-delimited JSON-RPC stdio，完成 `initialize`、`notifications/initialized`、分页 `tools/list` 与按请求 ID 分发的 `tools/call`。

## BGI 契约

Agent 内置工具只访问以下稳定端点：

- `GET /bridge/v1/info`
- `GET /bridge/v1/state`
- `GET /bridge/v1/catalog`
- `GET /bridge/v1/catalog/{methodId}`
- `POST /bridge/v1/invoke`
- `GET /bridge/v1/jobs/{jobId}`
- `POST /bridge/v1/jobs/{jobId}/cancel`

调用使用随机幂等键。`invoke` 返回 Job 不会被 Agent 当成业务成功；模型提示中也明确要求继续读取 Job 与验证状态。

## 已知边界

- 本仓库没有 BGI 本体，因此无法进行实机键鼠、截图或路线验收。
- 用户配置、模型密钥与数据只存放在安装目录的 `user/` 下，不写入源码目录或工作目录；
  界面里填写的明文密钥也会落在该文件中，长期使用建议改用 `${ENV:...}` 引用。
- 插件自述的 `readOnly` 被当作可信输入：它会跳过授权询问，自动放行的调用记录为
  `plugin.readOnly` 事件。不要启用来源不明的插件。
- 模型请求、MCP 请求与 BGI Job 都接入取消通道；同步 HTTP 插件仍使用兼容适配器，进程内阻塞调用无法确认停止时会保留未知结果。
- MCP 支持并发请求分发、分页、超时、取消通知和进程退出处理，但不向第三方 Server 提供采样、任意文件访问或客户端资源能力。
- 五种模型协议均统一为可取消流式输入；只有完整结束标记及完整工具 JSON 通过校验后才会执行工具。

更详细的边界与后续阶段见 [架构说明](./docs/architecture.md)。
