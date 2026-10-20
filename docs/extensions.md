# Skill 与 Plugin 格式

Sleepy Doll 通过技能（Skill）与插件（Plugin）扩展模型能力：技能是按需加载的操作说明，插件可以附带技能、HTTP 工具和标准 MCP 服务。这份文档定义两种格式的文件结构、安装与启用规则、执行策略和子进程隔离契约。

**适用读者**：想给 Agent 增加新能力的开发者和高级用户。

仓库里有两个可以直接照抄的实例：

- [`plugins/bgi/`](../plugins/bgi/)：随产品分发的 BetterGI 领域插件。
- [`plugins/example-weather/`](../plugins/example-weather/)：HTTP 工具示例，默认停用。

## 快速要点

- Skill = 一个目录 + `SKILL.md`（frontmatter 元数据 + 给模型的操作说明正文），上限 128 KiB，默认最多自动装 4 个。
- Plugin = 目录 + `.sleepy-doll-plugin/plugin.json` 清单，可含 `skills`、`httpTools`、`mcpServers`。
- **安装与启用是两件事**：放入目录只是安装，ID 进入 `plugins.enabled` 才会注册工具。
- 未声明执行策略的工具按最保守方式处理；声明 `readOnly` 的工具免授权，因此不要启用来源不明的插件。

## 目录

- [Skill](#skill)
- [Plugin 清单](#plugin-清单)
- [安装与启用](#安装与启用)
- [执行策略](#执行策略)
- [MCP](#mcp)
- [子进程隔离](#子进程隔离)

## Skill

一个目录里放 `SKILL.md`，frontmatter 描述元数据，正文是给模型的操作说明：

```markdown
---
name: my-skill
description: 何时应该使用这个 Skill
tags: bgi, route
---

# 完整操作说明
...
```

- 文件上限 128 KiB；默认最多自动装入 4 个。
- 目录中存在不代表全部内容进入每次模型上下文——匹配到的正文才快照进本次运行。
- 行尾符（LF 或 CRLF）不影响解析。

技能来源有两类：

| 来源 | 位置 | 行为 |
| --- | --- | --- |
| 用户技能 | `agent.skillDirectories` 配置的目录，含配置文件旁的 `skills/` | 界面上逐个开关 |
| 插件技能 | 插件清单 `skills` 字段引入 | 随插件装载，插件停用后一并消失，界面上不单独列出 |

产品自带的领域说明挂在 `plugins/bgi` 下，属于插件技能。

## Plugin 清单

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

| 字段 | 说明 |
| --- | --- |
| `schemaVersion` | 清单格式版本，当前为 `1` |
| `id` | 插件标识。`bgi` 是宿主提供方的保留 ID |
| `skills` | 插件附带的技能目录 |
| `httpTools` | HTTP 工具定义，`inputSchema` / `outputSchema` 为 JSON Schema |
| `mcpServers` | MCP 连接定义与逐工具执行策略 |

## 安装与启用

放入目录只是「已安装」，把插件 ID 加入 `plugins.enabled` 才会注册工具或启动子进程。

- 更新插件前必须先停用；移除和替换把旧文件留在插件目录的 `.retired/` 下。
- 配置目录旁的 `plugins/`（`plugins.directories` 引入）是用户自己装的插件；产品自带的插件放在安装目录的 `plugins/`，由配置里的 `../plugins` 引入。
- 产品插件默认开启，关闭它要把 ID 写进 `plugins.disabled`：仅列进 `plugins.enabled` 不会重新打开已被显式停用的插件。这类插件无法移除，界面上没有对应入口。
- `bgi` 的工具和领域说明都由插件清单装载；其 MCP 服务实现随程序分发。

## 执行策略

未提供 `execution` 的工具按可能写入处理：串行执行、需要授权、取消结果保守判为未知。只有明确声明 `readOnly` 与 `concurrencySafe` 的工具进入最多四路的只读批次。

> **安全边界**：`readOnly` 声明按可信输入处理——声明只读的工具不经过授权询问，自动放行的调用记录为 `plugin.readOnly` 事件。启用来源不明的插件，等于把这条免授权路径交给它。

- 可选的 `outputSchema` 在结果进入模型上下文前校验。
- `maxResultChars` 超出后完整结果保存在运行附件中，模型只接收有界预览。

## MCP

客户端与服务端使用官方 Rust SDK `rmcp`。支持标准 stdio 和 Streamable HTTP；应用内服务通过双向字节管道运行同一套 MCP JSON-RPC 编解码、握手和工具调用，不直接调用服务端方法。

| `transport` | 必填字段 | 说明 |
| --- | --- | --- |
| `stdio`（省略时默认） | `command` | 可带 `args`；兼容既有清单，子进程退出时连接关闭 |
| `streamableHttp` | `url` | 可带 `headers`；支持 JSON/SSE 响应、有会话和无会话服务 |
| `inProcess` | `server` | 仅可引用应用注入、与插件 ID 相同的服务；第三方清单不能指定任意实现 |

各传输都可指定 `timeoutMs`（默认 30000）与按远端原始工具名索引的 `toolExecution`。混用传输字段会拒绝加载。HTTP 标头支持 `${ENV:NAME}`，环境值不会写回清单。

例如连接绝区零一条龙已经启动的服务：

```json
{
  "id": "zzz",
  "transport": "streamableHttp",
  "url": "http://127.0.0.1:23001/mcp",
  "timeoutMs": 30000
}
```

- 握手、协议版本协商、会话标头、分页、请求 ID 和取消通知由 SDK 管理；服务端未声明的采样、文件访问等客户端能力不启用。
- 服务的 `instructions` 作为随插件启停的常驻操作说明装载；它不能改变用户授权。
- 远端工具注册名为 `<插件>.<服务>.<原始工具名>`；优先读取标准 `title`，其次读取 `annotations.title`。
- 返回 `structuredContent` 时按 `outputSchema` 校验后作为工具值返回；纯内容或带图片、音频、资源的混合结果保留整个标准结果，避免丢失 `content`。`isError` 保留服务端错误说明。
- 远端 `annotations` 和 `_meta` 是提示，不授予只读、并行或免审批权限；这些策略由本地 `toolExecution` 明确声明。
- 取消或超时会发送标准取消通知，并将未确认的执行结果报告为未知。后台游戏任务仍需领域停止工具和终态核验。

### BGI 服务

`plugins/bgi` 清单引用应用内 `bgi` 服务。模型工具保留 `bgi.*` 原名及原执行策略，按标准 MCP 结构化结果往返；运行内核的原生请求和 Job 事件也通过该 MCP 连接到 BGI 提供方，再访问宿主桥。宿主来源检查、授权、实例和目录版本、幂等提交、就绪闸门及业务核验仍保留。

`io.sleepy-doll/execution`、`io.sleepy-doll/error`、`io.sleepy-doll/internal` 是 MCP `_meta` 中的产品扩展。只有应用内服务可提供可信的执行策略与内部连接；内部连接不进入模型工具注册表。标准客户端可忽略这些扩展。

其他 MCP 客户端也可通过 stdio 使用 BGI 服务：

```json
{
  "command": "<安装目录>/sleepy-doll.exe",
  "args": ["--mcp-bgi", "--config", "<已有用户目录>/config.json"]
}
```

该模式只提供 MCP，不创建窗口、Agent 数据库或模型会话；要求已有配置启用 BGI 插件与连接，不会自动启动 BetterGI。标准输出仅承载 MCP，错误写入标准错误。开发入口 `sleepy-doll-dev` 支持同样参数。此服务使用配置中的宿主连接权限；外部客户端负责自己的用户授权与结果核验。

## 子进程隔离

插件拉起的子进程（MCP Server 与 Adapter）共用同一套隔离：

- 清空继承的环境变量（只保留 `PATH`、`SystemRoot`、`WINDIR`、`TEMP`、`TMP`，见[配置与数据存放](./configuration.md)的密钥一节）。
- 套上带 `KILL_ON_JOB_CLOSE` 与 512 MiB 单进程内存上限的 Windows Job Object。

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [配置与数据存放](./configuration.md) | `plugins` 配置段、密钥与环境变量 |
| [运行内核](./internals.md) | Adapter 与事务、审批与执行预算 |
