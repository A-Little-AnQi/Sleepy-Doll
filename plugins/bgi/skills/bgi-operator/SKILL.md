---
name: bgi-operator
description: BetterGI 的资源管理、设置修改、任务运行和排障；按需读取判断依据，接口与参数由当前能力目录提供。
tags: BGI操作
---
# BetterGI 操作

先理解用户对象和操作范围，再从当前能力的名称、说明、前置条件和影响中选择。读取真实契约后使用其返回的标识、参数和对象；不从技能复制接口名、页面名或数据路径。离线源码资料只能解释所对应版本，不能证明当前软件可调用。

已有工具说明足以完成一次读取或修改时直接处理；涉及不清楚的对象含义、影响或核验方法时，再读下列相关资料。不要为每次操作都依次加载全部手册。

| 所需知识 | 资料 |
|---|---|
| 用户资源、配置组与删除边界 | [references/resources.md](references/resources.md) |
| 全局设置与联动 | [references/settings.md](references/settings.md) |
| 任务准备、后台运行、停止 | [references/execution.md](references/execution.md) |
| 仓库、订阅和版本 | [references/repository.md](references/repository.md) |
| 脚本参数与错误 | [references/script-analysis.md](references/script-analysis.md) |
| 兑换码的来源和结果 | [references/redeem.md](references/redeem.md) |
| 缺项、错误及答复 | [references/shared.md](references/shared.md) |
| 必须操作原生界面 | [references/native-ui.md](references/native-ui.md) |
| 恢复历史设置 | [references/settings-recovery.md](references/settings-recovery.md) |

优先直接管理数据、设置和任务的能力。参数来自当前契约、实际对象与用户要求；保留未知字段和并发变更。修改、准备和运行分别按用户范围执行，不能因对象已可运行就自动启动。核实对应结果，不能用处理器返回代替业务完成。
