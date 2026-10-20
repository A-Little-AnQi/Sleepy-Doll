# 把已有任务加入快捷入口

只封装用户指定的已有项目。先用本轮只读证据确认目标仍存在，并取得当前接口契约；旧对话可定位名称，不能证明今天的文件、启用项或参数没有变化。更新已有入口用 shortcut.list/get 的真实 id。

## 调用层级

BetterGI 的 methodId 与 Agent 工具名是两层。bgi.api.describe 返回的 agentCall 给出调用工具；写动作通常为 bgi.api.invoke，只读动作为 bgi.api.read。业务参数放在 arguments.arguments。不要把 bgi.run_script_group 等 methodId 直接写成 binding.action.tool，也不保存 SDK 映射别名。

单组入口示例（组名须替换为本轮确认的真实名称）：

```json
{
  "name": "千星",
  "description": "运行已配置的千星组",
  "binding": {
    "applicationName": "BetterGI",
    "targetName": "千星",
    "prepare": [{"tool": "bgi.api.describe", "arguments": {"methodId": "bgi.run_script_group"}}],
    "action": {"tool": "bgi.api.invoke", "arguments": {"methodId": "bgi.run_script_group", "arguments": {"groupName": "现场确认的组名", "waitForCompletion": false}}}
  }
}
```

多个组按用户要求的顺序绑定一次 bgi.run_script_groups，内层参数是 groupNames 数组，可带 closeGameAfter。prepare 读取同一 methodId。默认 waitForCompletion=false，后续组与关闭游戏由 BetterGI 的计划执行，不用模型守护，也不因此拆成多个快捷入口。仅用户明确要求等待结果才使用 true。

其他有序动作使用 binding.steps，每步保留 title、prepare 和 action；不存临时 Job、上下文对象、目录版本、配置过程或未要求的准备写入。

## 保存与失败

调用 shortcut.save 保存；普通对话成功后即已加入，封装表单则只生成预览，等用户在表单确认。用户给定名称照用，否则使用简短自然名称。

保存只编译和登记绑定，不执行动作，不要求原神运行、截图器启动或 gameReady=true。状态接口能响应就不能把 bridgeReady=false 解释为连接失败。绑定工具未登记或参数层级不符时，按错误与当前目录修正绑定；不启动游戏、不导航界面、不更换存储通道。接口确实不存在或目标无法确认时说明具体缺口。

成功后立即结束，不试跑。以后点击运行时由确定性执行器核对当前契约和真实前提，不调用模型；用户改了 BetterGI 配置后使用其当前内容，目标已删除或接口不兼容时如实失败。
