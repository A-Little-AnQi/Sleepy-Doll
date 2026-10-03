---
name: bgi-assistant
description: BGI 插件的简短目标路由。用于 BetterGI 的脚本、路线、配置、任务、设置、页面和故障；具体链路按功能索引读取。
tags: BetterGI, BGI, 原神, 游戏脚本
alwaysLoad: true
---

# BGI 目标入口

此技能只适用于 BGI 目标；产品是通用 Agent，其他工具及普通对话不套用 BGI 规则。

复用已有证据，自行读取现场信息。路径、状态等可观测事实自己读取，不问用户。调用前读取当前接口契约。

## 选择下一步

- 运行／采集材料、路线或脚本：`bgi.user.resolve`，按 verdict 准备、运行。现成路线改运行参数走路线组直配（见下）；读实际战斗配置/策略用 bgi.inspect_group_effective。不送 JS 编写。
- 路线组直配：describe 后 invoke `bgi.prepare_pathing_group`（继承全局有效配置，返回需求聚合）→ 按返回适配 → `bgi.set_pathing_party` 写队与赶路角色（俗称数据映射）→ 回读。不扫叶子、不猜 Party 目录。细节读 bgi-operator 的 references/resources.md。
- 战斗/特殊路线（partyConfirmationRequired=true）：缺配队 user.ask→inspect_group_effective 读生效配置与策略→file 策略读原文、auto 按队伍选可用→按角色与策略适配组并回读→run；=false 沿用已有配置不额外问。
- 其他缺用户意图值：首轮只用 user.ask 问这一个值；意图值不从旧配置/名单/搜索结果推断。
- 脚本参数或源码含义：本机 `bgi.user.inspect_script/read`，仓库 `bgi.repo.search/read`。写 JS／OCR／宿主 API 加载 `bgi-javascript`。
- 原生界面交互：读 bgi-operator 的 references/native-ui.md。
- 删除配置组：定位精确 name/sha256 后 describe/invoke `bgi.delete_script_group`，不操作界面。删除资源走 inspect_local_resource → delete_local_resource。
- 停止任务：等待中的 Job 用 `bgi.job.cancel`；已交接启动 Job 已终态，用 `bgi.stop_current_task` 并核对原目标。
- 对象不明确：`bgi.feature.search` 后 `bgi.feature.read`；能否调用以 `bgi.api.describe` 为准。已知稳定接口 ID 时直接 describe，不泛搜索。
- 口语目标选域：读 references/user-goals.md。快捷任务：读 references/quick-tasks.md。

入口命名：助手自拟名取 ≤20 字自然中文目标短名（如"挖矿讨伐"），不用 +、@、目录、作者、版本、时间戳拼串；用户给出的名称照用不强改。

多组一次 bgi.run_script_groups，普通对话和快捷入口默认 waitForCompletion=false 后台交接，仅明确要等结果才 true；详见 references/action-policy.md。

按功能卡 references（skill/name/path）读取具体链路，一次只读相关资料。权限由运行时处理，不重复问许可。只报告已核验结果和一个真实阻塞项。
