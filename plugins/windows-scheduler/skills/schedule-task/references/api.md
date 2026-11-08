# 定时任务接口

以下是插件当前的受限 MCP 接口。先用 `tools.search` 读取当前声明，调用参数严格按声明。返回示例字段说明的是业务结果；工具传输层另有 ok/error 包装，错误不按成功对象读取。

## 定位与时钟

| 接口 | 参数 | 结果与使用方式 |
| --- | --- | --- |
| scheduler.clock | `{}` 或 `{localDateTime:"YYYY-MM-DDTHH:mm"}` | now 为 UTC 当前时刻；localNow 为含本机当前偏移的时刻；localDate、weekday（周一 1）、timeZone=system。指定 localDateTime 时增加 resolved.at（含目标日期实际偏移）与 resolved.future；换算失败返回错误。只读，不注册、不运行。 |
| scheduler.actions.list | 可选 query、category（shortcuts/defaults，默认 shortcuts）、offset=0、limit=20（1–50） | 只返回通过当前定时准入的快捷任务，含 id/title/content/publishedRevision/binding/available；默认项返回 title/binding/available 等，不携带源任务修订。total、nextOffset 在排除不可绑定项之后计算；搜索不到不代表原快捷任务不存在或用户目标不可实现。 |
| scheduler.list | `{}` | items 为本插件管理的安排，不列其他软件的系统任务。用名字、时间和任务列表定位，不凭名字编 ID。 |
| scheduler.get | `{id}` | 最新完整安排，用于修改前取得 revision 与各项 entryId。 |

clock 使用本机 Windows 时区转换；周期时间也跟随本机时区。它不是可设置系统时钟的接口，也不接受时区、账户或路径参数。

## 完整安排 ScheduleInput

必填 name（1–80 字符）、trigger、entries（1–32 项有序数组）；拒绝其他字段。用户没有指定名称时由助手拟自然短名；所有示例中的占位 ID 必须替换成接口返回的真实 ID。

```json
{
  "name": "晚间日常",
  "trigger": {"kind":"daily","localTimes":["20:00"]},
  "entries": [
    {"binding":{"kind":"shortcut","shortcutId":"<A的真实ID>","expectedPublishedRevision":3}},
    {"binding":{"kind":"shortcut","shortcutId":"<B的真实ID>","expectedPublishedRevision":1}},
    {"binding":{"kind":"shortcut","shortcutId":"<A的真实ID>","expectedPublishedRevision":3}}
  ]
}
```

每项只允许 binding 和可选 entryId；快捷任务 binding 只允许 kind=shortcut、shortcutId、expectedPublishedRevision≥1。创建不填 entryId；更新保留原项的真实 entryId，新项不填。不能根据重复的 shortcutId 合并执行项。

内置收尾项的 binding 仅为 `{"kind":"closeGame"}` 或 `{"kind":"closeToolSoftware"}`，不能增加目标字段。它们使用前面最近一个快捷任务的真实软件归属；连续两项收尾均关联该任务。没有前置任务、归属不唯一或软件未登记对应关闭能力时拒绝保存。重新排序后重新关联，运行时再检查源任务及关闭接口契约。可重复添加；不修改源快捷任务。每项错误后仍继续后面的项。

| trigger | 完整结构 |
| --- | --- |
| 指定日期 | `{kind:"once",at:"<clock.resolved.at>"}`，必须在未来；具体年月日、时分、偏移齐全，保存不立即运行 |
| 每天 | `{kind:"daily",localTimes:["08:00","20:00"]}` |
| 按天间隔 | `{kind:"daily",intervalDays:2,startDate:"2026-11-09",localTimes:["04:30"]}`，intervalDays 为 1–365 的整数，默认 1；大于 1 时 startDate 必填，按开始日递推 N 天，跨周／跨月不断开。startDate 可为过去的有效日期，用于保留原节奏；错过的时刻不补跑。 |
| 每周 | `{kind:"weekly",weekdays:[1,3,5],localTimes:["20:00"]}`，weekdays 非空，1–7，不重复 |
| 每月 | `{kind:"monthly",daysOfMonth:[1,15],lastDayOfMonth:false,localTimes:["20:00"]}` |
| 每月月末 | `{kind:"monthly",daysOfMonth:[],lastDayOfMonth:true,localTimes:["20:00"]}` |

周期时刻 localTimes 为 1–8 个不重复的 24 小时制 HH:mm；每个时刻都运行整个列表。按天规则没有新增字段时仍为每天；intervalDays 大于1必须提供有效 startDate（YYYY-MM-DD，1601–9999年），创建、修改、停用再启用和运行计算共用该起点，不从任务完成时重新计时。Windows 使用原生 DaysInterval，不近似成星期或月份。每月日期 1–31，不存在的日期跳过；日期与月末重合只触发一次。月末也必须传 daysOfMonth 数组。错过时刻跳过、重复触发重叠跳过是后端固定策略，不能给出唤醒、补跑、按小时任意间隔等无契约承诺。

## 预览和写入

| 接口 | 完整参数 | 成功依据 |
| --- | --- | --- |
| scheduler.preview | ScheduleInput 本身 | 返回 name/trigger/entries（title/workflowId）/nextRuns；只读，不保存草稿、不立即运行。更新预览也传 ScheduleInput，后端忽略既有 entryId 做引用检查。 |
| scheduler.create | ScheduleInput 本身 | 返回完整 Schedule；registration=verified、error 为空且 enabled=true 才能说已设置并会触发。相同内容幂等，不能改名绕过。 |
| scheduler.update | `{id,expectedRevision,schedule:<完整 ScheduleInput>}` | 返回新 Schedule；只改一部分也必须提交完整结构。保留暂停状态；registered verified 不意味着 enabled=true。 |
| scheduler.set_enabled | `{id,expectedRevision,enabled:true或false}` | 返回新 Schedule；只改后续触发，不停止已排队/运行的一轮。过去的单次安排不能重新启用为未来任务。 |
| scheduler.delete | `{id,expectedRevision}` | `{deleted:true,historyKept:true}`；实际删除所属安排及其系统登记，不删除原快捷任务，历史保留。 |

Schedule 字段：id、revision、name、trigger、entries、enabled、registration、error。entries 含 entryId/title/binding。get 返回的对象不能原封不动当 ScheduleInput：要去掉 id、revision、enabled、registration、error 与条目 title，仅保留输入允许的字段。

写入由运行时按真实权限审批；不能添加 approved、account、command、executable、XML、运行目录、提权开关、任意 tool 或模板。source 任务的执行范围由后端核验，技能不能放宽它。每条安排最多 32 项、8 个时刻，最多保存 100 条安排，不拆单绕过限制。

## 立即运行、停止和历史

| 接口 | 参数 | 含义 |
| --- | --- | --- |
| scheduler.run_now | `{id,expectedRevision,requestKey:"<本次操作的UUID>"}` | 按保存顺序排队一轮；requestKey 为本次请求生成的 UUID，同一次重试复用，用户再次明确要求再跑才换新键。返回 Occurrence，queued 只代表已提交。不要为创建或验证安排调用它。 |
| scheduler.history | 可选 id、limit=20（1–100） | items 为触发轮次，未给 id 则读取全部安排的最近记录。无分页游标参数。优先使用真实安排 id 限定范围。 |
| scheduler.stop | `{occurrenceId:"<真实轮次ID>"}` | 申请停止这一轮和未开始项；不暂停后续定时、不删除任何任务。结果以返回及后续历史为准。 |

Occurrence：id、schedule（安排快照）、state、results、createdAt、error。results 含 entryId/title/state/runId/error，可附 stopState（requested/confirmed/unknown）。轮次常见状态 queued/running/cancelling/succeeded/partial/failed/timeout/unknown/cancelled；必须读实际值，不能只凭 results 非空推断完成。cancelling 只表示已请求停止；控制流取消但仍有未确认外部调用时返回 unknown，不能称为已经停止。停止使用 Occurrence.id，不是 Schedule.id 或 runId；终态再次停止可返回 cancelRequested=false。历史查询核对真实运行和调用证据，不修改历史存储。source 修订、工具契约或资源失效可能使某项失败，后面仍尝试执行；历史保留每次重复项的独立结果，后台执行不进入最近对话列表。

## 执行时解析的原生命令计划

旧入口若仅含固定 cmd. 命令、固定等待及简单参数，绑定时会按当前契约转换并只读校验，通过才列入可定时任务库。含原生对象、界面上下文、选择或弹窗参数的入口不会自动转换；先用 bgi.api.describe 读取以下当前契约并修复封装：

- bgi.validate_command_plan：通过 bgi.api.read，参数 {mode:"host"或"game",plan:{steps:[…]}}，只读校验，不调用命令、不构造窗口或兑换，返回 valid、executed=false、contractHash、requiresGameReady 和真实命令作用范围。
- bgi.run_host_command_plan：无游戏动作的固定计划；作为快捷绑定 action 的 bgi.api.invoke methodId，arguments 是 plan。
- bgi.run_game_command_plan：仅在用户确已授权游戏/兑换操作，且真实目录至少一个命令 requiresGameReady 时使用；沿用游戏就绪和执行锁。

steps 最多16项；命令步骤为 command（当前目录的完整 cmd. ID）、title（用户可读名称）、可选 argument 或 resolveArgument 二选一；等待步骤为 waitSeconds，累计最多3600秒。只使用已审查、无弹窗、非破坏性且可等待的游戏命令。resolveArgument 的 collection 是当前命令所属 ViewModel 的公开集合，filters 最多8条，支持 equals/contains/notEmpty/todayOrLater/notBeforeNow/notBeforeNowOrUnknown/notAfterNow；orderBy/descending 必须明确，排序并列仍拒绝，不猜选。fromStep 使用0开始的前置步骤索引，仅接受本次步骤创建或更新的数据。每集合最多512项，不持久化 objectId/contextId，也不接受自由模板。

数据步骤可加 observe，结构同选择器，并用 fromStep 指向本次已执行的命令；成功只核验 runtimeData 范围的“本次取回”，不证明新发布、有效期或兑换。创建/重绑时 Scheduler 调用只读校验，并在独立运行快照固定 expectedContractHash；未来命令/桥实现或作用范围变化则拒绝，原快捷任务不被改写。

仅打开并实时获取时，按本次目录发现的命令和字段建立两步：先打开数据入口，再执行刷新并配置 observe。选择器以刷新步骤为 fromStep，核对本次更新、代码非空；条目名称、集合与时间字段从本次真实目录及数据读取，不沿用版本示例。使用 mode=host 与 run_host_command_plan。获取时间只证明本次抓取，不是发布时间。

若人类明确授权实际兑换，才在刷新之后追加本次发现的兑换命令，用 resolveArgument 选择相同的本次数据，改用 mode=game 与 run_game_command_plan。不能仅因之前聊过兑换就自行追加。已知有效期过期会跳过，未知有效期仍可按明确授权尝试。日期只有年月日时截止比较按当天结束，有时分则按具体时刻；确有发布时间才用 notAfterNow 排除未发布条目。

原生异步命令可能吞异常，不能从处理函数返回推断成功。适配器从调用前文件游标、当前进程及调用时间范围关联逐码日志，完整成功/失败才给对应结论；缺证、别进程或不完整保持 unknown。取消仅请求当前命令支持的取消，不可取消的命令须等其真实结束，期间不得放开锁或执行下一项。
新契约不存在时先核对已安装软件与当前驻留桥的能力。普通 enable 在旧桥仍响应时可能复用它；更新后先停用旧桥再连接并确认新目录，若重注入失败或新接口仍不存在，再重启 BetterGI 后连接。不要声称普通重连必然热更新，也不能把旧桥缺能力说成目标永远做不到。

归属和结果补充：新上下文只在命令实际进入 UI 线程后、首个异步等待之前原子捕获，不用异步期间的全局差集认领窗口；否则只复用计划开始时唯一上下文。observe 从目标集合取本次 fromStep 更新的数据，返回限定范围的 runtimeData 成功；game 计划仍须真实业务回执。配置为仅拉取时不能附加兑换命令。游戏计划使用当前异步执行的身份请求它支持的取消，不对全局取消上下文发停止；不支持取消的当前命令须等其真实异步工作结束，Job/执行锁在此之前不可确认为释放。
