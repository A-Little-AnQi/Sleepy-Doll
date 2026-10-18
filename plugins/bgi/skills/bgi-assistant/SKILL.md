---
name: bgi-assistant
description: BGI 插件的简短目标路由。用于 BetterGI 的脚本、路线、配置、任务、设置、页面和故障；具体链路按功能索引读取。
tags: BetterGI, BGI, 原神, 游戏脚本
alwaysLoad: true
---

# BGI 目标入口

仅用于 BetterGI；其他应用不套此规则。以熟悉 BGI 日常用法的助手表达：直接称 BetterGI/BGI，用配置组、调度器、地图追踪、脚本参数、战斗策略、订阅说明操作和结果；不说“宿主/注入”，不写研究接口的探路旁白。现场事实自行核实，不凭熟悉感编造版本或脚本行为。开发排错保留真实代码名称与错误线索。

## 选择下一步

- 运行／采集材料、路线或脚本：`bgi.user.resolve`，按 verdict 准备、运行。现成路线改参数走组直配，战斗配置读 bgi.inspect_group_effective；不送 JS 编写。
- 路线组直配：describe 后 invoke `bgi.prepare_pathing_group`→按返回适配→`bgi.set_pathing_party` 写队/赶路角色→回读；不扫叶子、不猜 Party 目录。细节读 bgi-operator 的 references/resources.md。
- Pathing 配队：仅 partyConfirmationRequired=true 时补必要信息→inspect_group_effective 读有效策略（file 读原文、auto 按队伍核对）→适配回读→run；false 不问，inspectionRequired=true 查路线而非问队伍。JS 按脚本自身必需参数/校验，选填或已有配队不重问。
- 用户要求新建/修改且必要意图缺失才 user.ask；不替用户从旧配置或名单挑选。运行已有组沿用保存参数，不例行查队伍或读源码。
- 脚本参数或源码含义：本机 `bgi.user.inspect_script/read`，仓库 `bgi.repo.search/read`。写 JS／OCR／BGI API 加载 `bgi-javascript`。
- 检查配置、优化运行：读 [guide-workflow.md](references/guide-workflow.md) 的“配置诊断与运行优化”，结合有效配置、脚本与运行证据判断；未开启的可选项不是缺陷。只检查时不修改、不启动。
- 界面交互：读 bgi-operator 的 references/native-ui.md。
- 删除配置组：定位精确 name/sha256 后 describe/invoke `bgi.delete_script_group`，不操作界面。删除资源走 inspect_local_resource → delete_local_resource。
- 停止任务：等待中的 Job 用 `bgi.job.cancel`；已交接启动 Job 已终态，用 `bgi.stop_current_task` 并核对原目标。
- 对象不明确：`bgi.feature.search` → `bgi.feature.read(live:true)` 按 nextMethodOffset 分页读单项当前契约；已知 ID 直接 describe。状态规则见 references/user-goals.md。
- 口语目标选域：读 references/user-goals.md。快捷任务：读 references/quick-tasks.md。

入口自拟名用 ≤20 字自然中文，不拼目录、版本或时间戳；用户命名照用。

多组一次 bgi.run_script_groups，普通对话和快捷入口默认 waitForCompletion=false 后台交接，仅明确要等结果才 true；详见 references/action-policy.md。

按功能卡 references 读资料，权限由运行时处理；只报告核实结果与实际阻塞。
