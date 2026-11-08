# Skill 与 Plugin 格式

Sleepy Doll 通过技能（Skill）与插件（Plugin）扩展模型能力：技能是按需加载的操作说明，插件可以附带技能、HTTP 工具和标准 MCP 服务。这份文档定义两种格式的文件结构、安装与启用规则、执行策略和子进程隔离契约。

**适用读者**：想给 Agent 增加新能力的开发者和高级用户。

仓库里有以下可参考实例：

- [`plugins/bgi/`](../plugins/bgi/)：随产品分发的 BetterGI 领域插件。
- [`plugins/windows-scheduler/`](../plugins/windows-scheduler/)：随产品分发、默认关闭的定时任务插件，含独立页面、受限 MCP 接口及 AI 问答、时间解释、操作与结果判定技能。
- [`plugins/web-search/`](../plugins/web-search/)：随产品分发、默认启用的网络搜索插件，提供原生搜索和网页读取；在插件页启停，不作为独立技能或手动命令展示。
- [`plugins/example-weather/`](../plugins/example-weather/)：HTTP 工具示例，默认停用。
- [`plugins/zzz/`](../plugins/zzz/)：绝区零一条龙独立反射桥与领域技能。
- [`plugins/march7th/`](../plugins/march7th/)：三月七独立桥与领域技能。
- [`plugins/maaend/`](../plugins/maaend/)：MaaEnd 独立桥与领域技能。

## 快速要点

- Skill = 一个目录 + `SKILL.md`（frontmatter 元数据 + 给模型的操作说明正文），上限 128 KiB，默认最多自动装 4 个。
- Plugin = 目录 + `.sleepy-doll-plugin/plugin.json` 清单，可含 `skills`、`httpTools`、`mcpServers`。
- **安装与启用是两件事**：宿主和网络搜索插件默认启用，可通过 `plugins.disabled` 关闭；其他插件的 ID 进入 `plugins.enabled` 才会装载。
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
- `name` 是稳定的内部标识；可选的 `displayName` 用于中文显示。手动选择后输入 `$显示名`，也兼容 `$内部标识`，两者装载同一份技能，不改变开关、引用和保存的标识。
- 目录中存在不代表全部内容进入每次模型上下文——匹配到的正文才快照进本次运行。
- 行尾符（LF 或 CRLF）不影响解析。
- `userVisible: false` 将内部操作说明从用户技能列表和手动选择菜单隐藏；不影响模型发现、读取或按需匹配。未填写时默认显示。产品内置的 `operate-task`、`continue-task` 使用此标记，`create-shortcut`、`create-schedule` 保留创建入口。

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
- HTTP 工具的非 2xx 响应作为工具错误返回，保留状态码和有界错误正文；不把版本冲突等业务拒绝统一解释为连接中断。错误正文最多读取 16 KiB、展示 4096 个字符，不附加请求 URL 或标头。

## MCP

客户端与服务端使用官方 Rust SDK `rmcp`。支持标准 stdio 和 Streamable HTTP；应用内服务通过双向字节管道运行同一套 MCP JSON-RPC 编解码、握手和工具调用，不直接调用服务端方法。

| `transport` | 必填字段 | 说明 |
| --- | --- | --- |
| `stdio`（省略时默认） | `command` | 可带 `args`；兼容既有清单，子进程退出时连接关闭 |
| `streamableHttp` | `url` | 可带 `headers`；支持 JSON/SSE 响应、有会话和无会话服务 |
| `inProcess` | `server` | 仅可引用应用注入、与插件 ID 相同的服务；第三方清单不能指定任意实现 |

各传输都可指定 `timeoutMs`（默认 30000）与按远端原始工具名索引的 `toolExecution`。混用传输字段会拒绝加载。HTTP 标头支持 `${ENV:NAME}`，环境值不会写回清单。

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

## 独立宿主桥

三月七、MaaEnd 与绝区零分别维护在 `plugins/march7th/`、`plugins/maaend/`、`plugins/zzz/`。每个目录独立包含插件清单、`bridge/`、领域技能和连接说明，不相互导入桥实现，也不把软件标识交给一个共用桥分流。

这三套桥使用现有 `httpTools` 契约登记工具。主程序继续负责插件启停、工具发现、参数校验、授权与技能装载；通用 MCP 支持仍可接入文档分析等其他插件。绝区零不再连接一条龙的 MCP 服务。

三套插件默认停用，导入和启用不会启动第三方软件。桥统一为 C++ 原生编译产物，各自使用独立端口和令牌，不附带 Python 等脚本桥或要求额外解释器。三月七与绝区零从原生侧调用宿主已加载运行时的反射接口；MaaEnd 使用 MXU 对已有实例公开的原生接口与任务描述，不能把它描述成对 Rust 私有对象进行 CLR 注入。各自的支持范围和手动入口见插件 README。

`test/local-adapters/` 是用户指定并由 Git 忽略的本机分析资料，保存官方源码、发布包、解包内容和来源校验信息。该目录不是生产插件依赖，不进入软件安装载荷，也不用于自动安装第三方工具。

独立交付目录为 `dist/plugins/<插件 ID>/`。原生检查通过后，运行 `scripts/package-adapters.ps1 -Action Build` 按清单组装；`-Action Check` 校验目录、版本及每个文件的 SHA-256。包内包括清单、技能、原生桥与启动说明，排除第三方工具本体、源码、测试程序和运行配置。在插件导入入口选择其中一个插件目录即可，不要求本机有源码或编译器；桥的手动连接步骤见包内 README。组装不会安装或启用插件。

构建输出分别归 `target/bridge/march7th/`、`target/bridge/maaend/`、`target/bridge/zzz/`，工具临时文件归已登记的 `target/.tmp/local-adapters/`。自动验证只运行自写的原生夹具；测试构建不包含真实注入、目标进程启动或游戏输入路径，不向已下载工具附着桥，也不加载其动态库。原生桥编译通过不能代替真实宿主注入或接口验收。

原生验证入口是 `tests/plugins/run-native.ps1`：检查测试产物导入表，拒绝真实注入、进程启动、键鼠输入和网络连接入口，再用清空凭据环境的独立桌面及单进程 Job 运行夹具。只接受三套桥标准输出目录中的测试 EXE，不接受下载的软件；测试辅助程序随检查结束清理。此限制用于可信夹具验证，不是运行任意第三方程序的虚拟机。

`cargo test --no-default-features --test plugins-load` 使用产品实际插件加载器检查三套清单、工具契约、技能、独立端口与默认停用状态；不调用桥、不启动宿主。伪宿主协议测试和插件加载通过均不代表真实宿主、游戏或已安装程序已验收。

三套适配技能沿用 BGI 的披露方式：入口只说明领域、对象边界、结果含义及引用选择，任务、配置、连接资料按需读取。技能摘要、正文与引用不固化具体业务名称、接口名称、参数名或宿主对象标识；具体调用来自当前工具契约，操作对象来自宿主反射目录。HTTP 工具先登记为按需发现，不因启用插件就把全部契约常驻给模型。插件加载检查同时核对这些边界及包内引用可读。

真实模型路由另由忽略测试 `live_glm_currencywars_isolated_plugins` 检查：使用用户原句，同时加载三套正式插件，将全部插件 HTTP 地址和令牌改指隔离假宿主，分别模拟接纳、明确拒绝和断线。假宿主只改变夹具内存，不能启动真实程序；公开资料可通过产品只读联网能力核实，命令、本机文件和真实宿主操作由测试钩子拦截。测试使用正式提交、调度和终态处理链路，核对任务目录、详情、会话依据、实际派发次数和用户可见结果；运行时拒绝拦截与模型答复分别记录，不把后台错误伪装成模型原话。

显式设置模型配置路径、模型 ID、阶段名以及 `SLEEPY_DOLL_TEST_ROOT=E:\BetterGIProject\Sleepy-Doll\target\.tmp\currencywars-agent`，并将工具临时变量指向该根，再运行 `cargo test --no-default-features --lib live_glm_currencywars_isolated_plugins -- --ignored --nocapture`。报告固定为 `target/reports/currencywars-agent.json`，保留不同阶段的原始记录；结束后清理隔离根。同一测试 EXE 仍在运行时不得重建它，避免 Windows 文件占用。单一问法通过不能代表所有问法和真实宿主通过。

HTTP 写工具在调用动作之前明确拒绝时，可返回非成功状态及 `{"submissionStatus":"notSubmitted","error":{"code":"…","message":"…"}}`。运行时记录未提交、释放本次执行占用并将拒绝原因交给模型解释；不能同时返回 `accepted:true` 或 `jobId`。仅有 HTTP 409、`executed:false`、超时或断线均不构成未提交证明，仍保留结果未知与防重放约束。三月七桥仅为已核实在动作之前拒绝的错误码发出该回执。

对声明不可重试的写工具，明确未提交后，同一目标版本、步骤、工具契约和相同参数不会再次派发；执行器直接保留拒绝原因结束，不依赖模型自行记住限制。参数或用户目标变化后重新校验，既定独立步骤使用各自的步骤标识。

启动/提交回执的核验范围与业务结果分开：已确认启动或接纳提交时，桥可返回 `verification.status=succeeded`，并以 `verification.scope=launch/submission` 说明本次工具的完成范围；未确认启动不得带已成功核验。此范围不能证明游戏任务完成。运行时对明确未提交及这两种有限范围的回执复用现有交付复核，核对最终答复中的结果、操作建议和重试次数；只读查询不因此增加复核轮次。

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

用户侧只有一个“网络搜索”技能；底层提供 `web.search` 和 `web.fetch`。它们是与模型协议无关的工具，不要求用户配置联网提供方、辅助模型或 API 兼容类型。主模型按系统提示词判断是否需要外部资料：用户明确要求，或实际执行缺少必要的外部事实时使用；普通聊天、一般知识问答和为了丰富答案不自动联网。

`web.search` 只要求 `query`，可选域名筛选、数量和返回预算；`web.fetch` 只要求 `url`，可用 `focus` 指明所需事实。工具参数描述实际工作，不要求模型重复填用户原话或提交一份联网理由表。当前用户目标、取消、时限、凭据检查和公开地址限制仍由运行时处理，网页内容不会解锁业务写入能力。

搜索通过内置公开服务取得真实来源：优先标准 MCP 的 Exa 服务，连接或限流失败时尝试 DuckDuckGo HTML。MCP 握手、流式响应和取消复用仓库的官方 SDK 客户端；先读取远端参数契约，只提交当前支持的查询字段，不向主模型装载整份远端工具目录。不携带模型密钥、Cookie 或会话原文。服务未提供可核对的来源、触发验证或全部连接失败时明确报告不可用，不能生成假结果，也不能把未知当作空结果。

返回标题、链接、日期与相关片段，默认8条、最多30条，总预算默认4096估算 token，最高32768。重复内容裁剪但保留来源；完整结果按现有产物系统保存，可按标识续读。同一运行15分钟内的相同搜索复用结果，提高返回预算不会重查。

网页使用通用 HTTP 读取并缓存清理后的原文15分钟。短页直接返回，长页由当前会话模型的独立上下文按问题提取，不传完整历史或业务工具。`mode=raw` 按行续读不调用模型，`mode=extract` 明确提取；`sourceId` 读取固定快照，`refresh=true` 明确更新。辅助提取用量记为 `web.extract`，检索网络请求不计作模型调用。引用行校验只证明位置存在，不能证明作者说法正确。

搜索、网页及辅助摘要始终标记为外部不可信参考。优先原作者、官方文档、原始数据与专业资料；核对限定条件及冲突，转载不算独立证据。工具使用低优先级指有可靠直接依据就不查，并非先穷尽本机和接口才允许补充外部事实。取得事实后继续原任务，用自然简短的话回答，不逐轮复述内部流程。

服务参考：[Exa MCP](https://exa.ai/docs/get-started/exa-mcp)、[DuckDuckGo 非 JavaScript 搜索](https://duckduckgo.com/duckduckgo-help-pages/features/non-javascript)。免密服务存在额度和可用性限制，不能承诺外部服务始终可达。

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
