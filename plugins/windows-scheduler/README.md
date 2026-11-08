# 定时任务插件

为已发布快捷任务设置指定日期、每天、每周或每月的执行时间。通过应用内 MCP 提供 `scheduler.*`，默认关闭；“工具与扩展”保留总开关，在定时任务页面创建或管理任务时会随该操作接通插件，不要求额外启用。插件依赖同一轮构建的桌面程序，不能只复制此目录到旧程序使用。

## 能力与边界

- 查询任务标题和保存内容，取得真实 ID 和发布修订。
- 任务库“默认”提供“关闭游戏”“关闭工具软件”，可复制拖入、排序及重复添加。自动关联前面最近一个快捷任务的真实软件归属；连续收尾项沿用该任务，未识别或未支持时拒绝保存，不接受 PID、进程名和程序路径。当前 BetterGI 关闭原神与关闭当前连接的 BetterGI 进程分别处理，关闭工具先请求正常退出，五秒未退出时只结束已锁定的该进程。
- 预览、创建、修改、启停、删除安排，立即执行一次，停止本轮，查询逐项历史。
- 列表允许 A → B → A，每项有独立 ID 和运行记录；单项错误后继续尝试下一项。资源仍被占用时记录原因，不强行并发。
- 当前管理员账户、Windows 最高权限和交互登录；程序路径、账号及系统目录由后端固定生成。
- 指定日期包含年月日、时分及 UTC 偏移，保存不立即运行。按天支持每 1–365 天，以开始日期为起点、保持本地时刻，跨周跨月不断开；改名、停启和编辑不自动重置起点。每月支持多个日期和月末；不存在的日期跳过，重合时只触发一次。
- 默认准入 BGI 已有配置组启动及其只读准备。执行副本等待完成，原快捷任务不变；其他 BGI 写入方法不放行。
- 其他动作由应用代码通过 `Scheduler::register_action` 登记工具名和来源，并要求无模型、允许无人值守的确定契约。第三方清单自行声明不构成准入。
- 不接受任意命令、exe 路径、XML、账号、运行时参数模板或嵌套定时管理动作。

## 调用

AI 的入口是插件技能 `schedule-task`：简短约定随插件装载，问答分支从 [对话流程](skills/schedule-task/references/conversation.md) 按需读取，完整字段和结果判断见 [接口契约](skills/schedule-task/references/api.md)。只通过现有 `user.ask` 控件补齐必要选择，不另建问答通道；不会为命名、已有顺序、可查询事实或已授权操作反复确认。

`scheduler.clock` 只读返回本机当前日期、时间与星期。可传 `localDateTime: "YYYY-MM-DDTHH:mm"`，由后端按目标日期的 Windows 时区规则返回 `resolved.at` 和 `resolved.future`，避免模型猜日期与 UTC 偏移。它不能调整时钟或选择运行账户。插件停用后，技能、参考资料入口及 MCP 工具一起退出当前能力目录。

先以 `scheduler.actions.list` 查询真实引用，再调用 `scheduler.create`：

```json
{
  "name": "晚间安排",
  "trigger": { "kind": "daily", "localTimes": ["20:00"] },
  "entries": [
    { "binding": { "kind": "shortcut", "shortcutId": "<任务A的真实ID>", "expectedPublishedRevision": 1 } },
    { "binding": { "kind": "shortcut", "shortcutId": "<任务B的真实ID>", "expectedPublishedRevision": 2 } },
    { "binding": { "kind": "shortcut", "shortcutId": "<任务A的真实ID>", "expectedPublishedRevision": 1 } }
  ]
}
```

相同参数的创建重试返回已有安排。修改携带 `id`、`expectedRevision` 和完整 `schedule`，保留既有项的 `entryId`。时间跟随本机时区，错过跳过、重叠跳过是固定策略。

`scheduler.run_now` 使用本次请求的 UUID `requestKey`，重试复用；`scheduler.stop` 使用 `occurrenceId`。暂停只影响后续触发，删除不删除源任务，历史保留。

## 实现与验收

源码在 `src/scheduler/`，使用现有用户数据库。系统入口为程序自身的 `<配置文件> --run-schedule <安排ID>`，这些参数不交给模型。已有程序通过持久队列接收；冷启动只运行固定任务，不恢复或取消普通聊天。插件包版本为 0.1.3；新增默认动作和时钟接口需要本轮主程序，不能只替换插件文本到旧宿主。其他应用可由可信代码通过 `register_default_action` 登记各自的固定关闭接口；插件清单或 AI 自行声明不构成准入。

入口为侧栏独立的“定时任务”，与“快捷任务”平级。快捷任务卡片和右侧任务栏的“安排时间”会跳转到该页，打开弹窗并预填所选任务。设置弹窗支持指定日期、每天、每周、每月，时分可输入或循环拖动；右侧任务库搜索标题和内容，拖入左侧生成独立执行项，可重复添加及排序。创建并启用使用同一后端服务，保存失败保留草稿。

浏览器交互验证入口为 `node tests/ui/scheduler-ui-check.mjs`，生产构建附加 `--production`。隔离测试覆盖真实 MCP 握手和确定性执行器的有序重复调用、错误继续；这不等于 Windows 管理员到时触发或 BetterGI 实机验收。
