# 当前可见功能的原生链路

稳定的资源、设置事务和任务入口优先。当前编辑器、列表行或弹窗没有稳定入口时，使用原生 UI 链路，不能把“自动适配缺失”说成 BGI 功能不存在。

1. 主页面先 list_pages/open_page，窗口用当前命令打开。含模态窗口的命令返回 operationId，初始阶段不等于结束。
2. describe/read `bgi.ui.read`，按 query、offset、limit 查看实际可见字段、按钮、容器和窗口。未展开的分组、菜单、页签用 ui.invoke 的 expand/contextMenu/focus，再读具体内容。字段来自真实 WPF 绑定，覆盖全局配置、窗口设置、列表行和 JS 自定义配置。
3. `bgi.ui.write` 使用真实 fieldId、expectedVersion 和 valueSchema。下拉／列表使用返回的 optionId；后续选项用 ui.options 分页。排序使用当前列表字段及版本的 ui.reorder，触发原生 Move 和集合保存回调。不能从 JSON 伪造宿主对象；旧绑定、虚拟列表换行和选项变化重新读取。敏感字段只提供明确的新值，不提交遮蔽占位。
4. 原生校验或联动发生后读取实际值；输入不等于落盘。用 ui.invoke 点击当前真实保存／确认按钮，或按该窗口语义 ui.close。关闭可能应用设置、保存文件或关闭到托盘，不能把“窗口消失”当目标核验。
5. Prompt／文件／确认弹窗使用该 operationId 的 ui.respond，提供 text、filePath、selectedValues 或 confirm；作用域保持最初调用前的窗口基线，因此已出现的本次弹窗仍可填写。一份输入只确认一个阶段，多层弹窗逐层读取和补充。
6. 用 ui.operation 取得真实命令返回、异常或取消，再回读配置、资源、日志或游戏状态。runningOrAwaitingInput 不能报告完成；cancel 请求不是已停止，编辑器仍开着时用其真实取消按钮收尾。

Loaded、Closing、输入事件不脱离界面伪造调用。它们由真实控件、绑定与窗口流程触发，功能索引保留用户可见目标及其原生链路。弃用跟踪和空测试入口保持删除；仍存在的可见功能不因需要弹窗或人工验证码而从知识目录消失。
