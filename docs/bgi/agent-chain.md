# Sleepy Doll：从用户输入到最终输出

源码事实源：`E:/BetterGIProject/Sleepy-Doll`。本文按当前代码路径描述；产品是通用游戏脚本工具 Agent，BGI 是其中一个提供方。图中的分支不要求每个请求全部经过。普通聊天可以直接回答，资源运行也不需要先加载整份功能目录。

## 总链路

```mermaid
flowchart TD
  U[用户输入：目标、补充信息、模型选择] --> W[React 输入区与 api.submitTask]
  W --> IPC[WebView IPC：task.submit / run.submit]
  IPC --> APP[AppController：校验与创建 Run]
  APP --> J[SQLite Journal：会话、消息、事件、幂等 clientKey]
  APP --> SUP[Supervisor：运行状态、时限、取消与恢复]
  SUP --> CTX[构造模型上下文：历史、资源证据、技能目录]
  CTX --> SK[按需技能：alwaysLoad、显式技能、自动匹配]
  SK --> DEF[工具契约：常驻入口与已发现工具]
  DEF --> BUD[上下文预算、压缩与缓存前缀]
  BUD --> GW[模型 Gateway：协议转换与流式解码]
  GW --> M[配置中的模型服务]
  M --> GW
  GW --> DEC{模型返回}
  DEC -->|文本增量| EV[Journal 事件：assistant.delta]
  DEC -->|工具调用| VAL[工具名、JSON 参数、已发现契约校验]
  VAL --> PROV{提供方分发}
  PROV -->|普通软件文件与命令| WS[workspace 工具：软件工作目录约束]
  PROV -->|其他插件| EXT[HTTP / MCP / Adapter / 任务能力]
  PROV -->|BGI 目标| BGI[BGI 插件与提供方]
  WS --> OBS[结构化结果与可核验证据]
  EXT --> OBS
  BGI --> OBS
  OBS --> HIST[保存 tool 结果、尝试记录、计划与状态]
  HIST -->|目标未完成且预算允许| CTX
  HIST -->|需要选择或授权| ASK[awaitingUser / awaitingApproval]
  ASK -->|用户补充或授权| SUP
  DEC -->|本轮最终答复| END[保存最终消息与 Run 终态]
  END --> EV
  EV --> READ[事件流或 events.read：sequence 游标]
  READ --> SES[前端 Session：状态、消息快照、流式边界]
  SES --> TRANS[Transcript：稳定 turn / part 标识]
  TRANS --> UI[最终输出流式显示；过程收到折叠内]
  U -->|停止| CANCEL[取消 Run 与 CancellationToken]
  CANCEL --> SUP
  CANCEL --> JOB[关联工具 Job 的取消与终态核对]
  JOB --> OBS
```

实际源码入口：[前端 IPC](../../web/src/ipc/api.ts)、[桌面 IPC 接收](../../src/main.rs)、[AppController](../../src/app/mod.rs)、[Supervisor](../../src/runtime/mod.rs)、[模型 Gateway](../../src/runtime/gateway.rs)、[Journal](../../src/runtime/store/journal.rs)、[前端 Session](../../web/src/session/index.ts)、[Transcript](../../web/src/components/chat/Transcript.tsx)。

## BGI 子链路与渐进式披露

```mermaid
flowchart TD
  G[用户的 BGI 目标] --> ROUTE[简短 bgi-assistant 路由]
  ROUTE -->|材料、路线、脚本运行| RES[user.resolve：本机组、引用、路线与 JS]
  RES -->|本机未命中| REPO[repo.search：当前中央仓库完整目录]
  REPO -->|候选命中| READ[repo.read：Git Blob / 仓库文件]
  READ --> PREP[核对前提与参数，必要时订阅、准备配置组]
  RES -->|已可运行| DES[api.describe：现场接口契约]
  PREP --> DES
  ROUTE -->|删除本机路线或脚本| DELPATH[user.read 的 referencedResources 或已定位 User 路径]
  DELPATH --> INSPECT[inspect_local_resource：范围、引用、订阅与版本]
  INSPECT --> DELETE[delete_local_resource：精确移动到恢复区并核验]
  DELETE --> RESULT
  ROUTE -->|脚本机制或报错| JS[inspect_script / user.read / repo.read]
  JS --> JSSK[按需 bgi-javascript：定义、分支、模块、调用顺序]
  JSSK --> RESULT[有源码依据的解释或修改]
  ROUTE -->|其他目标不明确| FS[feature.search：少量摘要与分页]
  FS --> FR[feature.read：一张功能卡]
  FR --> REF[skills.reference：当前目标的一份流程]
  REF --> DES
  DES --> CHECK[版本、callable、输入 Schema、副作用、结果判定]
  CHECK -->|只读| RD[api.read：实例与目录版本检查]
  CHECK -->|写入或执行| PERM[运行时权限与真实范围确认]
  PERM --> PRE[新鲜状态、资源版本、游戏前置与任务互斥]
  PRE --> INV[api.invoke：requestId / instanceId / catalogVersion]
  INV --> HOST[回环 HTTP、Bearer 鉴权、接口开关与参数校验]
  HOST --> DISPATCH[MethodRegistry：稳定入口或动态命令]
  DISPATCH --> NATIVE[宿主 UI 线程、原生对象与真实任务]
  NATIVE --> JOB[Job：排队、运行、取消、完成或失败]
  JOB --> VERIFY[verification 与文件、设置、状态、日志证据]
  RD --> RESULT
  VERIFY --> RESULT
  RESULT --> LOOP[返回通用 Supervisor，继续任务或生成最终答复]
```

`feature-index.json` 是离线知识索引，不等于当前宿主目录，也不授予执行权限。模型不会常驻读取全部条目。明确的运行请求直接查资源，明确的 JS 问题直接读脚本；分类没有结果不能推导为全仓没有，查询失败也不能推导为不存在。

对应实现：[BGI 提供方工具](../../src/bridge/mod.rs)、[离线功能索引](../../src/bridge/features.rs)、[资源定位](../../src/bridge/resolve.rs)、[检索路由](../../src/bridge/retrieval.rs)、[运行时桥调用](../../src/runtime/host/bridge.rs)、[托管桥宿主](../../bgi-bridge/managed/Hosting/BridgeHost.cs)。

## 反射、对象、选择与弹窗链路

```mermaid
flowchart TD
  C[源码功能卡或当前接口目录] --> ALL[扫描 BetterGI 程序集中的 ICommand 属性]
  ALL --> FILTER{源码审阅与当前有效入口}
  FILTER -->|已弃用、占位或脱离控件的生命周期| DROP[不登记旧命令，保留可见目标的真实链路]
  FILTER -->|已审查业务入口| CAT[命令说明、参数类型、选择 Schema、弹窗输入]
  CAT --> TARGET[list_command_targets：真实窗口、DataContext、对象集合]
  TARGET -->|已有实例| ID[contextId 与原生 objectId]
  TARGET -->|未创建的非泛型实例| CREATE[create_command_target：匹配唯一构造器与原服务依赖]
  CREATE --> ID
  TARGET -->|开放泛型或抽象基类| GENERIC[绑定已有具体实例，不猜泛型类型]
  GENERIC --> ID
  TARGET -->|需要新参数对象| ARG[create_command_argument：仅当前参数声明的类型]
  ARG --> ID
  ID --> LIVE[复查类型、引用存活与宿主对象图]
  LIVE --> SEL[selection：先校验全部选择，再绑定当前目标]
  SEL --> CAN[CanExecute 与当前版本前置]
  CAN --> SCOPE[本次调用的 NativeDialogScope]
  SCOPE --> DIALOG[只处理新建输入窗口；排除已有窗口]
  DIALOG --> INPUT[text / filePath / selectedValues / confirm / values]
  SCOPE --> OP[模态操作 operationId 续接]
  OP --> UIR[ui.read：真实字段、按钮、列表与窗口]
  UIR --> UIW[ui.write / options / reorder / invoke]
  UIW --> UIEND[ui.respond / operation / close：保存与终态]
  UIEND --> EVIDENCE
  INPUT --> CMD[ICommand 或 ExecuteAsync 原生处理器]
  CMD --> EVIDENCE[返回不等于业务完成：读取目标结果]
  EVIDENCE --> RELEASE[释放桥创建的上下文，用户已有实例保留]
```

窗口与未注册子对象不会因为没有 `IViewModel` 标记而从目录消失。查询目录不构造窗口；构造是独立写操作。复杂对象以真实引用绑定，不能从用户 JSON 随意重建宿主对象。弹窗输入缺项、旧引用、同名多实例、错误参数和禁用命令均返回明确错误，不能把通用调用入口当绕过方式。

反射只负责定位真实对象和调用已审查的业务入口，不创造宿主未实现的业务。旧补接实现及其 implementationInput 已删除；弃用、占位和脱离真实控件的生命周期命令不发布；当前可见编辑器与模态窗口通过原生 UI 续接，不能因为需要交互就删除功能。正常路线执行走资源定位、prepare_pathing_group 和 run_script_group；截图测试使用当前原生 start_capture_test。完整逐项保留／删除结果见 [全部接口审阅表](interface-review.md)。

对应实现：[命令发现与执行](../../bgi-bridge/managed/Catalog/CommandCatalog.cs)、[目标与原生对象绑定](../../bgi-bridge/managed/Bgi/CommandTargets.cs)、[上下文接口](../../bgi-bridge/managed/Tools/CommandTargetTools.cs)、[本次弹窗输入](../../bgi-bridge/managed/Bgi/NativeDialogScope.cs)。

## JS 编写、OCR 与真实引擎契约

```mermaid
flowchart TD
  GOAL[用户要求编写／修改 JS] --> SKILL[bgi-javascript：按需 writing.md]
  SKILL --> SEARCH[js_api.search：实际 EngineExtend 注入别名]
  SEARCH --> READ[js_api.read：真实类型、重载与继承成员]
  READ --> OCR[captureGameRegion / ImageRegion / Region / RecognitionObject]
  READ --> VISION[BvPage / BvLocator / BvImage / OpenCvSharp]
  READ --> OTHER[输入、任务、文件、通知、模块与 settings]
  OCR --> CODE[按参数、ROI、坐标、Task/Promise、Dispose 编写]
  VISION --> CODE
  OTHER --> CODE
  CODE --> SAVE[user.write：manifest / main / settings_ui]
  SAVE --> CHECK[源码／语法／非游戏检查]
  CHECK --> RUN[按授权准备、运行与读取日志／产物]
```

契约只描述真实注入、可达类型与返回对象，不创建脚本引擎、不执行 OCR 或键鼠动作。OpenCV 类型集合按具体类型分页读取，避免把全部程序集装进模型上下文。源码回退与真实类型检查明确区分。

## 输出、折叠与停止

```mermaid
sequenceDiagram
  participant Model as 模型服务
  participant Runtime as Supervisor / Gateway
  participant DB as Journal
  participant Session as 前端 Session
  participant View as Transcript
  Model-->>Runtime: 文本增量或工具参数
  Runtime->>DB: assistant.delta / tool 事件
  DB-->>Session: 事件与 sequence 游标
  Session->>View: 当前流、已完成消息与 Run 状态
  View->>View: 按稳定 turn / part 组织本轮内容
  Runtime->>DB: assistant.completed：本次响应边界
  DB-->>Session: 完成边界与消息快照
  Session->>Session: 清理本次流，下一次响应独立累计
  Session->>View: 最终文本与过程分组
  View->>View: 最终文本流式显示，过程置于折叠内
  Note over Runtime,View: 正常结束、失败、部分完成和用户取消分别表示
  View->>Runtime: 用户停止 task.cancel
  Runtime->>Runtime: CancellationToken 与关联 Job 取消
  Runtime->>DB: 取消请求与已确认的终态证据
```

前端渲染依据实际消息、工具关联、流式边界和 Run 状态，不能先把过程当最终答案渲染后再移动。`assistant.completed` 表示一次模型响应结束，后面仍可能继续工具流程；Job `completed` 也不自动表示用户的游戏目标完成。只有核验实际终态后才报告已完成或已停止。

## 所有权与边界

| 层 | 负责什么 |
|---|---|
| 通用 Agent | 会话、上下文、模型协议、技能与工具发现、权限、计划、预算、取消、结果输出 |
| BGI 插件／提供方 | BGI 目标路由、资源与仓库语义、功能索引、脚本理解、领域前置和核验规则 |
| Rust BGI 客户端 | 契约发现、实例与目录版本、资源校验、审批、状态新鲜度、请求幂等、Job 跟踪 |
| 托管桥 | 回环鉴权、Schema、UI 线程、反射、原生对象／选择／弹窗绑定、宿主执行与结果 |
| BGI 本体 | 游戏识别与任务算法、宿主资源、原生配置 setter、实际窗口、脚本环境、取消上下文 |
| React 界面 | 输入、任务状态、事件合并、流式文本、过程折叠与最终答复展示 |

调用链和源代码检查能核验工程连接，隔离宿主能核验参数／对象／弹窗适配；真实游戏识别、角色前提、实际收益与跨会话环境仍要现场验证。测试结果与当前已运行的桥版本分开记录，不能用新 DLL 已编译代替新版本已注入。
