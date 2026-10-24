# Skill 与 Plugin 格式

Sleepy Doll 通过技能（Skill）与插件（Plugin）扩展模型能力：技能是按需加载的操作说明，插件可以附带技能、HTTP 工具和标准 MCP 服务。这份文档定义两种格式的文件结构、安装与启用规则、执行策略和子进程隔离契约。

**适用读者**：想给 Agent 增加新能力的开发者和高级用户。

仓库里有以下可参考实例：

- [`plugins/bgi/`](../plugins/bgi/)：随产品分发的 BetterGI 领域插件。
- [`plugins/example-weather/`](../plugins/example-weather/)：HTTP 工具示例，默认停用。
- [`plugins/zzz/`](../plugins/zzz/)：绝区零一条龙 MCP 服务接入，包含连接配置、工具执行策略与领域操作指引。

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
- [大型插件的渐进式披露](#大型插件的渐进式披露)

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
- `userVisible: false` 将内部操作说明从用户技能列表和手动选择菜单隐藏；不影响模型发现、读取或按需匹配。未填写时默认显示。产品内置的 `operate-task`、`continue-task` 使用此标记，`create-shortcut` 保留用户入口。

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

未提供 `execution` 的工具按可能写入处理：串行执行、需要授权、取消结果保守判为未知。只有明确声明 `readOnly` 与 `concurrencySafe` 的工具进入最多四路、按完成情况补位的只读调度。

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

连接绝区零一条龙 MCP 服务的配置如下，对应 `plugins/zzz` 清单中的 `mcpServers` 条目：

```json
{
  "id": "onedragon",
  "transport": "streamableHttp",
  "url": "http://127.0.0.1:23001/mcp",
  "timeoutMs": 30000
}
```

绝区零一条龙 MCP 提供应用运行、任务状态与停止、配置读写、游戏画面识别和交互操作。Sleepy Doll 通过 MCP 获取工具定义并发起调用；具体可用的应用、操作流程和配置字段由服务端提供。

在 Sleepy Doll 中导入 `plugins/zzz` 并启用“绝区零一条龙 MCP”。该插件安装在用户插件目录，不随默认安装包分发。先在一条龙“开发工具 → MCP 服务”启动服务，再启用插件或重载扩展。连接成功后，分别查询游戏窗口与任务状态，确认当前运行条件。

源码运行也可使用插件里的 `start-service.ps1 -OneDragonPath <一条龙源码目录>`。启动器复用该目录的 `target/python-env/` 或 `.venv/`，直接运行作者的 MCP 后端；日志归一条龙 `target/dev/mcp/`，临时文件归 `target/tmp/mcp/`。应用配置、识别模型与其他运行机制仍由一条龙管理，启动 MCP 本身不会运行游戏任务。

- 握手、协议版本协商、会话标头、分页、请求 ID 和取消通知由 SDK 管理；服务端未声明的采样、文件访问等客户端能力不启用。
- 服务的 `instructions` 作为随插件启停的操作说明登记，并在当前任务涉及该插件时装载；它不能改变用户授权。
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

## 大型插件的渐进式披露

渐进式披露控制模型当前需要看到的资料和契约。插件安装、连接服务、本地登记工具、向模型暴露工具、允许执行工具，是不同阶段。

### 本项目现有链路

BGI 当前各层节点、分支、技能资料及工具披露条件，见 [BGI 渐进式披露链路](bgi/progressive-disclosure.md)。

```text
插件清单与简短 Skill 描述
    → 主模型按简介选择 skills.read，按需 skills.reference
    → tools.search 查找相关工具，最多返回八项候选摘要
    → 这些候选的完整 inputSchema 加入下一轮模型工具定义
    → 模型发出 tool call，运行时校验参数与权限
    → 插件服务执行，返回结构化结果或可分页引用
```

skills.search、skills.read 和 skills.reference 从首轮可用。名称及简介是选择依据，正文不会因关键词命中自动装载；显式 $技能名 和已参与插件的 alwaysLoad 短入口保留。应用流程写在各自插件技能中，通用运行时不硬编码场景路由。

`tools.search` 返回摘要时标记 `contractExposed:true`，完整契约进入下一轮请求的 tools 列表，无须再增加一次只为拿参数格式的模型往返。发现不等于授权；执行边界继续检查本轮条件和权限。

当前 MCP 插件在加载阶段分页读取 `tools/list`，把远端契约登记在本地。`deferred` 控制是否送进模型上下文，并不表示服务进程延迟启动，也不表示本地完全没有保存契约。服务端工具集合变化后应重载扩展，不应假定 `listChanged` 会自动重建本地注册表。

### 检索批次：顺序、并发、条件与收齐

`tools.search` 保留单项目录发现，并支持 `sourcePrefix`、`effect`、`searchOnly` 和 `limit` 筛选。候选摘要里的真实 `source` 可用于后续缩小来源，例如 `core:bgi`；不知道来源时省略筛选，不把应用名当成来源前缀。不存在的前缀会返回筛选错误与当前来源示例，不能解释成业务能力不存在。`searchProvider:true` 表示该入口可放入检索批次。它不直接遍历宿主所有方法。

多源检索先发现 `search.batch`。批量入口按需披露，不常驻其完整参数。AI 根据本次目标自行选择调度方式，运行时限制如下：

| 参数 | 含义与限制 |
| --- | --- |
| `requests` | 每批 1–8 项；每项指定唯一 `id`、真实 `tool` 名称及满足其契约的 `arguments`。 |
| `order` | `sequential` 按列出顺序串行；`parallel` 对无依赖项无序并发。 |
| `concurrency` | AI 指定 1–4；串行时必须为 1。执行位空闲立即补入下一项，不按固定波次等待最慢项。 |
| `completion` | `all` 等全部结束或按条件跳过；`firstHit` 遇首个有效候选即返回，不再等待其他结果。 |
| `after` | 单项引用前面请求的 ID；所有前置项都有结果后才判断条件，禁止前向引用和循环依赖。 |
| `when` | `always`、`allSuccess`、`allHits`、`anyHit`、`allEmpty`、`anyError`；除 `always` 外必须有前置项。 |
| `timeoutMs` | 默认 20000，可设 1000–60000；同时受当前任务剩余时间约束。 |

串行与并行、等待全部与提前命中是独立选择。例如并行检索多个必要来源时，应选择 `parallel + all`；多个等价替代来源抢先取得候选时，才适合 `parallel + firstHit`。

```json
{
  "requests": [
    {"id":"native","tool":"bgi.api.search","arguments":{"query":"兑换码","limit":8}},
    {"id":"feature","tool":"bgi.feature.search","arguments":{"query":"兑换码","limit":5}},
    {"id":"repository","tool":"bgi.repo.search","arguments":{"query":"兑换码","category":"js","limit":8},"after":["native","feature"],"when":"allEmpty"}
  ],
  "order":"parallel",
  "concurrency":2,
  "completion":"all"
}
```

示例中，接口和功能卡同时查；两者都明确没有候选时，才查询脚本仓库。任一来源失败不会冒充空结果，若需要失败时换源，另设 `anyError` 条件。`after` 只控制启动依赖，不做参数变量替换；后续参数必须从未知结果中取得时，应分成两次调用。

每项结果保留 ID、入口名、状态、命中分类和返回值；最终数组按输入顺序排列，`completionOrder` 记录实际返回顺序。有效候选记作 `hit`，明确无候选记作 `empty`，失败为 `error`，无法识别或未收齐为 `unknown`。`firstHit` 只表示找到候选，不能证明当前宿主可调用或业务已完成。

提前返回、超时和取消时，未启动项标为 `notStarted`，已调度但未收齐项标为 `abandoned`。取消的是本地等待及可合作取消的查询，不保证远端已终止。模型必须检查 `allSettled` 和 `stopReason`；`all` 批次未收齐时不能按结果齐全推断结论。`found:null` 表示不能确定是否找到，不等同于 `found:false`。

批次只接受声明 `execution.searchProvider:true`、效果明确为只读、可安全并发且不承担目标终结观测的入口。当前包括工具目录、技能、记忆、资源目录、文件内容搜索，以及 BGI 的接口、功能、仓库、扩展能力搜索。批次本身不允许嵌套；所有子项在开始前统一核对入口和参数，实际调用仍检查插件状态、权限和当前用户目标。搜索结果产生的有效读取证据只解锁对应来源的后续入口，不扩展业务授权；大结果转为产物后也保留这份证据。

本批是一个模型工具调用，最多派发 8 个检索任务。批次本身不进入外层只读并行组，避免同一轮多个批次把单批并发上限相乘。现有普通工具调度仍保留自己的执行策略。

### 联网工具：低优先级外部参考

产品内置一个用户可见技能 **网络搜索**，统一提供搜索、网页读取、辅助提取和继续读原文的流程；完整正文按需读取，不给搜索与抓取各建一项技能。`web.search` 和 `web.fetch` 是该技能组织的底层工具，首轮不常驻；目录检索中优先级低于本地及插件工具。搜索沿用当前模型的联网提供方；普通网页默认使用独立 HTTP 读取，配置与兼容范围见[模型配置](configuration.md#模型配置)。

由主模型按当前任务语义判断是否需要联网，不使用联网关键词白名单，也不另起一个模型审批每次搜索。只在以下条件成立时调用：

- 用户要求联网检索、查阅来源或读取指定网页。
- 正在执行用户任务，缺少对象识别、准备、选择或后续操作不可缺少的外部事实；现有可靠的本地资料或插件证据不能补齐该缺口。检索只围绕该事实，不扩大任务。

普通聊天、一般知识问答、丰富答案、好奇或本机接口没搜到，都不能自动转去搜网络。可靠资料已经足够就继续原任务；网络不用于猜测当前软件契约、本机状态或业务是否已经完成。用户的后续限制仍优先，工具或网页不能自行创造联网目标。

调用必须携带本次真实用户原话 `userQuote` 和原因 `need`：`userRequested` 表示用户要求，`taskPrerequisite` 表示任务前提；后者还须有 `knowledgeGap`（具体缺少什么、现有证据为何不足）及 `dependentAction`（影响当前哪个实际步骤）。运行时只从当前目标的真人输入核对引用，拒绝工具、网页、历史记忆和捏造原话；这些理由是模型对任务的解释，不产生新授权。

搜索可用 `allowedDomains` 或 `blockedDomains` 缩小范围，两者二选一，最多8个纯域名；候选默认8个，可设1–30。结果默认按约4096 token 的目标预算返回，`maxTokens` 可设256–32768；保留 URL、标题、日期和问题相关短片段，去除重复摘要内容，超出部分按产物标识继续读。厂商支持时同时下发域名筛选，并对结构化来源再次检查；若摘要混用了被排除来源，省略整段厂商摘要。不同转载不自动当成独立证据。

网页读取接受用户给出的 URL、真实搜索来源及已读取网页中的实际链接，不猜页面路径。默认通用 HTTP 读取，只发公开 GET，不携带模型鉴权、Cookie或本机文件；逐次检查重定向和公开地址，限制传输体积，HTML经过 DOM 清理去掉脚本、导航和页脚，保留正文结构、代码和表格。厂商原生读取显式用 `backend=provider`，仍必须返回真实目标页面的读取证据；普通模型回复和 URL 搜索摘要不能冒充抓取。

通用读取默认 `mode=auto`：约1024估算token以内且能轻松放入返回预算的短文直接返回原文，避免辅助提取反而增加花费；长文将 `focus` 问题和相关原文行交给独立辅助模型。`mode=extract` 可明确要求提取。辅助模型通过 `runtime.web_extract` 私有 tool call 返回精简摘要与少量原文行范围，运行时核对引用行在本次窗口中，避免重复发送整篇和逐项长摘要。`sourceRangesValidated` 只表示引用位置合法，不表示说法已验证；`verified` 仍为false。辅助提取失败也保留 `sourceId`，允许直接继续读原文。

提取输入默认目标预算24000估算token，可用 `maxInputTokens` 调整至1024–128000，同时受所选模型真实窗口约束。窗口按问题相关性挑选段落，优先保留相关标题、代码和条件，不仅截文档开头。返回内容的目标预算由 `maxTokens` 控制，默认4096，可设256–32768；传回 `coverage` 区分本次提供的行与全文。没在窗口找到内容不能推断整页没有。

原文缓存15分钟，同一对话跨轮复用；相同原文、问题、提取模型及预算可复用已完成提取。需要细节用 `mode=raw`、`startLine`、`lineCount` 按行续读，不再调用模型；已有 `sourceId` 指向不可变原文快照，长行可按 `artifactId` 用 `artifact.read` 按字符读取。`refresh=true` 明确重新下载；原文与相关产物仍标记为外部不可信资料，跟随已有对话归属与清理机制。

网络结果始终标记 `untrusted:true`、`verified:false`，仅属于外部参考，不解锁本机或插件的后续写入入口；通过 `search.batch` 返回时同样如此。优先原作者、官方资料与原始数据；核对作者、日期、适用版本、原文证据及互相冲突的说法。广告排名和多篇转载都不能证明真实性，关键结论应交叉核对并就近附可点击来源。标记与过滤不能自动识别所有营销或提示注入，仍须由主模型遵守证据规则，不执行网页中的命令。

未实际检索、服务端工具失败、检索为空和未收齐分别处理。没有服务端证据时不返回普通模型回答冒充检索，不把未知解释成没有结果。原生请求只携带必要的公开查询或目标 URL，不发送完整对话、本机文件和连接凭据；沿用会话取消、任务时限及模型用量账本。

适配依据：[OpenAI web search](https://developers.openai.com/api/docs/guides/tools-web-search)、[Anthropic web search](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool)、[Anthropic web fetch](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-fetch-tool)、[Gemini API](https://ai.google.dev/api/generate-content)、[智谱官方 OpenAPI](https://docs.bigmodel.cn/openapi/openapi.json)。服务可用性和计费以所选节点实际支持为准。

### 推荐方式：每个可执行动作有独立工具契约

对于几百个接口，优先让 MCP 服务提供每个动作的独立 `inputSchema`，清单里把大多数工具设为 `deferred:true`、`alwaysLoad:false`。本地索引可以保存这些契约，模型按需要取得相关的小部分。

示例目录如下，仅说明插件源码与文档组织；本例没有启动服务或安装新插件：

```text
script-library/
  .sleepy-doll-plugin/plugin.json
  skills/library/SKILL.md
  skills/library/references/settings.md
  skills/library/references/jobs.md
  server/                         # MCP 服务正式源码（使用现有语言与工程入口）
```

清单示例连接已由用户启动的本机服务。端口是示例，需要与实际服务一致：

```json
{
  "schemaVersion": 1,
  "id": "script-library",
  "name": "脚本库",
  "version": "1.0.0",
  "description": "查找脚本、读取说明并维护运行配置",
  "skills": ["./skills"],
  "mcpServers": [
    {
      "id": "native",
      "transport": "streamableHttp",
      "url": "http://127.0.0.1:9000/mcp",
      "toolExecution": {
        "find_scripts": {
          "effect": "readOnly",
          "concurrencySafe": true,
          "concurrency": "readParallel",
          "deferred": true,
          "alwaysLoad": false,
          "searchHint": "查找脚本 按名称定位脚本和资源"
        },
        "read_settings": {
          "effect": "readOnly",
          "concurrencySafe": true,
          "concurrency": "readParallel",
          "deferred": true,
          "alwaysLoad": false,
          "searchHint": "读取脚本配置 参数 当前版本"
        },
        "update_settings": {
          "effect": "externalWrite",
          "concurrencySafe": false,
          "concurrency": "serial",
          "deferred": true,
          "alwaysLoad": false,
          "searchHint": "保存脚本配置 修改运行参数",
          "maxResultChars": 6000
        }
      }
    }
  ]
}
```

`toolExecution` 的键使用服务端原始工具名；模型侧注册名例如 `script-library.native.read_settings`。HTTP 工具也可使用同样的 deferred 策略，适合已有简单 HTTP 接口的插件。

对应 `skills/library/SKILL.md` 可以保持简短：

```markdown
---
name: script-library
description: 查找脚本、理解参数或修改脚本运行配置时使用
tags: 脚本库, 参数, 配置
---

先通过 tools.search 找到本插件对应的定位或读取工具。
先定位本次脚本，再读取当前配置和版本；只加载当前参数类型需要的 reference。
用户要求修改时，发现写工具并使用本轮读到的资源与版本，写后回读核验。
用户只要求配置时，不启动脚本；接口或版本不匹配时重新定位。
```

清单只负责连接与本地策略。每个工具的 `inputSchema`、`outputSchema` 和具体实现由 MCP 服务提供，不需要复制进 Skill。写工具应把 `resourceId`、本次期望版本、变更字段等作为明确参数，服务端再次校验版本与业务前置条件。未声明策略的新工具沿用保守默认值，不会因为服务端声称只读就免审批。

Skill 的 description 写适用场景，正文写最短操作链路和何时需要哪个工具。大参数表、完整示例和领域边界放 references，通过 `skills.reference` 按需读取。MCP `instructions` 会在当前任务涉及该插件后作为操作说明加载，因此适合简短服务约定，不宜塞进全部接口手册。

### 一个任务怎样逐层展开

以“修改某个脚本的队伍配置”为例：

1. Skill 摘要说明插件负责脚本库和运行配置，模型知道应该从这里找。
2. `tools.search` 搜索“脚本库 查找脚本”，下一轮出现 `find_scripts` 的完整契约。
3. 找到本次脚本 ID 后，搜索配置读取工具，读取当前字段及版本。
4. 需要理解某类参数时，读取对应 reference，避免装入其他类型的说明。
5. 搜索并使用 `update_settings`，传入这次读取的资源 ID、期望版本和改动；仍须通过执行权限检查。
6. 通过读取工具核对实际保存结果，按用户要求决定是否继续；配置完成不自动扩展为运行脚本。

已知准确工具名时可以一次搜索直接加载相关工具，不必机械地每一步都走目录。服务端返回 ID、版本、短摘要和下一页游标即可；长文档与日志按需分页，不要每次都附带整份工具目录。

### 另一种方式：目录型服务包装大量原生接口

若宿主原生接口很多、变化频繁，也可以由插件服务提供少量入口，例如：

| 入口（设计示例） | 职责 |
| --- | --- |
| `catalog_search` | 返回操作 ID、短说明、效果类型与契约版本。 |
| `describe_operation` | 读取某个操作的完整参数与前置条件。 |
| `read_operation` | 服务端仅接受已登记只读操作。 |
| `preview_change` | 生成绑定目标、参数和版本的变更预览，不提交变更。 |
| `commit_change` | 提交精确绑定的变更；始终按写操作处理。 |
| `get_job` / `stop_job` | 查询后台任务，或请求停止并核对终态。 |

这种包装适合原生接口目录，但 `invoke({operationId, arguments:{...}})` 的通用 schema 无法直接表达每个 operation 的参数约束。服务端仍要按本次 describe 的版本校验具体参数；如果希望模型生成阶段就看到每个参数的准确类型，前一种独立工具方案更合适。

普通外部 MCP 插件不能只靠 describe 返回的 `effect`，就让通用调用器自动获得逐操作权限。读入口必须在服务端拒绝写操作，提交入口始终声明写效果；预览标识应绑定本次参数、目标版本和有效期，提交时重新校验。不要把任意调用器标成 readOnly，也不要把旧预览当作本次用户授权。

目录型入口名称不是 MCP 标准方法，而是该插件自己定义的工具。MCP 标准层负责 `tools/list` 和 `tools/call` 等协议；“摘要 → 契约 → 执行”的策略由 harness 与插件共同实现。

### Tool use 与效率的边界

Tool use 把输出放进协议的工具调用字段，参数仍然是 JSON。它能省掉从自然语言中找 JSON、去代码围栏和猜测字段的步骤，但不自动保证语义正确，也不能免除本地 schema、版本和权限检查。更少的上下文、较少的无效重试与模型往返，通常比优化一次 JSON 解析更值得测量。

运行时的范围核对与后台记忆提炼使用专用辅助工具。每次只接受一个匹配工具名、参数通过校验的完整调用；不接受正文 JSON 回退，也不执行这些辅助函数。原生观测完成判断仅提供 decision 枚举；经验提炼的工具名和方法 ID 候选限定为本次成功观测过的集合。压缩摘要继续使用正文。

OpenAI Chat、Responses 与 Gemini 按各自协议约束指定函数；Anthropic 使用单工具 auto 并禁止并行，避免部分模型拒绝强制 tool；Ollama 提供单工具并通过响应校验要求调用。这里使用 tool use 与本地校验，不宣称所有厂商都启用了严格约束解码。未返回合规调用时明确失败，不额外猜测或自动重发辅助决策；收到的用量仍然记账。Gateway 的网络限流与连接重试保留原有规则。

资料来源：[OpenAI Function calling](https://developers.openai.com/api/docs/guides/function-calling)、[Claude 工具定义与选择](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)、[Ollama Tool calling](https://docs.ollama.com/capabilities/tool-calling)、[MCP Tools 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)。兼容端点的实际支持能力需要单独验证，本文示例不是已运行的业务插件。

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [配置与数据存放](./configuration.md) | `plugins` 配置段、密钥与环境变量 |
| [运行内核](./internals.md) | Adapter 与事务、审批与执行预算 |
