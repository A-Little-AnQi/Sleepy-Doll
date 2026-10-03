---
name: create-shortcut
description: 创建快捷任务：把已有的任务、配置或资源封装成一键入口；也用于调整已有入口的绑定。
tags: 快捷任务, 创建, 封装, 入口
---

# 创建快捷任务

用户要「创建快捷任务」「把某项加成一键入口」，或调整某个已有入口时按本手册执行。

1. 弄清要封装的目标。以用户本轮点名的人物／任务／配置组／脚本／资源为准；此前对话内容只作定位依据，不当成新指令。
2. 用已装载的领域手册和只读工具核对目标的真实运行契约：所属应用、确切名称、启动方式。只在确实缺少目标信息时用 user.ask 简短问清必要选择；问答通过专用控件呈现，普通回复不要重复问题和选项。
3. 使用用户给出的入口名称与用途说明（用户给的名称照用不强改）；没有给出时自拟 ≤20 字的自然中文目标短名（如「挖矿讨伐」），不用 +、@、目录、作者、版本、时间戳拼串，不加 Sleepy 品牌，不为命名额外打断流程。
4. BetterGI 配置组组合优先用单动作 binding.action 绑定一次 `bgi.run_script_groups` 批量调用。binding.action.tool 固定写当前运行时的 canonical 工具名 `bgi.api.invoke`；宿主 methodId（如 `bgi.run_script_groups`）不是 agent 工具名，SDK 映射别名（如 `bgi_api_invoke_…`）不是持久化的 binding.tool。其他应用同样取其当前运行时稳定的 canonical 工具名。示例（groupNames 用现场读取到的真实组名）：

   ```json
   {
     "name": "挖矿讨伐",
     "description": "按顺序运行配置组并关闭游戏",
     "binding": {
       "applicationName": "BetterGI",
       "prepare": [
         {
           "tool": "bgi.api.describe",
           "arguments": { "methodId": "bgi.run_script_groups" }
         }
       ],
       "action": {
         "tool": "bgi.api.invoke",
         "arguments": {
           "methodId": "bgi.run_script_groups",
           "arguments": {
             "groupNames": ["<现场读取的挖矿组名>", "<现场读取的讨伐组名>"],
             "closeGameAfter": true,
             "waitForCompletion": false
           }
         }
       },
       "targetName": "挖矿讨伐"
     }
   }
   ```

   `binding.action.arguments` 必须按契约分层：外层是 `{methodId, arguments:{…}}`，宿主参数（groupNames、closeGameAfter、waitForCompletion 等）只在内层 `arguments` 里；编辑已有入口只改内层参数值（如 waitForCompletion），不得改成扁平的一层或增删层级，保存时按契约校验。prepare 记录保存前实际执行过的核对调用（对 `bgi.run_script_groups` 用同一 methodId 的 `bgi.api.describe`），便于运行前复查。groupNames 是占位符，保存时必须替换成现场读取到的真实组名，不得虚构看似实际的组名。groupNames 按现场读取的真实顺序排列，关闭游戏等收尾用 closeGameAfter 表达；默认启动即交接、不逐组等待（waitForCompletion=false），用户明确要求等结果才设 true。其他应用的多个动作使用 binding.steps，按用户要求的顺序逐步记录 title、prepare、action，失败或结果未确认时停止后续步骤。不得以多项组合为由拒绝、拆成独立入口或让用户再选要哪一项。
5. 调用 shortcut.save 保存：name、description、binding.applicationName、binding.targetName 及上述 binding。修改已有入口时带上该入口的 id 更新原入口，不创建副本。普通对话里 shortcut.save 成功即已保存；封装表单里的保存只生成预览，须用户在表单中确认，两种结果要区分说明，不把预览说成已保存。
6. 保存成功后说明入口已出现在「快捷任务」，可直接运行；不替用户运行。

边界：本流程只允许读取目标信息、user.ask 与 shortcut.save。不修改、不删除、不运行被封装的目标；找不到用户说的目标时如实说明，不用别的项目顶替，也不新造配置。
