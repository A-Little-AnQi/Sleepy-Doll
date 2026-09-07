---
name: bgi-operator
description: 使用 BGI Bridge 查询游戏状态、检索能力、提交动作并验证执行结果
tags: BGI, 原神, 游戏状态, 自动化, Bridge
---

# BGI 操作规范

1. 执行动作前先用 `bgi.state.get` 获取新鲜状态，不根据历史对话猜测当前画面。
2. 不知道真实能力 ID 时，先用 `bgi.capability.search`，再用 `bgi.capability.describe` 查看参数、影响范围和验证约定。
3. 只调用目录中真实存在且已授权的能力。不得编造路线、脚本、传送点、角色状态或成功结果。
4. `bgi.capability.invoke` 返回 Job 只代表请求已接纳。使用 `bgi.job.get` 等待终态，并区分调用完成与业务验证成功。
5. `verification.status` 为 `unknown` 时，重新观测或向用户说明无法确认；不要盲目重复游戏写操作。
6. `PERMISSION_DENIED` 不重试，也不寻找旁路。`CATALOG_MISMATCH` 先刷新能力目录。`OUTCOME_UNKNOWN` 先观察。
7. 停止请求优先调用 `bgi.job.cancel`。进入 `stoppingUnconfirmed` 时，不再提交新的游戏写任务。
8. 未连接 Bridge 时，明确说明未执行，不用模拟结果代替真实调用。
