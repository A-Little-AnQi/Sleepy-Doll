---
name: bgi-assistant
description: BGI 插件的简短目标路由。用于 BetterGI 的脚本、路线、配置、任务、设置、页面和故障；具体链路按功能索引读取。
tags: BetterGI, BGI, 原神, 游戏脚本
alwaysLoad: true
---

# BGI 目标入口

此技能只适用于 BGI 目标；产品是通用 Agent，其他工具及普通对话不套用 BGI 规则。

用户描述目标即可。能从资源、接口或日志取得的信息自行读取；已有证据直接复用，不反复发现同一接口。不要把某个界面命令不可调用当成整个业务不可执行，也不要把请求已发出当成目标完成。

## 选择下一步

- “执行／运行／采集某个材料、路线或脚本”：直接 `bgi.user.resolve`，按返回的资源类型和 verdict 准备、运行；本机未安装不等于仓库不存在。
- 脚本参数、README 或源码含义：本机用 `bgi.user.inspect_script/read`，仓库用 `bgi.repo.search/read`；按 manifest 的 settings_ui/main 继续读取。用 `skills.read` 加载 `bgi-javascript`。
- 编写／修改 JS，或 OCR、图像、宿主 API：加载 `bgi-javascript` 的 writing.md，通过 api.read 的 bgi.js_api.search/read 查询实际引擎契约。
- 当前可见编辑器、窗口或列表设置：稳定入口优先；需要原生交互时读取 bgi-operator 的 native-ui.md，使用 bgi.ui.read/write/invoke/respond/operation 完成输入、保存与核验，不因弹窗而删掉功能。
- 删除配置组：定位精确 name/sha256，describe/invoke `bgi.delete_script_group`；不要求界面选中、不改成禁用。
- 停止任务：关联原 Job；无 Job 时 `bgi.stop_current_task`。取消请求或 timeout 都不表示已经停止。
- 其他功能或对象不明确：用 `bgi.feature.search` 搜索用户目标，再 `bgi.feature.read` 读取最相关条目。索引覆盖源码功能，但当前能否调用仍以 `bgi.api.describe` 为准。
- 询问总体能力：以“功能目录”搜索流程摘要并按 nextOffset 分页；不要读取全部单项卡片。

详细说明只在当前任务需要时读取：功能卡会给出 references（skill/name/path）、执行步骤、分支和验证证据。不要加载全部卡片、整份功能清单或无关参考资料。

权限由运行时处理；明确授权的目标不重复问许可。缺少真正必要的选择或参数才询问。只报告已核验的结果和一个真实阻塞项，普通回复不讲程序集、反射或接口实现。
