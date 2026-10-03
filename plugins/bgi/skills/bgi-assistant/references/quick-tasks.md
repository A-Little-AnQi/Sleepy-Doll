# 指定任务加入快捷任务

只有用户明确说“加入快捷任务”“添加快捷入口”或在快捷任务 AI 配置区要求保存时创建。在普通原对话中，保存用户明确指定的已有／已产出项目。在快捷任务封装表单中，只读定位已有项目并核对精确名称、内容和当前调用契约，不订阅、不新建组、不更改参数，也不执行任务；需要新建时说明应先在原对话完成。保存时不运行；不要把整个配置过程、订阅、准备组、UI 选择操作或历史 Job 放进入口。

对于已配置的 BetterGI 调度器组，用现场读取到的精确组名绑定调用。单个组绑定 `bgi.run_script_group`；多个组的组合绑定一次 `bgi.run_script_groups` 批量调用（groupNames 为有序字符串数组，closeGameAfter 表示结束后关闭游戏，默认 waitForCompletion=false 启动即交接）。`bgi.api.invoke` 每次运行都要求当前运行已读取接口契约，所以在 binding.prepare 中放同一 methodId 的 bgi.api.describe；不要固化目录版本、临时 Job 或界面对象 ID。保存前已经读取实际契约，参数必须与之相符。

```json
{
  "name": "挖矿讨伐",
  "description": "按顺序运行已配置好的挖矿与讨伐配置组，结束后关闭游戏。",
  "binding": {
    "applicationName": "BetterGI",
    "targetName": "挖矿讨伐",
    "prepare": [{"tool": "bgi.api.describe", "arguments": {"methodId": "bgi.run_script_groups"}}],
    "action": {"tool": "bgi.api.invoke", "arguments": {"methodId": "bgi.run_script_groups", "arguments": {"groupNames": ["现场读取的配置组名一", "现场读取的配置组名二"], "closeGameAfter": true, "waitForCompletion": false}}}
  }
}
```

这是 `shortcut.save` 的示例参数。groupNames 只能现场读取，不照抄示例；入口名称以用户给定的为准（用户给的照用不强改），助手自拟时取 ≤20 字自然中文目标短名（如「挖矿讨伐」），不用 +、@、目录、作者、版本、时间戳拼串。`binding.action.arguments` 外层固定 `{methodId, arguments:{…}}`，宿主参数只在内层；编辑入口只改内层参数值（如 waitForCompletion），不得扁平化或改变层级，保存时按契约校验。与内嵌快捷创建相同的 canonical binding 区分：`binding.action.tool` 固定写当前运行时的 canonical 工具名 `bgi.api.invoke`，宿主 methodId（如 `bgi.run_script_groups`）不是 Agent 工具名，SDK 映射别名不是持久化的 `binding.tool`。没有实际登记的专用运行入口时不猜、不绑不存在的工具名。

修改入口时使用现有 id 更新；更改应用中的任务配置与更改入口是两件事，按用户要求分别完成。多个配置组的组合是一个入口、一次 bgi.run_script_groups 批量调用：groupNames 按用户要求的顺序排列，closeGameAfter 覆盖关闭游戏等收尾；不拆成多个入口，也不按组拆成 steps 拼长期等待。默认 waitForCompletion=false（启动即交接，无逐组等待）；只有用户明确要求等结果才设 true。其他应用的组合仍用 binding.steps 逐步保存 title、prepare、action；每一步的动态调用在该步 prepare 中读取同一 methodId 的契约。不得要求用户拆分、只选其中一项或跑完后再另行关闭游戏。旧版流程按用户指定的动作顺序重新绑定，不运行旧流程进行猜测。

普通运行请求不会顺手创建快捷入口。快捷任务运行时不重新订阅、配置或调用模型；前置未就绪时说明实际问题，由用户明确决定是否修改配置。面向用户只说“任务已加入快捷任务，点击即可运行”，不要展示工具、接口、桥接或绑定参数。

封装表单中的 shortcut.save 只生成预览，AI 应立即结束，不声称已保存；用户会在表单确认后保存。若提供原对话资料，复用其中已核实的具体名称并按需只读确认，不另开一项新任务。普通原对话中的明确加入请求则直接保存入口。

快捷入口的批量组调用默认 waitForCompletion=false：启动即交接，宿主计划继续执行，不逐组等待；用户明确要求等结果的入口才设 true，由无模型执行器跟踪运行／停止。普通对话的运行请求同样默认 false，启动交接后结束，不继承快捷入口的等待选项。
