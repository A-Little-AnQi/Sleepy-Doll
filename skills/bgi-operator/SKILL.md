---
name: bgi-operator
description: BetterGI 用户资源、宿主设置与执行命令的统一操作手册。用于查询、修改、创建和运行 BGI 内容。
tags: BetterGI, 原神, 配置组, 调度器, 路线, 脚本, 设置, 查询, 修改, 创建, 运行
alwaysLoad: true
requiresProviders: bgi
---

# BGI 操作规范

## 先判断对象，再选工具

每个请求只能先走一条证据路径。不要同时搜索接口、插件和用户目录。

| 用户目标 | 权威来源 | 首选工具 |
|---|---|---|
| “我有哪些”、梳理、查某个已安装内容 | `User\` 中的真实文件 | `bgi.user.list/read` |
| 查询或修改 BetterGI 全局设置 | 宿主当前 `AllConfig` | `bgi.api.search/describe/read/invoke` |
| 运行任务、切换界面、调用宿主动作 | 当前接口目录与运行状态 | `bgi.api.*`，提交前才用 `bgi.state.get` |
| 插件声明的语义能力或资源 | 已安装插件目录 | `bgi.capability.*` / `resource.search` |
| 理解某个 JS 脚本的参数 | 该脚本自己的清单和说明 | `manifest.json`、`README.md`、`settings.json` |
| 更新脚本仓库或已订阅脚本 | User 订阅清单 + 宿主仓库更新接口 | `bgi.user.list/read` + `bgi.update_subscribed_scripts` |

`bgi.api.search` 列的是“BetterGI 能做什么”；`bgi.user.list` 列的是“这个用户实际装了什么、配了什么”。两者不能互相替代。没有安装插件时，不调用 `bgi.capability.search` 作为兜底。

## BetterGI 对象模型

| 对象 | 用户目录中的位置 | 关键事实 |
|---|---|---|
| 调度器配置组 | `ScriptGroup\<组名>.json` | `projects[]` 才是实际任务清单 |
| JS 脚本 | `JsScript\<目录>\` | 目录名由任务的 `folderName` 引用 |
| 地图追踪路线 | `AutoPathing\...` | 层级和名称由已安装数据决定 |
| 键鼠脚本 | `KeyMouseScript\` | 录制与回放文件 |
| 一条龙 | `OneDragon\` | 与调度器配置组不是同一种对象 |
| 战斗/卡牌/音乐等资源 | 对应 `AutoFight\`、`AutoGeniusInvokation\`、`Music\` | 以磁盘实际内容为准 |
| 全局设置 | `config.json` | 运行中以内存 `AllConfig` 为准，修改走设置事务 |

配置组包含 `name`、`index`、`config`、`projects[]`。任务常用字段包括 `name`、`folderName`、`index`、`type`、`schedule`、`status`、`runNum`、`jsScriptSettingsObject`。字段形状以用户现有同类文件和脚本自己的参数定义为准。

`schedule` 是 BetterGI 的调度周期。脚本内部还可能按星期、账号状态或运行记录自行跳过，因此组名和 `schedule` 都不能单独证明任务何时实际执行。

## 查询现有配置

1. 用一次 `bgi.user.list` 找到目标目录。只要名称时不传 `jsonKeys`；需要梳理 JSON 内容时，在同一次调用传 `jsonKeys`。配置组通常投影 `name,index,projects`，只有分析运行参数时才读取 `config`。
2. 仅当某个文件没有返回 `data`、内容被截断或需要完整写回时，再对该文件调用 `bgi.user.read`。多个独立文件在同一轮并行读取。
3. 汇总磁盘事实，不再调用 `bgi.api.search`、`plugins.list` 或重复列目录。
4. 区分启用状态、调度周期、任务类型和脚本内部参数；不要把文件名或组名当作结论。

## 理解和修改脚本任务

1. 从配置组任务的 `folderName` 定位脚本目录，不按显示名称猜目录。
2. 调用一次 `bgi.user.inspect_script` 取得 manifest、README、settings 参数定义、目录条目和账户配置；不要再对这些文件逐个 list/read。
3. `jsScriptSettingsObject` 的键必须来自 `settings.json.name`；值满足对应类型、选项和默认值。
4. 需要账户文件或脚本子资源时，按 README/manifest 给出的路径读取。不要为了“了解脚本”读取 `main.js`。
5. 修改配置组时读取完整目标文件，只改目标字段，保留未知字段；写入时传该次读取返回的 `sha256`，防止覆盖期间出现的新改动。

## 创建用户资源

1. 确认目标名称在对应目录中不存在；若用户没指定名称，使用能表达目标且不冲突的名称，不为普通命名再次提问。
2. 读取一个最接近的现有对象作为结构样板。若用户明确要创建同类组，可保留样板的 `config`，只替换组身份和用户要求的 tasks；不要再读 User/config.json 或 AutoFight 重建同一份宿主配置。样板不能继承用户未要求的额外任务、账号、配队或通知权限。
3. `folderName` 已被现有配置组引用时，仍须确认对应目录或文件还在；缺失则更新/订阅，不得视为已安装。
4. JS 任务必须核对脚本参数定义；没有可靠默认值的必填参数才构成用户缺项。
5. 计算顺序字段时复用目录投影里的 `index`；不要再逐文件读取文件头。
6. 调用 `bgi.user.write` 提交完整文件。替换已有文件时传 `expectedSha256`；新建时省略。运行时按实际改动范围决定要不要确认一次：改几个字段、新建一个对象都直接执行；删除文件或大范围改配置（同一意图内累计 10 个以上配置叶字段、3 个以上对象，或整份替换）才弹一次范围确认。不在对话里重复索要许可。
7. 写后重新读取目标文件，核对名称、任务数、引用和关键参数。保留返回的 `backup`；需要撤销时调用 `bgi.user.restore`，并传当前文件的 `sha256`，不要手工覆盖。

## 修改全局设置

`User\config.json` 不用 `bgi.user.write`。运行中的 BetterGI 会用内存值覆盖磁盘，而且字段 setter 可能有联动行为。

固定流程：

1. `bgi.api.search` 在 `settings` 组用一个核心业务词定位候选。
2. 只对最可能的候选调用一次 `bgi.api.describe`。
3. 用 `bgi.api.read` 读取当前值、`valueSchema`、`writable` 和 `valueVersion`。
4. 单字段可用 `bgi.set_setting`；多字段用 `bgi.preview_settings` 后提交 `bgi.commit_settings`。
5. 运行时按实际改动范围决定要不要确认一次写操作。提交后回读；保存 `changeId` 供回退。
6. 候选不可写或语义不确定时停止修改并说明具体缺口，不靠字段名猜效果。

## 执行任务

1. 运行现有配置组或采集材料时，先调用一次 `bgi.user.resolve`。不要先 `bgi.api.search`，也不要列 AutoPathing 或逐条读取路线 JSON。
2. `verdict=run` 时直接读取 `bgi.run_script_group` 契约并传入精确 `name`。路径缺失时运行时会拒绝空跑；不要在 `repair` 状态下调用它。
3. `verdict=repair` 时只处理 `missing` 列出的路径：更新仓库或订阅后再次 `resolve`。
4. 只有准备提交执行时才调用一次 `bgi.state.get`，检查截图器、游戏句柄、任务锁和窗口状态。纯查询或文件编辑不需要状态快照。
5. 其他动作若已知道精确 `methodId`，直接 `bgi.api.describe`；否则只在 `command` 组按一个动作词搜索一次。
6. 契约必须同时满足：`callable=true`、参数可提供、接口确实作用于目标对象。其他低层命令仍依赖界面当前选择时，不得声称能按名称执行。
7. 调用 `bgi.api.invoke` 后用返回的 Job ID 查询到终态。完成只证明处理器返回；按契约要求复查状态或结果。
8. 接口不可调用时直接说明唯一阻塞项。不要继续换中英文、查插件、搜生命周期接口或让用户重复提供已经查到的信息。

## 更新脚本仓库与订阅内容

1. `bgi.update_subscribed_scripts` 是固定的稳定接口，直接 `bgi.api.describe`，不先调用 `bgi.api.search`。用户只要求刷新中央脚本仓库时使用 `repositoryOnly`；不要用“打开脚本仓库”代替更新。
2. 用户要求更新某个已安装脚本或路线时，读取 `User/Subscriptions` 中当前订阅文件，把目标解析为其中真实的订阅路径。名称对应不唯一时才询问。
   “更新/升级/同步某脚本”默认指订阅内容；除非用户明确说配置组或一条龙流程，否则不并行查询 `OneDragon`、`ScriptGroup`。
3. 调用 `bgi.update_subscribed_scripts`：传 `paths` 时只更新这些已订阅路径；省略时更新全部订阅。接口会先同步当前渠道的中央仓库。
4. 仓库/脚本更新不依赖截图器、游戏句柄或前台窗口，不调用 `bgi.state.get`，也不读取自动更新周期、上次更新时间等设置来代替执行。
5. 更新会覆盖订阅资源的程序文件，但沿用 BetterGI 自带的脚本配置保留逻辑。Job 完成后回读目标脚本的 manifest 或关键文件；不要只凭“处理器返回”声称版本已更新。
6. `bgi.api.invoke` 已跟踪 Job 到终态。返回中已经有 completed、failed 或 cancelled 时直接处理 evidence，不再调用 `bgi.job.get`；只有恢复中断任务且手里只有 Job ID 时才查询。

## 何时询问用户

先把本地文件、接口契约和状态中能取得的事实查完。只在以下情况询问，并一次问完：

- 两个真实存在的目标都符合请求，选择会导致不同结果；
- 缺少脚本定义为必填且没有默认值的参数；
- 用户要求的目标不存在，且不能从已安装资源确定替代项；
- 操作结果不可逆，而用户原话没有确定目标或范围。

不要询问 BetterGI 是否运行、目录在哪里、有哪些配置组、接口是否可用、是否需要保存、是否允许执行已明确要求的写操作。程序会获取这些事实；普通写入直接执行，只有删除和大范围配置变更会让用户确认一次真实范围。

用户要求运行游戏任务而宿主或游戏还没启动时，**自己把它启动起来**：用 `bgi.api.invoke` 调用 `bgi.start_game`（没有这个接口时，用 `bgi.api.search` 在 `command` 组找启动触发器的命令 `cmd.home_page.start_trigger`），再用 `bgi.get_status` 等到 `ready=true`，然后继续原任务。运行类调用返回「游戏尚未就绪」不是终点，那是让你先启动再重试。不要把「请你先启动游戏」当成结论交回用户 —— `bgi.start_game` 失败时会带上缺的那一项（未配置安装路径、未开启联动启动、没有游戏窗口），照它说的做或如实转述。

## 调用纪律

- 独立只读调用放在同一轮并行执行；不要逐个等待后再决定读取下一个同类文件。
- 已经得到的路径、接口 ID、配置值和状态在未发生变化时直接复用。
- 已知对象类型时直接进入对应目录，不先列 User 根目录做“环境调查”。
- 每个证据源最多做一次发现；零结果后检查是否选错证据源，而不是连续换同义词。
- 参数以 Schema、现有文件和脚本定义为准，不凭经验补字段。
- 只报告已读取或已验证的事实。未提交、Job 未完成、结果未知必须明确区分。
- 完成后先说结果，再说必要的生效条件或一个真实阻塞项；不列“你可以 A/B/C”把下一步重新交给用户。
