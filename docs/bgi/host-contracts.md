# BGI 宿主契约

注入桥运行在 BetterGI（BGI）进程内，通过反射调用宿主已有的能力。下面这些约束来自宿主自身
的实现方式。

宿主源码基线：`babalae/better-genshin-impact`，提交
`b3e46b3004b8e4a1065846243a3a2a518b9a214d`。以下结论来自该提交的静态核对，没有构建或运行宿主。
接口清单以桥当前生成的目录为准，见 [配置与数据存放](../configuration.md) 的「BetterGI 接口说明与配置恢复」。

## 宿主源码事实

### 宿主已经有一个容器

`App.xaml.cs` 通过 `Host.CreateDefaultBuilder()` 构建 Generic Host，集中注册 `IConfigService`、
`IScriptService`、`TaskTriggerDispatcher`、地图服务等对象。桥接入这一个宿主，容器由宿主构建，
桥不对现有 `IServiceCollection` 再次调用 `BuildServiceProvider()`。

### 能力来源有四类

只扫描 Service 会漏掉大部分可调用能力：

| 来源 | 例子 | 接入方式 |
| --- | --- | --- |
| 容器注册的服务 | `IScriptService`、地图 API Service、配置 Service | 按注册契约扫描，不提前实例化 |
| 脚本宿主对象 | `Genshin`、`AutoPathingScript`、`Dispatcher` | 类型级 Provider + 延迟构造 |
| 静态能力 | `Bv`、`TaskControl` 的相关识别函数 | 只扫描明确允许的类型 |
| 独立任务与不可远程化对象 | `PathExecutor`、`TpTask`、`ScriptProject` | 参数简单的接入；依赖上下文或结果不可靠的新增薄适配 |

脚本宿主对象由 `EngineExtend.InitHost` 集中装配，不在容器里，构造还可能依赖
`TaskContext.SystemInfo`：这类对象在游戏初始化之后才可创建，扫描目录阶段不构造。

`Genshin` 提供的是 `Tp(double x, double y, ...)` 这类重载，没有 `sceneId + pointId` 形式的签名。
对外若需要传送点 ID，由外层资源索引把 ID 解析成地图名与坐标，再绑定到实际签名。

### 部分入口依赖 ViewModel，不是无 UI 入口

`ScriptService.StartGameTask()` 依赖 `HomePageViewModel`；`RunMulti()` 的部分逻辑访问
`ScriptControlViewModel`；脚本 `Dispatcher.RunTask()` 的部分分支读取 `TaskSettingsPageViewModel`。
反射能发现这些入口，但依赖仍然存在。需要无 UI 入口时另建薄适配，原 Service 保持原样。

### 调用正常返回不代表业务成功

- `TaskRunner.RunCurrentAsync()` 在获取执行锁失败时直接返回，也会捕获并记录部分执行异常。
- `AutoPathingScript.Run()` 会捕获并记录路径执行异常。
- `PathExecutor.SuccessEnd` 也不能独自证明路线完整成功：`HandledException` 分支会把它设为
  `true`，而部分不可继续的场景同样走到这类异常。

因此调用完成、执行器报告、目标实际达成是三件不同的事，最后一项只能靠独立观测。

### 脚本失败只留在宿主的按天日志里

宿主没有查询运行历史的接口，脚本失败只能从日志读：`<安装目录>\log\better-genshin-impact<yyyyMMdd>.log`，
UTF-8，每个自然日一个文件。记录之间用空行分隔，首行是
`[HH:MM:SS.mmm] [级别] [Primary:S<会话>:P<进程>:T<线程>] <记录器>`，正文在其后的行里；`T<线程>`
是同一次运行的关联键。

同一次失败可能被写两遍：`[ERR]` 行的 `执行脚本时发生异常: "消息"`，以及 `[DBG]` 行的
`执行脚本时发生异常` 加随后独立的 .NET 异常文本（`System.IO.DirectoryNotFoundException: ...`）。
两种写法不共享记录标识，桥按线程号、时间窗和异常消息归并。

栈帧里的路径有两类：宿主自身的是构建机路径（`D:\a\better-genshin-impact\…\File.cs:line N`），
用户文件则是安装目录下的单引号绝对路径。本机采集到的日志里没有 `.js:行:列` 形式的帧，
脚本自身的位置靠脚本名与源码对照，桥不解析 JavaScript 调用栈。

### 执行互斥只有一个入口

`TaskControl.TaskSemaphore` 是进程级的独占任务锁，被调用的宿主入口内部已经持锁；两层都加锁
会让 `TaskRunner.RunCurrentAsync()` 在二次加锁后直接返回。桥只读 `TaskSemaphoreCount` 判断当前
是否有任务持锁，写操作前发现有任务持锁则拒绝并返回 `BUSY`。

## Job 与业务成功分离

只读调用直接返回结果；写到宿主的调用不等待结果，先接纳并返回 Job 标识，再凭 Job ID 查询。
接纳后进入 Job 生命周期：

```
queued → running → completed / failed / cancelled
```

声明的状态还有 `cancelling`、`interrupted` 与 `stoppingUnconfirmed`：前两者分别表示「取消已请求、
宿主未确认」和「执行中进程消失」，`stoppingUnconfirmed` 是仍可能持有执行权的非终态，不按已取消
处理。当前桥实际产生的终态是 `completed`、`failed`、`cancelled`，客户端仍需按声明接受全部取值。

`completed` 只表示调用正常结束，业务判定放在独立的 `verification` 字段：

```json
{
  "jobId": "job-example",
  "methodId": "bgi.commit_settings",
  "state": "completed",
  "result": null,
  "verification": {
    "status": "unknown",
    "reason": "处理器返回不代表业务目标已验证。"
  },
  "timestamps": { "acceptedAt": "2026-09-07T10:00:00Z", "endedAt": "2026-09-07T10:00:20Z" }
}
```

- 默认 `verification.status` 为 `unknown`：BGI 不提供权威业务结果。
- 只有处理器自己回读核验过（配置事务组）才置为 `succeeded`，并把依据写进 `reason`。
- Agent 在 `completed` 且 `verification.status` 为 `succeeded` 时记为 `verifiedSucceeded`；其余情况
  用 `/bridge/v1/state` 的新鲜观测按绑定谓词复核，判定为 `verifiedSucceeded`、`verifiedFailed` 或
  `unknown`。成功判定取自验证结果与新鲜观测，桥的 Job 结果不构成成功；`interrupted` 保留执行锁，
  等重新观测后再收敛。

## 错误协议

错误响应体是平铺的 `{code, message}`，HTTP 状态与错误码同时保留。缺项、拒绝原因和被拒后的
处置方式都写在 `message` 里，客户端把它一起交给 Agent。

| 错误码 | HTTP | 含义 | 处理 |
| --- | --- | --- | --- |
| `UNAUTHORIZED` | 401 | 缺少或错误的 Bearer token | 不重试，检查凭据 |
| `FORBIDDEN` | 403 | 非回环来源，或宿主拒绝访问 | 不重试 |
| `DISABLED` | 503 | 连接开关已关闭 | 提示用户在界面开启 |
| `METHOD_NOT_FOUND` | 404 | 方法或端点不存在 | 刷新目录，不猜测替代方法 |
| `NOT_CALLABLE` | 409 | 方法存在但当前不可调用 | 读目录里的不可调用原因 |
| `INVALID_ARGUMENT` | 400 | 参数不合规 | 本地修复或询问用户 |
| `INSTANCE_MISMATCH` | 409 | 宿主实例已变化 | 重新读取目录 |
| `CATALOG_MISMATCH` | 409 | 契约版本不匹配 | 重新解析或校验计划 |
| `HOST_CAPABILITY_MISSING` | 501 | 宿主里找不到该类型或成员 | 版本不匹配，按缺失能力处理 |
| `IDEMPOTENCY_CONFLICT` | 409 | 同一请求标识用于不同内容 | 视为客户端错误 |
| `JOB_EXPIRED` | 410 | 原请求记录已过期 | 不重新执行，按新请求处理 |
| `QUEUE_FULL` | 429 | 请求记录已达上限 | 不接受任务，退避后原键重试 |
| `BUSY` | 409 | 有互斥执行（如独立任务持锁） | 有界等待或告知用户 |
| `GAME_NOT_READY` | 409 | 游戏或截图未就绪 | 等待，不重复提交写操作 |
| `EXECUTION_FAILED` | 500 | 执行异常可确认 | 按语义策略决定恢复 |
| `TIMEOUT` | 504 | 宿主操作超时 | 先观测再决定是否重试 |
| `CANCELLED` | 400 | 宿主操作已取消 | 查询 Job 终态 |
| `INVALID_STATE` | 409 | 宿主当前状态不允许该操作 | 重新读取状态 |
| `UNSUPPORTED_HOST` | 409 | 宿主缺少必需的回调或扩展点 | 不修改，报告限制 |

配置事务组还有一组专用码，语义都是「不覆盖已变化的配置」：`CONFIG_CONFLICT`（409）、
`CONFIG_TOO_LARGE`（409）、`INVALID_CONFIG_FILE`（409）、`PLAN_EXPIRED`（409）、
`RECOVERY_REQUIRED`（409）、`RECOVERY_BUSY`（409）、`HOST_RUNNING`（409）、
`COMMIT_REVERTED`（409）。

客户端另有两条判断规则：

- `INTERNAL`（500）是未处理异常的兜底，只带异常消息，不带栈。
- 5xx 中只有 `INTERNAL` 和客户端合成的 `REQUEST_FAILED` 表示响应不是桥按协议写的，
  此时无法判断执行到哪一步；其余带桥错误码的响应说明桥已确认结果。

## 并发限制

宿主现有的取消入口会覆盖全局状态：`TaskRunner.RunSoloTaskAsync()`、`ScriptService.RunMulti()`
以及一条龙 ViewModel 的某些入口，都会在获取执行锁之前调用全局 `CancellationContext.Set()`。
因此在桥一侧观察或获取执行锁，并不能阻止用户同时点界面后替换全局 CTS。

按「不改 ViewModel、不重构原 Service」的约束，支持的运行模式是：一个 BGI 实例进入远程控制期间，
不同时从界面或热键启动另一组业务任务；已有的全局停止可作为人工急停。桥只能观测冲突：写操作前
`TaskSemaphoreCount` 不是正数（有任务持锁，或状态未知）即返回 `BUSY`，`/bridge/v1/state` 报告
`taskLockHeld`。

这是运行约束，不是隔离保证。若要支持界面与 Agent 任意混用，需要在原初始化与取消入口上增加
共享执行所有权机制；「只新增代码就能保证」尚未经过验证。

多实例各自持有队列与 Job，但如果多个实例仍向同一个桌面发送前台输入，依然可能争抢焦点。不同
端口不等于游戏输入已经隔离。

## 取消与超时不等于终止

- Job 接纳后的生命周期不绑定 HTTP 连接：连接断开默认继续，客户端凭 Job ID 重新查询。
- `cancel` 只取消指定 Job，作用范围限于该 Job；它不停止已经开始的宿主命令，宿主任务队列独立于
  该接口。
- 取消响应不带确认：桥只对指定 Job 的取消源发信号，并回 `cancellationRequested: true` 与
  「已开始的宿主命令可能继续运行，必须继续查询终态」。只有处理器观察到取消，Job 才落到
  `cancelled`；否则它仍可能以 `completed` 或 `failed` 结束。
- 超过宽限时间（客户端取 15 秒）仍未确认时，结果保持 `unknown`，执行锁不释放：任务可能仍在按键，
  超时不构成释放锁的依据。
- 同进程内无法安全强杀任意 .NET Task。对不响应取消的第三方脚本或原生调用，不承诺无损强制停止；
  最终恢复可能需要人工结束该 BGI 进程。
- 进程退出、服务停机、任务失败时都会尽最大努力释放键鼠输入，但「已发出松键」不等于旧任务不会
  再按键。
