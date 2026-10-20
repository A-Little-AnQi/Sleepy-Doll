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
- 路线组配置：读取 bgi-operator 的 references/resources.md；prepare_pathing_group 后按需 set_pathing_party 并回读，不改叶子路线。
- 配队仅按工具证实的 Pathing 必要条件补缺项；inspectionRequired 是资源检查。JS 依据自身参数和校验，选填或已有队伍不重问；不例行重配。
- 必要意图缺失才 user.ask；用户已明确或委托选择的不重复问。运行已有组沿用保存参数，不例行查队伍或读源码。
- 脚本参数或源码含义：本机 `bgi.user.inspect_script/read`，仓库 `bgi.repo.search/read`。写 JS／OCR／BGI API 加载 `bgi-javascript`。
- 配置诊断：读 [guide-workflow.md](references/guide-workflow.md)，结合有效配置、本机脚本和运行证据；选填留空不算缺陷，只检查时不修改、不运行。
- 界面交互：读 bgi-operator 的 references/native-ui.md。
- 删除配置组：定位精确 name/sha256 后 describe/invoke `bgi.delete_script_group`，不操作界面。删除资源走 inspect_local_resource → delete_local_resource。
- 停止任务：等待中的 Job 用 `bgi.job.cancel`；已交接启动 Job 已终态，用 `bgi.stop_current_task` 并核对原目标。
- 对象不明确：bgi.feature.search/read 定位功能，live:true 按 nextMethodOffset 补读当前契约；已知 ID 直接 describe。
- 口语目标选域：读 references/user-goals.md。封装快捷入口时读 [quick-tasks.md](references/quick-tasks.md)，按 describe 返回的 agentCall 绑定；工具名是 bgi.api.invoke/read，接口标识放 arguments.methodId，业务参数放 arguments.arguments。保存不运行。

状态接口响应即已连接；gameReady（旧 bridgeReady）为 false 只说明游戏未就绪，不阻止配置读写或保存入口。绑定错误按具体错误修复，不解释成断连。

多组用 run_script_groups，默认启动即交接；仅明确要求等结果才 waitForCompletion=true。详见 references/action-policy.md。

按功能卡 references 读资料，权限由运行时处理；只报告核实结果与实际阻塞。
