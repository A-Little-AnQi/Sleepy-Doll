---
name: bgi-operator
description: 使用 BGI Bridge 查询游戏状态、检索能力、提交动作并验证执行结果
tags: BGI, 原神, 游戏状态, 自动化, Bridge
---

# BGI 操作规范

1. 执行动作前先用 `bgi.state.get` 获取新鲜状态，不根据历史对话猜测当前画面。
2. 首先用 `bgi.api.search` 按用户目标检索当前宿主的完整接口目录。每条结果包含用途、调用时机、主要参数、副作用与可用状态；不是只有接口名。使用 `offset=nextOffset` 继续读取，不能把第一页当作全部。禁用或尚不支持的接口仍有说明，不得绕过限制。
3. 对选中的接口调用 `bgi.api.describe`，阅读完整参数/返回结构、前置条件、示例、核验方法和回退边界。详情版本必须属于当前宿主实例。只读接口使用 `bgi.api.read`，写接口使用 `bgi.api.invoke`；两者都传目录里的精确 `methodId` 和符合 `inputSchema` 的 `arguments`。没有参数时传 `{}`，不要编造参数。
4. 修改配置先读取 `bgi.get_setting`（也可读取具体 `setting.*`），检查 `writable`、`valueSchema` 和 `valueVersion`。用 `bgi.preview_settings` 提交 `path/value/expectedVersion` 得到差异与 `planId`，核对用户授权后调用 `bgi.commit_settings`。保存返回的 `changeId`；回退使用 `bgi.rollback_settings`，遇到冲突重新读取，不覆盖用户后续修改。遮蔽字符串不是可写回的秘密值。
5. 宿主命令的配置检查点不覆盖脚本文件、外部通知或游戏进度。不能承诺这类操作可以撤销。宿主无法启动时，引导用户关闭 BetterGI 后使用设置页的“恢复记录”，核对备份时间与差异后恢复；不要直接覆盖配置文件。
6. 已安装的语义能力仍通过 `bgi.capability.search/describe/invoke` 调用；它们不是完整宿主接口目录。只调用目录中真实存在且已授权的接口或能力，不编造路线、脚本、传送点、角色状态或成功结果。
7. 写入返回 Job 只代表请求已接纳。使用 `bgi.job.get` 等待终态，区分调用完成与业务验证成功。`verification.status` 为 `unknown` 时重新观测或明确说明无法确认，不盲目重复写操作。
8. `PERMISSION_DENIED` 不重试，也不寻找旁路。`INSTANCE_MISMATCH` / `CATALOG_MISMATCH` 重新检索并读取说明；`CONFIG_CONFLICT` 重新读取配置并预览；`OUTCOME_UNKNOWN` 先观察，不换请求标识重发。
9. 停止请求使用 `bgi.job.cancel`，但取消等待不保证宿主命令停止。进入 `stoppingUnconfirmed` 时不再提交游戏写任务，也不得停止不属于当前请求的其他任务。
10. 未连接 Bridge 时明确说明未执行，不用模拟结果代替真实调用。
