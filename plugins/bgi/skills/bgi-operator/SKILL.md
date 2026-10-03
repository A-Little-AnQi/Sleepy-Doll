---
name: bgi-operator
description: 执行 BGI 操作时按任务读取资源、设置、运行、仓库或脚本排障流程；不常驻完整手册。
tags: BGI操作
---

# BGI 操作说明

按当前目标读取一份相关流程，已取得的路径、版本和接口 ID 可复用。bgi.feature.search/read 返回单项卡片，bgi.api.describe 返回当前调用契约；静态索引不授予执行权限，不证明运行中的版本有该接口。

直接资源、设置和领域数据接口优先；界面命令、页面上下文和 ui.* 是最低优先级。先 describe 当前直接接口；read/invoke 按本次契约 effect 选择。删除路线无需先选中路线、打开页面或建立上下文。

路线／配置组文件的关键层：组 JSON 的运行参数在 config.pathingConfig（enabled、partyName、hurryOnAvatar、autoFightConfig、collectTimeout 等），projects 是任务清单不改内容；队伍与赶路角色用 `bgi.set_pathing_party` 原生写入，其余参数=全文读→只改点名的 pathingConfig 字段→保留其余字段与完整 SHA 写回→回读。

| 当前目标 | skills.reference 的 path |
|---|---|
| 查询、创建、修改或删除配置组／用户资源 | [references/resources.md](references/resources.md) |
| 修改全局设置与联动事务 | [references/settings.md](references/settings.md) |
| 准备运行、检查游戏、停止或核对任务 | [references/execution.md](references/execution.md) |
| 更新仓库与订阅 | [references/repository.md](references/repository.md) |
| JS 参数、资料与脚本报错 | [references/script-analysis.md](references/script-analysis.md) |
| 需要澄清或遇到阻塞 | [references/shared.md](references/shared.md) |

参数必须来自资源、脚本定义或当前接口 Schema。修改比较读取版本，保留未知字段；全局配置使用设置事务。游戏就绪仅用于游戏任务，资源查询、删除和页面导航不先启动游戏。

提交后按卡片和契约验证。已验证的 deleted/prepared/opened 等状态只证明该项操作；用户要求运行时仍须继续执行。无可调用接口时说明具体限制和可核实的替代路径，不扫描桥二进制，不编造完成结果。
