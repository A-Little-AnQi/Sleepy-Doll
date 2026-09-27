---
name: bgi-assistant
description: BGI 插件的简短目标路由。用于 BetterGI 的脚本、路线、配置、任务、设置、页面和故障；具体链路按功能索引读取。
tags: BetterGI, BGI, 原神, 游戏脚本
alwaysLoad: true
---

# BGI 目标入口

此技能只适用于 BGI 目标；产品是通用 Agent，其他工具及普通对话不套用 BGI 规则。

复用已有证据，自行读取现场信息。界面不可调用不等于业务不可执行，发出请求不等于完成。

调用优先级：直接资源／设置／领域数据接口 → 已有业务脚本或任务 → ViewModel／原生界面兜底。用户要删除、修改或运行一个目标，并不意味着要打开界面；真实路径、名称和版本足够时直接操作数据。调用前读取当前直接接口契约。只有用户明确要打开／操作界面，或直接接口无法完成而原生界面可以完成时才使用 UI。

## 选择下一步

- “执行／运行／采集某个材料、路线或脚本”：直接 `bgi.user.resolve`，按返回的资源类型和 verdict 准备、运行；本机未安装不等于仓库不存在。
- 脚本参数、README 或源码含义：本机用 `bgi.user.inspect_script/read`，仓库用 `bgi.repo.search/read`；按 manifest 的 settings_ui/main 继续读取。用 `skills.read` 加载 `bgi-javascript`。
- 编写／修改 JS，或 OCR、图像、宿主 API：加载 `bgi-javascript` 的 writing.md，通过 api.read 的 bgi.js_api.search/read 查询实际引擎契约。
- 当前可见编辑器、窗口或列表设置：稳定入口优先；需要原生交互时读取 bgi-operator 的 native-ui.md，使用 bgi.ui.read/write/invoke/respond/operation 完成输入、保存与核验，不因弹窗而删掉功能。
- 删除配置组：定位精确 name/sha256，describe/invoke `bgi.delete_script_group`；不要求界面选中、不改成禁用。
- 删除路线／JS／键鼠资源：已有路径用 inspect_local_resource → delete_local_resource；未知路径用 user.list 或组的 referencedResources 定位。不要用运行用的 resolve，也不操作 UI。只删路线时保留组，仍被引用先说明冲突；同时删组和资源时先删组。接口缺失检查桥版本，不无限试 UI。
- 停止任务：完整等待 Job 用原 Job；仅启动 Job 已结束时用 `bgi.stop_current_task`，不能当作脚本结束。核验停止。
- 其他功能或对象不明确：用 `bgi.feature.search` 搜索用户目标，再 `bgi.feature.read` 读取最相关条目。索引覆盖源码功能，但当前能否调用仍以 `bgi.api.describe` 为准。
- 用户指定快捷任务：读取 references/quick-tasks.md，保存指定入口，不自动生成。
- 总体能力：以“功能目录”搜索流程摘要并按 nextOffset 分页。

配置组默认 waitForCompletion=false，交接后结束，不查状态或重跑；仅明确等结果或后续步骤时 true。总结只说目标与启动结果，不复述字段名或主动提守护。启动不是业务完成。未就绪用 start_game，再 wait_ready 阻塞等待（默认120秒），超时报阻碍，不让模型轮询。快捷入口用 true 跟踪卡片，执行器无模型。

按功能卡 references（skill/name/path）读取具体链路，不加载无关卡片。

权限由运行时处理；明确授权的目标不重复问许可。缺少真正必要的选择或参数才询问。只报告已核验的结果和一个真实阻塞项，普通回复不讲程序集、反射或接口实现。
