# BGI 全部接口审阅表

审阅范围是 Sleepy Doll 的 BGI 插件与桥。BGI 本体源码保持原样；删除的是插件补造的旧实现及公开接口，宿主原有用户配置文件不迁移、不删字段。

源码版本：`7e02dc8cee57f264aa8d5f3efe4f18a956c39611`。真实类型检查：`E:\BetterGIProject\better-genshin-impact\BetterGenshinImpact\bin\x64\Debug\net8.0-windows10.0.22621.0\BetterGI.dll`。

公开 Agent 工具 20 个、稳定桥接口 36 个、动态命令 263 个。源码命令 319 项逐项审查后，移除 58 项；622 个设置叶节点移除 2 项。继承别名和源码清单的数量不同，真实程序集动态命令由上一阶段的 343 项减少为 263 项。

完整 Schema、参数、前置条件、副作用与核验规则见 [结构化接口审阅表](interface-review.json)。每一项命令和设置均在下面列出；运行时仍以当前实例的 api.describe 为准。没有执行全部游戏任务、通知测试、升级、账号操作或资源删除，不能把源码审查称为实机全通过。

## 删除规则与替代链路

- `[Obsolete]`、空方法、仅注释占位、已无界面绑定且无源码调用的旧入口不发布。
- 生命周期／鼠标／下拉框等内部输入事件不作为独立业务 API。
- 没有输入和收尾适配的模态编辑窗口／ContentDialog 不发布；反射发现不代表能自动执行。
- 未进入源码审查记录的宿主新成员不自动发布，避免升级后又复活旧功能。
- 旧跟踪按钮和旧教程入口已删除；正常地图追踪走 resolve → repo/subscription → prepare_pathing_group → run_script_group。
- 空测试按钮删除；图像测试保留当前原生 start_capture_test。旧表单删除；设置、资源编辑与当前窗口的真实业务入口保留。
- 设置的只读限制与功能弃用分开：缺 setter／安全写入 Schema 的有效设置保留读取，不伪装为可写。

## Agent 可见工具

| 工具 | 用途 | 定义 |
|---|---|---|
| `bgi.state.get` | 读取一次 BetterGI、截图器、游戏窗口和任务锁状态。仅在准备执行、执行后核验或排障时使用；查询和编辑 User 文件不需要先调它。 | src/bridge/mod.rs:353 |
| `bgi.capability.search` | 搜索已安装扩展登记的语义能力和资源。它不包含 BetterGI 自身接口，也不用于查用户配置；没有扩展时调用一次空结果即结束。 | src/bridge/mod.rs:363 |
| `bgi.capability.describe` | 读取已由 capability.search 找到的扩展能力契约。BetterGI 原生接口使用 bgi.api.describe。 | src/bridge/mod.rs:373 |
| `bgi.capability.invoke` | 执行已读取契约的扩展能力。只使用 capability.search 返回的精确 ID，并继续核验 Job 结果。 | src/bridge/mod.rs:383 |
| `bgi.job.get` | 仅在已有 Job ID、但原调用没有返回终态证据时查询 Job。bgi.api.invoke 已返回 completed/failed/cancelled 时不要重复查询。verification 才表示业务是否已核验。 | src/bridge/mod.rs:395 |
| `bgi.job.cancel` | 请求取消指定 Job。cancellationRequested 不等于宿主任务已经停止，继续查询同一 Job。 | src/bridge/mod.rs:405 |
| `bgi.feature.search` | 离线检索 BGI 插件的全量功能与流程索引；只返回少量摘要。用户目标或设置/命令不明确时用它，明确运行资源仍先 user.resolve。静态命中不代表现场可调用；用 feature.read 读取单项链路，再 describe 当前接口。 | src/bridge/mod.rs:417 |
| `bgi.feature.read` | 读取 feature.search 返回的一个精确 ID，取得输入来源、步骤、分支、验证方法与相关参考资料。不读取全量手册，不证明当前接口可调用；只按当前目标继续读取引用和 describe。 | src/bridge/mod.rs:421 |
| `bgi.repo.search` | 搜索中央仓库全部资源，默认 all；JS 参数用 js，采集/地图追踪必须用 pathing。地图追踪返回完整目标目录和作者包、requirements，不返回散落叶子供拼接。分类无命中不等于全仓库没有。用精确路径阅读、订阅并准备运行，不要扫描桥程序集。 | src/bridge/mod.rs:425 |
| `bgi.repo.read` | 直接从中央 Git 仓库读取未订阅的 settings.json、README、manifest、入口 JS 与引用模块。path 使用搜索返回的 js/... 路径再拼文件名，不加 repo/，不猜已安装目录。contains 定位字段或函数及上下文；返回行号、SHA-256、truncated 和 nextLine，截断时继续分页。阅读不会运行脚本或修改订阅。 | src/bridge/mod.rs:429 |
| `bgi.api.search` | 在当前 BetterGI 宿主中发现设置或动作。它不搜索配置组、路线、脚本等用户资源。group 必须来自目录实际返回的分组；用一个业务词查询，一次零结果后检查证据源。 | src/bridge/mod.rs:433 |
| `bgi.api.describe` | 读取一个精确 methodId 的用途、参数、前置条件、副作用、结果判定和回退边界。每个候选读一次；callable=false 时以 unavailableReason 为最终结论。 | src/bridge/mod.rs:437 |
| `bgi.api.read` | 调用刚通过 api.describe 确认的只读接口。用于读取宿主当前设置或诊断；不用于读取 User 文件。arguments 必须满足该接口 inputSchema。 | src/bridge/mod.rs:441 |
| `bgi.api.invoke` | 调用刚通过 api.describe 确认的写接口。运行时显示审批并跟踪 Job 到终态；返回 outcome/evidence 后直接按契约核验，不重复调用 job.get。不能把 completed 或处理器返回自动当成业务成功。 | src/bridge/mod.rs:444 |
| `bgi.user.list` | 列出 BetterGI User 目录下指定位置的一层真实文件和目录。jsonKeys 可在同一次调用中投影每个 JSON 文件的顶层字段，避免逐文件读取；不用于发现宿主接口。 | src/bridge/mod.rs:447 |
| `bgi.user.read` | 读取 BetterGI User 目录中的一个文本文件。JSON 可用 keys 投影所需顶层字段；查询配置组通常读取 name、index、projects，只有修改整个文件时才读取全文。多个独立文件应在同一轮并行读取。 | src/bridge/mod.rs:498 |
| `bgi.user.inspect_script` | 一次读取一个已知 JS 脚本包的 manifest、README、settings 参数定义、目录条目和 settings 下的账户配置。folderName 必须来自配置组任务或 User/JsScript 目录；不要再分别 list/read 同一脚本。 | src/bridge/mod.rs:542 |
| `bgi.user.resolve` | 查找用户要求运行的实际资源：先匹配本机配置组、核验引用和地图追踪父目录；未命中自动搜索当前中央仓库全部分类。不要扫描路线 JSON 或先刷新仓库。run 直接运行；repair 补 missing；create 准备本地父目录；resourceFound 按 repository 候选订阅、配置并继续运行；lookupFailed 是查询失败，不能称资源不存在；notFound 才是本机和当前全仓索引均未命中。 | src/bridge/mod.rs:597 |
| `bgi.user.write` | 原子创建或替换 BetterGI User 资源文件。已有文件必须提交 user.read 返回的 sha256，写入前校验 JSON、比较版本并保留独立备份；写后自动核验。不得修改 User/config.json。 | src/bridge/mod.rs:613 |
| `bgi.user.restore` | 把 bgi.user.write 返回的独立备份恢复到原资源。恢复前比较当前 sha256，避免覆盖写入后的其他修改；恢复本身也为当前版本创建新备份并核验。 | src/bridge/mod.rs:676 |

## 稳定桥接口：全部保留

| 接口 | 用途 | 效果 | 必需参数 | 核验 |
|---|---|---|---|---|
| bgi.commit_settings | 提交 preview_settings 生成的计划，创建恢复记录并原子写入。 | configurationWrite | planId | 回读目标 path，并保留 changeId。 |
| bgi.create_command_argument | 仅构造当前命令声明的参数类型；不能指定任意 CLR 类型。泛型从真实 contextId 取得具体类型，已有资源仍使用引用。 | hostCommand | command, arguments | 继续绑定精确 contextId/objectId，执行后核对业务证据。 |
| bgi.create_command_target | 显式构造未注册的宿主对象；服务依赖从原容器取得，未知／歧义参数拒绝。目录查询不偷偷实例化。 | hostCommand | command, arguments | 继续绑定精确 contextId/objectId，执行后核对业务证据。 |
| bgi.delete_script_group | 按精确名称删除一个调度器配置组，不依赖界面当前选择；保留脚本、地图追踪路线和订阅。 | hostCommand | groupName, expectedSha256 | 返回已验证结果即完成，不再查生命周期、导航或游戏状态。 |
| bgi.exit_game | 结束原神游戏进程。等价于 BetterGI 一条龙收尾的退出方式：先请求正常关闭，5 秒未退再结束进程。任务收尾、切换账号前使用。 | hostCommand | — | 调用后读一次 bgi.get_status：gameHandle 变 0 或 captureReady 回落即已退出；不要反复调用。 |
| bgi.get_script_errors | 从宿主日志里提取脚本执行失败：报错原文、JS 层错误、出错的脚本名、涉及的用户文件路径和宿主源位置。 | readOnly | — | 按 script 读取 User/JsScript 下的源码核对 error 指出的位置；需要完整过程时用 bgi.read_host_log 按同一 thread 读取那次运行。 |
| bgi.get_setting | 读取一个精确 path 的当前值、Schema、写入限制和并发版本。 | readOnly | path | 保存 valueVersion；按 valueSchema 生成新值。 |
| bgi.get_setting_change | 读取一个 changeId 的脱敏差异、状态和恢复记录。 | readOnly | changeId | 核对 changeId、状态和目标 path。 |
| bgi.get_status | 读取截图器、游戏句柄、窗口前台状态和独立任务锁。 | readOnly | — | 检查 observedAt；执行结果按目标接口另行核验。 |
| bgi.invoke_command | 执行已确定且当前可调用的界面命令；异步命令等待处理器返回。 | hostCommand | command | 按具体命令契约读取状态或资源；超时后不盲目重发。 |
| bgi.list_command_targets | 反射当前窗口、数据上下文和宿主集合；返回真实 objectId 与构造参数，不靠界面当前选中猜目标。 | readOnly | command | 继续绑定精确 contextId/objectId，执行后核对业务证据。 |
| bgi.list_commands | 列出低层 WPF 命令的目标、参数、可调用状态和影响。命令通常作用于界面当前选择。 | readOnly | — | 选择一个命令后读取其独立契约。 |
| bgi.list_pages | 列出 BetterGI 主窗口的明确页面标识与名称。 | readOnly | — | 使用返回的页面标识；不查生命周期命令，不猜类名。 |
| bgi.list_setting_changes | 列出桥创建的配置事务和命令前检查点，不返回敏感原文。 | readOnly | — | 对目标 changeId 调用 bgi.get_setting_change。 |
| bgi.list_setting_sections | 列出 AllConfig 的设置分区及可读、可写、敏感字段数量。 | readOnly | — | 选定分区后调用 bgi.search_settings。 |
| bgi.open_page | 按明确页面标识打开 BetterGI 页面并核验导航选择。 | hostCommand | page | 使用返回的页面标识；不查生命周期命令，不猜类名。 |
| bgi.ping | 检查当前 token 能否访问注入桥。不读取游戏画面。 | readOnly | — | 需要执行环境时读取 bgi.get_status。 |
| bgi.prepare_js_group | 用已安装 JS 脚本的宿主构造器与真实设置定义建立单任务配置组，不执行脚本。 | hostCommand | folderName | prepared 只表示配置准备完成，用户要求运行时继续执行。 |
| bgi.prepare_pathing_group | 使用所选父目录下全部路线建立配置组，保留原文件名和目录；配置使用宿主默认值，不拼接游戏设置。 | hostCommand | path | 随后先确认游戏就绪，再直接 describe/invoke bgi.run_script_group；不要搜索 ViewModel 命令或程序集。 |
| bgi.preview_settings | 校验 1–20 项变更并生成绑定当前内存和磁盘版本的计划，不写配置。 | readOnly | changes | 差异与用户目标一致后提交同一 planId。 |
| bgi.probe | 检查桥依赖的 BetterGI 类型、DI 容器和 UI 调度器。 | readOnly | — | 按目标接口所需依赖判断；不把探测成功当作任务成功。 |
| bgi.read_host_log | 读取 BetterGI 自己写的按天日志。脚本的 log() 输出、宿主异常和脚本异常都在里面，不需要用户复制粘贴。 | readOnly | — | 改变 date、level、logger、contains 或 thread 重新读取；重复同一条查询不算新证据。 |
| bgi.read_script_repository_file | 直接读取中央 Git 仓库中的 README、settings、manifest、JS 与模块源码；未订阅文件同样可读，不检出、不订阅、不执行。支持行号分页和关键词上下文。 | readOnly | path | 脚本参数以 settings 与 JS 的实际读取、分支和调用为准；已安装版本可能与中央仓库版本不同。 |
| bgi.release_command_target | 结束使用后释放由桥创建的对象，用户已有窗口和对象不能通过此入口释放。 | hostCommand | objectId | 继续绑定精确 contextId/objectId，执行后核对业务证据。 |
| bgi.rollback_settings | 仅在目标字段仍等于该事务提交值时恢复旧值；保留其他后续修改。 | configurationWrite | changeId | 回读目标 path。CONFIG_CONFLICT 表示未覆盖后续修改。 |
| bgi.run_one_dragon | 运行 BetterGI 的「一条龙」日常流程（领邮件、合成树脂、自动秘境、首领讨伐、幽境危战、地脉花、每日奖励、尘歌壶等，按该配置启用的任务执行）。执行完按配置可能自动退出游戏。 | gameWrite | — | Job 到终态后读一次 bgi.read_host_log（过滤「一条龙」或 ERR）确认各任务结果与是否已退出游戏；不要反复轮询。 |
| bgi.run_script_group | 调用 BetterGI 自带的按名称执行入口，运行指定配置组中的已启用任务。无需用户预先在脚本调度页选中目标。 | gameWrite | groupName | Job 到终态后读一次 bgi.read_host_log（过滤「执行结束」或 ERR）即可收尾：无错误即报告完成，有错误列出到场的问题；不要反复轮询或多方取证。 |
| bgi.search_script_repository | 按脚本标题或功能词搜索当前中央仓库索引，包括未订阅脚本；返回精确内容路径，不搜索宿主设置。 | readOnly | query | 脚本参数以 settings 与 JS 的实际读取、分支和调用为准；已安装版本可能与中央仓库版本不同。 |
| bgi.search_settings | 按业务词、路径、分区和值类型查找 AllConfig 设置；不搜索配置组、脚本或路线。 | readOnly | — | 只对选中的 path 再调用 bgi.get_setting。 |
| bgi.set_game_resolution | 把 BetterGI 当前要启动的原神显示记录改为目标值。仅写入注册表不保证启动后的实际窗口尺寸；远程桌面会话太小时游戏会覆盖它。 | hostCommand | width, height | 下次启动后用 bgi.get_status 的 gameResolution 确认目标分辨率已生效。 |
| bgi.set_setting | 使用最新 expectedVersion 修改一个设置，执行完整备份、提交和核验。 | configurationWrite | path, value, expectedVersion | 回读目标 path；保留 changeId。 |
| bgi.start_game | 让 BetterGI 按「联动启动」的配置拉起原神并开始截图，随后返回最新状态。 | hostCommand | — | 用 bgi.get_status 等到 ready=true，再执行原任务。 |
| bgi.stop_current_task | 向 BetterGI 当前独立任务的取消上下文发送停止请求，并检查宿主任务锁是否释放。 | hostCommand | — | 有桥 Job 时继续跟踪原 Job；只有请求发出或 cancellationRequested 不能证明停止。 |
| bgi.subscribe_script_resources | 按精确中央仓库路径导入路线或脚本，调用 BetterGI 的订阅器，不手工重写 JSON。 | hostCommand | paths | 订阅完成后准备配置组并运行；不把准备当作执行完成。 |
| bgi.update_subscribed_scripts | 调用 BetterGI 自带仓库更新器。可只刷新中央仓库、只更新指定的已订阅路径，或更新全部当前订阅；不需要打开脚本仓库窗口。 | hostCommand | mode | selected/all 完成后回读目标 manifest、版本或关键文件；repositoryOnly 可结合 repositoryChanged 与更新时间判断。 |
| bgi.wait_ready | 在游戏启动后等待最多 20 秒，返回最新状态；发现非 16:9 时不继续等主界面。 | readOnly | — | 以 ready 与 runtime.gameResolution 核对；timeout 后可按需要再次等待，不要用 PowerShell 睡眠。 |

## 已删除的源码命令

下面是删除证据，不是功能菜单，也不进入 Agent 功能索引。

| 原入口 | 删除原因 | 源码 |
|---|---|---|
| `cmd.drawer.toggle_drawer` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:55 |
| `cmd.check_update_window.update` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:139 |
| `cmd.main_window.activated` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:164 |
| `cmd.main_window.closing` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:309 |
| `cmd.main_window.loaded` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:379 |
| `cmd.mask_window.loaded` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:179 |
| `cmd.mask_window.overlay_layout_committed` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:467 |
| `cmd.mask_window.point_click` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:921 |
| `cmd.mask_window.point_hover` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:951 |
| `cmd.mask_window.point_right_click` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:944 |
| `cmd.mask_window.window_size_changed` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:513 |
| `cmd.common_settings_page.open_about_window` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:376 |
| `cmd.common_settings_page.open_key_bindings_window` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:384 |
| `cmd.common_settings_page.switch_mask_enabled` | 空实现或仅注释占位，已移除而不补造业务 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:284 |
| `cmd.common_settings_page.switch_taken_screenshot_enabled` | 空实现或仅注释占位，已移除而不补造业务 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:309 |
| `cmd.home_page.capture_mode_drop_down_changed` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:206 |
| `cmd.home_page.change_web_banner_image` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:755 |
| `cmd.home_page.loaded` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:153 |
| `cmd.home_page.open_game_command_line_document` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:524 |
| `cmd.home_page.open_hardware_acceleration_settings` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:575 |
| `cmd.home_page.test` | 空实现或仅注释占位，已移除而不补造业务 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:418 |
| `cmd.js_list.delete_script` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:139 |
| `cmd.map_pathing.delete` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:199 |
| `cmd.music_page.begin_seek` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:542 |
| `cmd.music_page.open_settings` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:462 |
| `cmd.music_page.update_track_selection` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:581 |
| `cmd.one_dragon_flow.config_drop_down_changed` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:402 |
| `cmd.one_dragon_flow.copy_task` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:747 |
| `cmd.one_dragon_flow.delete_task` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:769 |
| `cmd.one_dragon_flow.loaded` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:534 |
| `cmd.one_dragon_flow.save_action_config` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:440 |
| `cmd.one_dragon_flow.set_task_as_next` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:779 |
| `cmd.one_dragon_flow.strategy_drop_down_opened` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:447 |
| `cmd.script_control.continue_multi_script_group` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:2120 |
| `cmd.script_control.edit_js_script_settings` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1628 |
| `cmd.script_control.import_script_group` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1943 |
| `cmd.script_control.open_log_parse` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:138 |
| `cmd.script_control.open_script_group_settings` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1995 |
| `cmd.script_control.start_multi_script_group` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:2258 |
| `cmd.task_settings_page.go_to_auto_track_path_url` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:659 |
| `cmd.task_settings_page.go_to_auto_track_url` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:624 |
| `cmd.task_settings_page.go_to_torch_previous_versions` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:774 |
| `cmd.task_settings_page.open_artifact_salvage_test_ocrwindow` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:807 |
| `cmd.task_settings_page.strategy_drop_down_opened` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:389 |
| `cmd.task_settings_page.switch_auto_track` | 源码标记 Obsolete，已移除公共入口 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:595 |
| `cmd.task_settings_page.switch_auto_track_path` | 源码标记 Obsolete，已移除公共入口 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:630 |
| `cmd.trigger_settings_page.edit_skill_cd_config` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:128 |
| `cmd.trigger_settings_page.open_blacklist_mode_config` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:74 |
| `cmd.trigger_settings_page.open_whitelist_mode_config` | 模态窗口／ContentDialog 尚无完整输入与收尾适配，不作为可调用接口 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:84 |
| `cmd.trigger_settings_page.remove_skill_cd_rule` | 没有有效界面绑定或源码调用，未作为当前产品功能发布 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:116 |
| `cmd.auto_fight.strategy_drop_down_opened` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:97 |
| `cmd.pathing_config.closing` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:57 |
| `cmd.script_group_config.strategy_drop_down_opened` | WPF 生命周期／控件输入事件，不是独立业务接口 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:110 |
| `cmd.form.add` | 旧表单类型已无原生界面／业务调用，使用当前黑白名单窗口或资源接口 | BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:29 |
| `cmd.form.edit_at` | 旧表单类型已无原生界面／业务调用，使用当前黑白名单窗口或资源接口 | BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:41 |
| `cmd.form.remove_at` | 旧表单类型已无原生界面／业务调用，使用当前黑白名单窗口或资源接口 | BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:35 |
| `cmd.form.save` | 旧表单类型已无原生界面／业务调用，使用当前黑白名单窗口或资源接口 | BetterGenshinImpact/ViewModel/Windows/FormViewModel.cs:52 |
| `cmd.map_pathing_dev.drop_down_changed` | 空实现或仅注释占位，已移除而不补造业务 | BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:26 |

## 公开动态命令：全部列出

每项保留真实上下文、参数和选择依赖。复杂对象使用真实 objectId；执行结果仍按契约核验。

| 接口 | 用途 | 参数／依赖 | 核验 | 源码 |
|---|---|---|---|---|
| cmd.auto_fight.open_fight_folder | 在自动战斗中打开战斗策略目录。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:120 |
| cmd.auto_fight.open_local_script_repo | 在自动战斗中打开本地脚本仓库。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/AutoFightViewModel.cs:113 |
| cmd.auto_pick_blacklist_config.cancel | 拾取黑名单：取消 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/AutoPickConfigWindowViewModelBase.cs:26 |
| cmd.auto_pick_blacklist_config.save | 将拾取黑名单当前编辑内容保存到 BetterGI 管理的数据位置；可能覆盖原文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/AutoPickBlacklistConfigViewModel.cs:33 |
| cmd.auto_pick_config_window_view_model_base.cancel | AutoPickConfigWindowBase：取消 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/AutoPickConfigWindowViewModelBase.cs:26 |
| cmd.auto_pick_whitelist_config.cancel | 拾取白名单：取消 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/AutoPickConfigWindowViewModelBase.cs:26 |
| cmd.auto_pick_whitelist_config.save | 将拾取白名单当前编辑内容保存到 BetterGI 管理的数据位置；可能覆盖原文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/AutoPickWhitelistConfigViewModel.cs:29 |
| cmd.check_update_window.background_update | CheckUpdateWindow：后台更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:121 |
| cmd.check_update_window.cancel | CheckUpdateWindow：取消 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:244 |
| cmd.check_update_window.edit_cdk | CheckUpdateWindow：Mirror酱服务 💰：修改CDK | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:253 |
| cmd.check_update_window.ignore | CheckUpdateWindow：不再提示 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:235 |
| cmd.check_update_window.other_update | CheckUpdateWindow：手动下载 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:130 |
| cmd.check_update_window.update_from_git_host_platform | CheckUpdateWindow：开源渠道：立即更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:148 |
| cmd.check_update_window.update_from_mirror_chyan | CheckUpdateWindow：Mirror酱服务 💰：立即更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:181 |
| cmd.check_update_window.update_from_steambird | CheckUpdateWindow：Steambird 服务 🆓：立即更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/CheckUpdateWindow.xaml.cs:168 |
| cmd.child_session_window.hide | 隐藏桌面分身窗口，不等于退出或停止后台任务。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:247 |
| cmd.child_session_window.launch_better_gi | 桌面分身：运行 BetterGI | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:259 |
| cmd.child_session_window.launch_executable | 桌面分身：以管理员权限启动… | 无参数; contextId; dialogInput 必需 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:421 |
| cmd.child_session_window.open_desktop_help | 桌面分身：查看帮助文档 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:454 |
| cmd.child_session_window.select_default_resolution | 桌面分身：1920 × 1080 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:265 |
| cmd.child_session_window.show_desktop | 桌面分身：发送 Win+D 显示桌面 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:409 |
| cmd.child_session_window.show_task_view | 桌面分身：发送 Win+Tab 切换窗口 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:415 |
| cmd.child_session_window.start | 桌面分身：启动 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:213 |
| cmd.child_session_window.switch_window | 桌面分身：切换窗口 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:253 |
| cmd.child_session_window.toggle_audio_muted | 切换桌面分身的静音状态。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:372 |
| cmd.child_session_window.toggle_game_mouse_mode | 切换桌面分身的游戏鼠标交互模式。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:353 |
| cmd.child_session_window.toggle_keep_aspect_ratio | 桌面分身：保持宽高比 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:306 |
| cmd.child_session_window.toggle_send_system_shortcuts_to_remote | 桌面分身：系统组合键发送到桌面分身 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:317 |
| cmd.child_session_window.toggle_small_window_mode | 切换桌面分身的小窗口显示模式。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:299 |
| cmd.child_session_window.toggle_topmost | 切换桌面分身窗口置顶。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:443 |
| cmd.child_session_window.use_adaptive | 桌面分身：自适应 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:271 |
| cmd.child_session_window.use_one_to_one | 桌面分身：1 : 1 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/ChildSessionWindowViewModel.cs:285 |
| cmd.common_settings_page.check_update | 检查 BetterGI 更新信息，可能访问网络并打开更新界面。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:393 |
| cmd.common_settings_page.check_update_alpha | 通用设置：版本更新：检查更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:403 |
| cmd.common_settings_page.clear_main_background_image | 通用设置：主窗口自定义背景：清除 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:491 |
| cmd.common_settings_page.game_lang_selection_changed | 卸载当前 OCR 实例，使下一次识别按已经保存的游戏语言配置重新创建模型；本命令自身不修改语言设置。 | System.Collections.Generic.KeyValuePair`2[[System.String, System.Private.CoreLib, Version=8.0.0.0, Culture=neutral, PublicKeyToken=7cec85d7bea7798e],[System.String, System.Private.CoreLib, Version=8.0.0.0, Culture=neutral, PublicKeyToken=7cec85d7bea7798e]]; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:439 |
| cmd.common_settings_page.go_to_folder | 通用设置：开发者功能：log/screenshot | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:314 |
| cmd.common_settings_page.go_to_hot_key_page | 通用设置：开发者功能：绑定快捷键 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:297 |
| cmd.common_settings_page.go_to_log_folder | 通用设置：显示的指标：打开日志目录 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:338 |
| cmd.common_settings_page.go_to_reward_recognition_folder | 通用设置：开发者功能：log/RewardRecognition | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:326 |
| cmd.common_settings_page.import_local_scripts_repo_zip | 通用设置：选择zip文件导入 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:350 |
| cmd.common_settings_page.open_custom_html_mask_editor | 通用设置：显示的指标：打开编辑器 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:252 |
| cmd.common_settings_page.open_custom_html_mask_folder | 通用设置：显示的指标：打开目录 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:258 |
| cmd.common_settings_page.open_recognition_template_editor_from_image | 通用设置：开发者功能：选择图片 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:303 |
| cmd.common_settings_page.paddle_ocr_model_config_changed | 应用 Paddle OCR 模型选择并更新识别配置。 | BetterGenshinImpact.Core.Recognition.PaddleOcrModelConfig; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:445 |
| cmd.common_settings_page.question_button_on_click | 打开内置日志分析网页窗口，显示由 BetterGI 生成的统计内容；不修改游戏状态。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:144 |
| cmd.common_settings_page.refresh_mask_settings | 通知遮罩重新读取设置并计算控件位置。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:227 |
| cmd.common_settings_page.reset_mask_overlay_layout | 恢复遮罩布局默认值，会修改相关配置。 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:265 |
| cmd.common_settings_page.reset_overlay_style | 恢复遮罩样式默认值并刷新显示，会修改相关配置。 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:240 |
| cmd.common_settings_page.select_crosshair_image | 通用设置：启用准星：浏览 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:452 |
| cmd.common_settings_page.select_main_background_image | 通用设置：主窗口自定义背景：选择图片 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:466 |
| cmd.common_settings_page.ui_language_selection_changed | 应用软件界面语言选择。 | System.Object; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/CommonSettingsPageViewModel.cs:426 |
| cmd.custom_html_mask_editor.close | 在HTML 遮罩编辑器中关闭。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:129 |
| cmd.custom_html_mask_editor.open_custom_html_mask_folder | 在HTML 遮罩编辑器中打开HTML 遮罩目录。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:99 |
| cmd.custom_html_mask_editor.restore_default_custom_html_mask | 恢复默认 HTML 遮罩内容，可能覆盖自定义文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:82 |
| cmd.custom_html_mask_editor.save_custom_html_mask | 保存 HTML 遮罩编辑内容到文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:45 |
| cmd.custom_html_mask_editor.toggle_custom_html_mask_preview | 切换 HTML 遮罩预览。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/CustomHtmlMaskEditorViewModel.cs:61 |
| cmd.drawer.close_drawer | 在详情抽屉中关闭详情抽屉。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:49 |
| cmd.drawer.open_drawer | 在详情抽屉中打开详情抽屉。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | System.Object; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Controls/Drawer/DrawerViewModel.cs:42 |
| cmd.feed_window.auto_redeem_item | 动态订阅：一键兑换 | BetterGenshinImpact.ViewModel.Windows.FeedItem; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:137 |
| cmd.feed_window.copy_item_codes | 动态订阅：复制 | BetterGenshinImpact.ViewModel.Windows.FeedItem; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:118 |
| cmd.feed_window.get_live_redeem_codes | 动态订阅：实时获取前瞻兑换码 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:44 |
| cmd.feed_window.refresh | 重新加载动态订阅的数据来源并更新界面列表；完成后应重新读取列表确认变化。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:98 |
| cmd.feed_window.select_cn_server | 动态订阅：国服 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:104 |
| cmd.feed_window.select_global_server | 动态订阅：国际服 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/FeedWindowViewModel.cs:111 |
| cmd.hardware_acceleration.open_cache_folder | 硬件加速：推理设备配置：打开缓存目录 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/HardwareAccelerationViewModel.cs:28 |
| cmd.home_page.change_banner_image | 启动页：更换背景图片 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:709 |
| cmd.home_page.go_to_wiki_url | 在浏览器打开Wiki的使用文档，不执行游戏操作。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:412 |
| cmd.home_page.manual_pick_window | 打开目标窗口选择器；用户选定窗口后立即将它设为捕获目标并启动 BetterGI 截图器。用户取消选择时不修改当前目标。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:243 |
| cmd.home_page.open_child_session_window | 启动页：BetterGI 桌面分身：打开 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:147 |
| cmd.home_page.open_display_advanced_graphics_settings | 启动页：BetterGI 截图器，启动！：手动设置 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:261 |
| cmd.home_page.refresh_web_banner_image | 启动页：刷新网络图片 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:782 |
| cmd.home_page.reset_banner_image | 启动页：恢复默认图片 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:807 |
| cmd.home_page.select_install_path | 启动页：同时启动原神：浏览 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:455 |
| cmd.home_page.start_capture_test | 在启动页中启动Capture Test。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:223 |
| cmd.home_page.start_trigger | 启动截图器、识别调度和已启用触发器；要求已选定游戏窗口。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:272 |
| cmd.home_page.stop_trigger | 停止截图调度并请求取消独立任务，同时隐藏相关遮罩。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/HomePageViewModel.cs:371 |
| cmd.hot_key_setting_model.switch_hot_key_type | 宿主反射命令 BetterGenshinImpact.Model.HotKeySettingModel.SwitchHotKeyTypeCommand；需当前上下文、原生参数与操作后的业务证据，不从处理器返回推断用户目标完成。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/Model/HotKeySettingModel.cs:234 |
| cmd.js_list.go_to_js_script_url | 在浏览器打开JsScript的使用文档，不执行游戏操作。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:187 |
| cmd.js_list.open_local_script_repo | JavaScript 脚本：脚本仓库 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:194 |
| cmd.js_list.open_script_detail_drawer | 在JavaScript 脚本中打开脚本详情抽屉。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | System.Object; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:207 |
| cmd.js_list.open_script_project_folder | JavaScript 脚本：打开目录 | BetterGenshinImpact.Core.Script.Project.ScriptProject; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:110 |
| cmd.js_list.open_scripts_folder | JavaScript 脚本：打开脚本目录 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:104 |
| cmd.js_list.refresh | JavaScript 脚本：刷新 | BetterGenshinImpact.Core.Script.Project.ScriptProject; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:133 |
| cmd.js_list.set_right_click_selection | 将右键指向的资源设为当前选择，供后续菜单操作使用。 | System.String; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:201 |
| cmd.js_list.start_run | 运行所选 JavaScript 脚本，可能产生脚本定义的游戏和文件副作用。 | BetterGenshinImpact.Core.Script.Project.ScriptProject; contextId; selection：当前目标; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/JsListViewModel.cs:116 |
| cmd.json_mono.close | 在JSON 编辑器中关闭。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/JsonMonoViewModel.cs:63 |
| cmd.json_mono.save | 将JSON 编辑器当前编辑内容保存到 BetterGI 管理的数据位置；可能覆盖原文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/JsonMonoViewModel.cs:39 |
| cmd.key_bindings_settings_page.fetch_from_registry | KeyBindingsSettingsPage：从注册表中读取按键 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyBindingsSettingsPageViewModel.cs:373 |
| cmd.key_mouse_record_page.delete_script | 键鼠脚本：删除 | BetterGenshinImpact.Model.KeyMouseScriptItem; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:190 |
| cmd.key_mouse_record_page.edit_script | 键鼠脚本：修改名称 | BetterGenshinImpact.Model.KeyMouseScriptItem; contextId; dialogInput 必需 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:146 |
| cmd.key_mouse_record_page.go_to_km_script_url | 在浏览器打开KmScript的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:224 |
| cmd.key_mouse_record_page.open_local_script_repo | 键鼠脚本：脚本仓库 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:230 |
| cmd.key_mouse_record_page.open_script_folder | 键鼠脚本：打开脚本目录 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:140 |
| cmd.key_mouse_record_page.start_play | 回放所选键鼠录制脚本，将向目标窗口发送输入。 | System.String; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:118 |
| cmd.key_mouse_record_page.start_record | 开始录制键盘和鼠标操作，结束后需要保存录制结果。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:81 |
| cmd.key_mouse_record_page.stop_record | 停止当前键鼠录制，把已录制的键鼠事件序列化为 JSON，并在 BetterGI 界面打开保存窗口；未处于录制状态时拒绝执行。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/KeyMouseRecordPageViewModel.cs:96 |
| cmd.macro_settings_page.edit_avatar_macro | 宏设置：一键宏（按角色）：前往设置 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:64 |
| cmd.macro_settings_page.go_to_hot_key_page | 宏设置：一键领取奖励：绑定快捷键 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:58 |
| cmd.macro_settings_page.go_to_one_key_macro_url | 在浏览器打开OneKeyMacro的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MacroSettingsPageViewModel.cs:70 |
| cmd.main_window.dismiss_redeem_code | 只关闭主窗口中的兑换码更新提示卡片；不打开动态窗口、不读取兑换码，也不执行兑换。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:356 |
| cmd.main_window.hide | 隐藏主窗口窗口，不等于退出或停止后台任务。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:200 |
| cmd.main_window.open_feed | 打开动态订阅窗口。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:328 |
| cmd.main_window.switch_backdrop | 切换主窗口背景材质或背景显示模式。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MainWindowViewModel.cs:206 |
| cmd.map_pathing.go_to_pathing_url | 在浏览器打开Pathing的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:187 |
| cmd.map_pathing.open_dev_tools | 地图追踪路线：开发者工具 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:155 |
| cmd.map_pathing.open_local_script_repo | 地图追踪路线：脚本仓库 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:280 |
| cmd.map_pathing.open_pathing_detail | 在地图追踪路线中打开路线详情。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:293 |
| cmd.map_pathing.open_script_project_folder | 地图追踪路线：打开目录 | BetterGenshinImpact.Core.Script.Project.ScriptProject; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:114 |
| cmd.map_pathing.open_scripts_folder | 地图追踪路线：打开任务目录 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:103 |
| cmd.map_pathing.open_settings | 地图追踪路线：设置 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:170 |
| cmd.map_pathing.refresh | 地图追踪路线：刷新 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:193 |
| cmd.map_pathing.set_right_click_selection | 将右键指向的资源设为当前选择，供后续菜单操作使用。 | System.String; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:287 |
| cmd.map_pathing.start | 地图追踪路线：执行任务 | BetterGenshinImpact.Model.FileTreeNode`1[[BetterGenshinImpact.GameTask.AutoPathing.Model.PathingTask, BetterGI, Version=0.65.1.0, Culture=neutral, PublicKeyToken=null]]; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/MapPathingViewModel.cs:125 |
| cmd.map_pathing_dev.open_map_editor | 地图追踪开发面板：录制编辑器 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:47 |
| cmd.map_pathing_dev.open_map_viewer | 地图追踪开发面板：查看实时追踪地图 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/MapPathingDevViewModel.cs:32 |
| cmd.mask_map_point_info_popup.close | 在地图点位信息中关闭。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:281 |
| cmd.mask_map_point_info_popup.open_url | 在地图点位信息中打开Url。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | System.String; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:305 |
| cmd.mask_map_point_info_popup.toggle_hidden | 切换当前地图点位的隐藏状态。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskMapPointInfoPopupViewModel.cs:275 |
| cmd.mask_window.exit_overlay_layout_edit_mode | 退出遮罩布局编辑模式。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:524 |
| cmd.mask_window.hide_all_map_points | 隐藏当前地图点位显示。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:981 |
| cmd.mask_window.reset_selected_map_label_selection | 清空当前地图标签筛选选择。 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:233 |
| cmd.mask_window.select_map_label_category | 选择地图标签分类并更新可选项。 | BetterGenshinImpact.ViewModel.MapLabelCategoryVm; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:203 |
| cmd.mask_window.select_map_label_item | 选择地图标签条目并更新点位显示。 | BetterGenshinImpact.ViewModel.MapLabelItemVm; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:210 |
| cmd.mask_window.show_all_map_points | 显示当前地图范围的全部已加载点位。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:1000 |
| cmd.mask_window.toggle_map_point_hidden | 切换指定地图点位的隐藏状态。 | BetterGenshinImpact.Model.MaskMap.MaskMapPoint; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:962 |
| cmd.mask_window.toggle_map_point_picker | 切换地图点位拾取或选择模式。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/MaskWindowViewModel.cs:187 |
| cmd.music_page.choose_folder | 音乐播放：选择目录 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:331 |
| cmd.music_page.cycle_playback_mode | 轮换音乐顺序播放、循环等播放模式。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:320 |
| cmd.music_page.delete_music_folder | 音乐播放：删除记录 | System.String; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:395 |
| cmd.music_page.initialize | 初始化音乐播放的数据和界面状态；不应在已运行的页面中反复触发。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:219 |
| cmd.music_page.next | 音乐播放：下一首 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:530 |
| cmd.music_page.open_folder | 音乐播放：打开当前目录 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:435 |
| cmd.music_page.play_pause | 音乐播放：播放/暂停 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:472 |
| cmd.music_page.play_selected | 播放当前选中的音乐轨道。 | BetterGenshinImpact.GameTask.Music.Model.PerformanceScore; contextId; selection：当前目标; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:491 |
| cmd.music_page.previous | 音乐播放：上一首 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:536 |
| cmd.music_page.refresh | 重新加载音乐播放的数据来源并更新界面列表；完成后应重新读取列表确认变化。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:455 |
| cmd.music_page.refresh_mapping | 重新加载音乐按键映射。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:570 |
| cmd.music_page.save_profiles | 音乐播放：保存映射 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:588 |
| cmd.music_page.seek | 把音乐播放位置跳转到请求的进度。 | System.Double; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:548 |
| cmd.music_page.select_music_folder | 使用 argument 指定的已有目录切换曲谱库：停止当前播放、保存目录历史与配置、切换目录监听并刷新曲目列表。它不打开目录选择器。 | System.String; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:350 |
| cmd.music_page.stop | 音乐播放：停止 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/MusicPageViewModel.cs:519 |
| cmd.notification_settings_page.bind_group_qq | 连接 QQ 网关并等待机器人加入群聊或收到验证码，成功后自动回填群 OpenID；要求已配置 AppID 与 AppSecret。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:765 |
| cmd.notification_settings_page.bind_qq | 连接 QQ 网关并等待用户发送验证码，成功后自动回填 QQ 用户 OpenID；要求已配置 AppID 与 AppSecret，可等待最多 60 秒。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:600 |
| cmd.notification_settings_page.bind_wechat_clawbot | 启动微信 Clawbot 扫码登录与一次性验证码绑定；成功后一次性保存 bot token、用户 ID 和上下文 token，失败或取消时不应留下混合凭据。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:671 |
| cmd.notification_settings_page.cancel_bind_group_qq | 取消正在等待的 QQ 群绑定 WebSocket 流程；不删除已经保存的群 OpenID。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:833 |
| cmd.notification_settings_page.cancel_bind_qq | 取消正在等待的 QQ 用户绑定 WebSocket 流程；没有绑定流程时不执行其他操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:661 |
| cmd.notification_settings_page.cancel_bind_wechat_clawbot | 取消正在进行的微信 Clawbot 登录或验证码绑定；不主动删除已经保存的凭据。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:754 |
| cmd.notification_settings_page.clear_notification_event_selection | 通知渠道设置：全局通知设置：取消选择 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:158 |
| cmd.notification_settings_page.open_notification_event_document | 通知渠道设置：全局通知设置：打开文档 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:839 |
| cmd.notification_settings_page.select_all_notification_events | 通知渠道设置：全局通知设置：全选 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:152 |
| cmd.notification_settings_page.test_bark_notification | 向已配置的 Bark 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:412 |
| cmd.notification_settings_page.test_ding_ding_webhook_notification | 向已配置的 DingDingWebhook 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:469 |
| cmd.notification_settings_page.test_discord_webhook_notification | 向已配置的 DiscordWebhook 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:488 |
| cmd.notification_settings_page.test_email_notification | 向已配置的 Email 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:393 |
| cmd.notification_settings_page.test_feishu_notification | 向已配置的 Feishu 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:317 |
| cmd.notification_settings_page.test_gotify_notification | 向已配置的 Gotify 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:544 |
| cmd.notification_settings_page.test_meow_notification | 向已配置的 Meow 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:526 |
| cmd.notification_settings_page.test_one_bot_notification | 向已配置的 OneBot 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:336 |
| cmd.notification_settings_page.test_qq_notification | 向已配置的 Qq 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:561 |
| cmd.notification_settings_page.test_server_chan_notification | 向已配置的 ServerChan 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:507 |
| cmd.notification_settings_page.test_telegram_notification | 向已配置的 Telegram 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:431 |
| cmd.notification_settings_page.test_web_socket_notification | 向已配置的 WebSocket 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:374 |
| cmd.notification_settings_page.test_webhook | 向配置的 Webhook 接收方发送测试通知，并更新 BetterGI 的测试状态；可能包含截图，需用户授权接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:279 |
| cmd.notification_settings_page.test_wechat_clawbot_notification | 向已配置的 WechatClawbot 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:578 |
| cmd.notification_settings_page.test_windows_uwp_notification | 向已配置的 WindowsUwp 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:298 |
| cmd.notification_settings_page.test_work_weixin_notification | 向已配置的 WorkWeixin 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:355 |
| cmd.notification_settings_page.test_xxtui_notification | 向已配置的 Xxtui 通知渠道发送测试消息，会产生外部通信；需用户授权该接收方。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/NotificationSettingsPageViewModel.cs:450 |
| cmd.notify_icon.check_update | 检查 BetterGI 更新信息，可能访问网络并打开更新界面。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:71 |
| cmd.notify_icon.exit | 退出 BetterGI，桥随它一起退出；不能把连接断开当成可重试错误。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:56 |
| cmd.notify_icon.open_child_session_window | 在托盘菜单中打开桌面分身窗口。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:34 |
| cmd.notify_icon.show_or_hide | 切换主窗口显示状态，不停止后台任务。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/NotifyIconViewModel.cs:40 |
| cmd.one_dragon_flow.add_config | 在一条龙流程中添加配置。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:859 |
| cmd.one_dragon_flow.add_task_group | 一条龙流程：新增配置 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:429 |
| cmd.one_dragon_flow.clear_next_task_group | 一条龙流程：清除标记 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:841 |
| cmd.one_dragon_flow.delete_config | 在一条龙流程中删除配置。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:882 |
| cmd.one_dragon_flow.delete_config_display_task_list_from_config | 移除当前配置中的任务显示列表条目。 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:385 |
| cmd.one_dragon_flow.delete_task_group | 一条龙流程：删除 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:803 |
| cmd.one_dragon_flow.next_task_group | 一条龙流程：从此执行 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:811 |
| cmd.one_dragon_flow.one_key_execute | 按当前一条龙配置依次执行任务；需先核对任务列表和资源。 | 无参数; contextId; selection：当前目标; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:572 |
| cmd.one_dragon_flow.rename_config | 在一条龙流程中重命名配置。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:953 |
| cmd.one_dragon_flow.reset_auto_boss_completed_run_count | 一条龙流程：自动首领讨伐配置：清空累计 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/OneDragonFlowViewModel.cs:453 |
| cmd.pathing_config.add_avatar_condition_config | 地图追踪条件配置：+ 添加条件 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:39 |
| cmd.pathing_config.add_party_condition_config | 地图追踪条件配置：+ 添加条件 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:24 |
| cmd.pathing_config.remove_avatar_condition_config | 地图追踪条件配置：删除 | System.Object; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:48 |
| cmd.pathing_config.remove_party_condition_config | 地图追踪条件配置：删除 | System.Object; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/View/PathingConfigViewModel.cs:30 |
| cmd.recognition_template_editor.browse_assets_root | 识别模板编辑器：选择… | 无参数; contextId; dialogInput 必需 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:325 |
| cmd.recognition_template_editor.browse_recognition_json | 识别模板编辑器：浏览… | 无参数; contextId; dialogInput 必需 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:297 |
| cmd.recognition_template_editor.cancel | 识别模板编辑器：取消 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:441 |
| cmd.recognition_template_editor.fit_image | 识别模板编辑器：适应窗口 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:345 |
| cmd.recognition_template_editor.normalize_template_file_name | 按识别模板规则规范化文件名。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:447 |
| cmd.recognition_template_editor.save | 将识别模板编辑器当前编辑内容保存到 BetterGI 管理的数据位置；可能覆盖原文件。 | 无参数; contextId | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Windows/RecognitionTemplateEditorViewModel.cs:351 |
| cmd.script_control.add_js_script | 脚本调度：JS脚本 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:729 |
| cmd.script_control.add_km_script | 脚本调度：键鼠脚本 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:840 |
| cmd.script_control.add_next_flag | 脚本调度：下一次任务从此处执行 | BetterGenshinImpact.Core.Script.Group.ScriptGroupProject; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1587 |
| cmd.script_control.add_pathing | 脚本调度：地图追踪任务 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:871 |
| cmd.script_control.add_script_group | 脚本调度：新增组 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:85 |
| cmd.script_control.add_script_group_next_flag | 脚本调度：连续任务从此开始执行 | BetterGenshinImpact.Core.Script.Group.ScriptGroup; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:590 |
| cmd.script_control.add_shell | 脚本调度：Shell | 无参数; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:861 |
| cmd.script_control.clear_tasks | 脚本调度：清空 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:119 |
| cmd.script_control.copy_script_group | 脚本调度：复制组 | BetterGenshinImpact.Core.Script.Group.ScriptGroup; contextId; dialogInput 必需; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:605 |
| cmd.script_control.delete_script | 脚本调度：移除 | BetterGenshinImpact.Core.Script.Group.ScriptGroupProject; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1712 |
| cmd.script_control.delete_script_by_folder | 脚本调度：根据文件夹移除 | BetterGenshinImpact.Core.Script.Group.ScriptGroupProject; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1683 |
| cmd.script_control.delete_script_group | 脚本调度：删除组 | BetterGenshinImpact.Core.Script.Group.ScriptGroup; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:696 |
| cmd.script_control.edit_script_common | 脚本调度：修改通用配置 | BetterGenshinImpact.Core.Script.Group.ScriptGroupProject; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1571 |
| cmd.script_control.export_merger_jsons | 脚本调度：导出根据控制文件修改任务 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:562 |
| cmd.script_control.go_to_script_group_url | 在浏览器打开ScriptGroup的使用文档，不执行游戏操作。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1937 |
| cmd.script_control.open_local_script_repo | 脚本调度：打开脚本仓库 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:498 |
| cmd.script_control.open_script_folder | 脚本调度：打开所在目录 | BetterGenshinImpact.Core.Script.Group.ScriptGroupProject; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1730 |
| cmd.script_control.rename_script_group | 脚本调度：重命名 | BetterGenshinImpact.Core.Script.Group.ScriptGroup; contextId; dialogInput 必需; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:646 |
| cmd.script_control.reverse_task_order | 脚本调度：任务倒序排列 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:553 |
| cmd.script_control.start_script_group | 启动指定或当前选中的脚本组，执行组内配置的任务。 | 无参数; contextId; selection：当前目标; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:1968 |
| cmd.script_control.update_tasks | 脚本调度：根据文件夹更新 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/ScriptControlViewModel.cs:505 |
| cmd.script_group_config.auto_fight_enabled_checked | 勾选脚本组自动战斗后，同时把该脚本组的路线追踪配置 Enabled 设为 true；会修改配置。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:141 |
| cmd.script_group_config.get_execution_order | 根据当前脚本组配置计算或显示任务执行顺序。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:121 |
| cmd.script_group_config.go_to_auto_eat_url | 在浏览器打开自动吃药的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:147 |
| cmd.script_group_config.open_fight_folder | 脚本组配置：战斗配置：打开目录 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:135 |
| cmd.script_group_config.open_local_script_repo | 脚本组配置：战斗配置：脚本仓库 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/View/ScriptGroupConfigViewModel.cs:116 |
| cmd.script_repo_window.import_local_scripts_repo_zip | ScriptRepoWindow：下载文件 导入下载的zip文件 清理临时文件 | 无参数; contextId; dialogInput 必需; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:498 |
| cmd.script_repo_window.open_local_script_repo | 在ScriptRepoWindow中打开本地脚本仓库。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:335 |
| cmd.script_repo_window.reset_repo | ScriptRepoWindow：重置仓库 | 无参数; contextId; selection：当前目标 | 重新读取用途说明所指的配置、集合或文件，确认目标变化。 | BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:409 |
| cmd.script_repo_window.update_repo | ScriptRepoWindow：更新仓库 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:259 |
| cmd.script_repo_window.update_subscribed_scripts | ScriptRepoWindow：一键更新订阅 | 无参数; contextId; selection：当前目标 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/View/Windows/ScriptRepoWindow.xaml.cs:545 |
| cmd.task_settings_page.copy_artifact_salvage_java_script_from_repository | 独立任务设置：自动分解圣遗物：从脚本仓库复制 | 无参数; contextId; dialogInput 必需 | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:814 |
| cmd.task_settings_page.go_to_artifact_salvage_url | 在浏览器打开圣遗物分解的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:801 |
| cmd.task_settings_page.go_to_auto_domain_url | 在浏览器打开自动秘境的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:554 |
| cmd.task_settings_page.go_to_auto_fight_url | 在浏览器打开自动战斗的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:480 |
| cmd.task_settings_page.go_to_auto_fishing_url | 在浏览器打开自动钓鱼的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:768 |
| cmd.task_settings_page.go_to_auto_genius_invokation_url | 在浏览器打开七圣召唤的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:436 |
| cmd.task_settings_page.go_to_auto_ley_line_outcrop_url | 在浏览器打开地脉花的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:582 |
| cmd.task_settings_page.go_to_auto_music_game_url | 在浏览器打开音游的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:674 |
| cmd.task_settings_page.go_to_auto_stygian_onslaught_url | 在浏览器打开幽境危战的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:576 |
| cmd.task_settings_page.go_to_auto_wood_url | 在浏览器打开自动伐木的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:451 |
| cmd.task_settings_page.go_to_get_grid_icons_folder | 独立任务设置：截取物品图标（开发者）：log/gridIcons | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:874 |
| cmd.task_settings_page.go_to_get_grid_icons_url | 在浏览器打开背包图标采集的使用文档，不执行游戏操作。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:886 |
| cmd.task_settings_page.go_to_hot_key_page | 在独立任务设置中转到快捷键设置页。依赖该页面当前选择；如出现文件或输入对话框，需要用户在 BetterGI 界面完成交互。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:395 |
| cmd.task_settings_page.go_to_inventory_count_comparison_folder | 独立任务设置：创建并打开数量 OCR 对比结果根目录。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:922 |
| cmd.task_settings_page.open_fight_folder | 独立任务设置：自动战斗：打开目录 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:589 |
| cmd.task_settings_page.open_local_script_repo | 独立任务设置：自动七圣召唤：脚本仓库 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:780 |
| cmd.task_settings_page.run_inventory_count_comparison | 按当前选择的对比目标启动背包数量 OCR 对比独立任务；运行期间临时打开背包图标采集状态，结束后关闭。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:907 |
| cmd.task_settings_page.sone_dragon_flow | 执行当前选中的一条龙配置；未选中配置时不会启动。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:349 |
| cmd.task_settings_page.stop_solo_task | 向 BetterGI 独立任务发送取消请求并重置任务开关；需要继续观测任务是否已停止。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:364 |
| cmd.task_settings_page.switch_artifact_salvage | 使用 BetterGI 当前配置启动圣遗物分解任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:786 |
| cmd.task_settings_page.switch_auto_album | 使用 BetterGI 当前配置启动千音雅集任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:680 |
| cmd.task_settings_page.switch_auto_boss | 使用 BetterGI 当前配置启动首领讨伐任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:538 |
| cmd.task_settings_page.switch_auto_combo | 宿主反射命令 BetterGenshinImpact.ViewModel.Pages.TaskSettingsPageViewModel.SwitchAutoComboCommand；需当前上下文、原生参数与操作后的业务证据，不从处理器返回推断用户目标完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:698 |
| cmd.task_settings_page.switch_auto_combo_run | 独立任务设置：响应「自动连招（实验）」的界面事件 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:707 |
| cmd.task_settings_page.switch_auto_cook | 使用 BetterGI 当前配置启动自动烹饪任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:689 |
| cmd.task_settings_page.switch_auto_domain | 使用 BetterGI 当前配置启动自动秘境任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:486 |
| cmd.task_settings_page.switch_auto_fight | 使用 BetterGI 当前配置启动自动战斗任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:457 |
| cmd.task_settings_page.switch_auto_fishing | 使用 BetterGI 当前配置启动自动钓鱼任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:747 |
| cmd.task_settings_page.switch_auto_genius_invokation | 使用 BetterGI 当前配置启动七圣召唤任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:401 |
| cmd.task_settings_page.switch_auto_ley_line_outcrop | 使用 BetterGI 当前配置启动地脉花任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:757 |
| cmd.task_settings_page.switch_auto_music_game | 使用 BetterGI 当前配置启动音游任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:665 |
| cmd.task_settings_page.switch_auto_redeem_code | 使用 BetterGI 当前配置启动兑换码任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; dialogInput 必需; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:934 |
| cmd.task_settings_page.switch_auto_stygian_onslaught | 使用 BetterGI 当前配置启动幽境危战任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:560 |
| cmd.task_settings_page.switch_auto_wood | 使用 BetterGI 当前配置启动自动伐木任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId; 游戏截图器就绪 | 查询 Job 终态，再读取运行状态或任务产物；executed=true 不是游戏目标完成。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:442 |
| cmd.task_settings_page.switch_get_grid_icons | 使用 BetterGI 当前配置启动背包图标采集任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:860 |
| cmd.task_settings_page.switch_grid_icons_model_accuracy_test | 使用 BetterGI 当前配置启动背包图标模型准确率测试任务；需要游戏和截图器就绪、独立任务空闲。命令返回不等于目标已验证完成。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TaskSettingsPageViewModel.cs:892 |
| cmd.trigger_settings_page.auto_pick_mode_changed | 应用自动拾取的黑名单或白名单模式选择。 | BetterGenshinImpact.GameTask.AutoPick.AutoPickMode; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:94 |
| cmd.trigger_settings_page.go_to_hot_key_page | 触发器设置：快速传送：[手动触发快速传送触发快捷键] | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:153 |
| cmd.trigger_settings_page.toggle_voice_diagnostic_recording | 触发器设置：响应「自动剧情」的界面事件 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Pages/TriggerSettingsPageViewModel.cs:147 |
| cmd.web_image_input.submit_web_image_url | 提交图片 URL 供 BetterGI 加载或校验；可能访问网络。 | 无参数; contextId | 核对用途说明所指的界面状态；没有可观测结果时只报告处理器已返回。 | BetterGenshinImpact/ViewModel/Windows/WebImageInputViewModel.cs:44 |

## 全部设置叶节点

保留表示设置读取入口保留；是否能写由当前 writable、Schema 和联动事务决定。

| 设置入口 | 处理 | 含义 | 类型／变更钩子 | 源码 |
|---|---|---|---|---|
| setting.autoArtifactSalvageConfig.artifactSetFilter | 保留；当前契约判定可写性 | JavaScript | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:17 |
| setting.autoArtifactSalvageConfig.javaScript | 保留；当前契约判定可写性 | JavaScript | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:10 |
| setting.autoArtifactSalvageConfig.maxArtifactStar | 保留；当前契约判定可写性 | 快速分解圣遗物的最大星级 1~4 | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:27 |
| setting.autoArtifactSalvageConfig.maxNumToCheck | 保留；当前契约判定可写性 | 最多检查多少个圣遗物 | int | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:31 |
| setting.autoArtifactSalvageConfig.recognitionFailurePolicy | 保留；当前契约判定可写性 | 单次识别失败策略 | RecognitionFailurePolicy | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:35 |
| setting.autoArtifactSalvageConfig.regularExpression | 已删除：源码标记 Obsolete，已移除公共入口 | 正则表达式 | string | BetterGenshinImpact/GameTask/AutoArtifactSalvage/AutoArtifactSalvageConfig.cs:21 |
| setting.autoBossConfig.bossName | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:12 |
| setting.autoBossConfig.returnToStatueAfterEachRound | 保留；当前契约判定可写性 | 每轮讨伐后返回七天神像；开启后每次领奖后先回血，再重新前往首领 | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:36 |
| setting.autoBossConfig.reviveRetryCount | 保留；当前契约判定可写性 | 角色死亡后重试次数；战斗中存在角色死亡时，复活后重新讨伐当前首领 | int；有变更钩子 | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:33 |
| setting.autoBossConfig.rewardRecognitionEnabled | 保留；当前契约判定可写性 | 是否启用奖励名称识别。默认关闭。 | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:42 |
| setting.autoBossConfig.runCount | 保留；当前契约判定可写性 | 讨伐次数： | int；有变更钩子 | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:24 |
| setting.autoBossConfig.specifyRunCount | 保留；当前契约判定可写性 | 指定讨伐次数；关闭时刷取至原粹树脂耗尽，开启后按成功领取奖励次数停止 | bool；有变更钩子 | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:21 |
| setting.autoBossConfig.strategyName | 保留；当前契约判定可写性 | 选择战斗策略；仅用于首领讨伐，不覆盖其他策略设置 | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:15 |
| setting.autoBossConfig.teamName | 保留；当前契约判定可写性 | 切换队伍；留空则不更换队伍 | string | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:18 |
| setting.autoBossConfig.timeout | 保留；当前契约判定可写性 | 战斗超时 | int | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:48 |
| setting.autoBossConfig.useFragileResin | 保留；当前契约判定可写性 | 原粹不足时使用脆弱树脂补充： | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:30 |
| setting.autoBossConfig.useTransientResin | 保留；当前契约判定可写性 | 原粹不足时使用须臾树脂补充： | bool | BetterGenshinImpact/GameTask/AutoBoss/AutoBossConfig.cs:27 |
| setting.autoComboBuildConfig.apiKey | 保留；当前契约判定可写性 | llm服务密钥 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:26 |
| setting.autoComboBuildConfig.extraPrompt | 保留；当前契约判定可写性 | 注入给建树 LLM 的额外提示词（追加在系统指令末尾），留空则不注入 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:32 |
| setting.autoComboBuildConfig.modelName | 保留；当前契约判定可写性 | 向服务请求的模型名 To learn more about the available models, see https://platform.openai.com/docs/models. | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:20 |
| setting.autoComboBuildConfig.planningLlmEndpoint | 保留；当前契约判定可写性 | 决策模型的 OpenAI 兼容端点 | string | BetterGenshinImpact/GameTask/AutoCombo/ComboBuild/AutoComboBuildConfig.cs:13 |
| setting.autoCookConfig.checkIntervalMs | 保留；当前契约判定可写性 | 检测间隔（毫秒）；每次截图检测的时间间隔，最小 1ms | int | BetterGenshinImpact/GameTask/AutoCook/AutoCookConfig.cs:9 |
| setting.autoCookConfig.stopTaskWhenRecoverButtonDetected | 保留；当前契约判定可写性 | 自动结束烹饪任务；开启后检测到“自动烹饪”按钮会点击并结束当前任务 | bool | BetterGenshinImpact/GameTask/AutoCook/AutoCookConfig.cs:12 |
| setting.autoDomainConfig.autoArtifactSalvage | 保留；当前契约判定可写性 | 结束后是否自动分解圣遗物 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:49 |
| setting.autoDomainConfig.autoEat | 保留；当前契约判定可写性 | 自动吃药 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:37 |
| setting.autoDomainConfig.condensedResinUseCount | 保留；当前契约判定可写性 | 使用浓缩树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:80 |
| setting.autoDomainConfig.domainName | 保留；当前契约判定可写性 | 需要刷取的副本名称 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:45 |
| setting.autoDomainConfig.fightEndDelay | 保留；当前契约判定可写性 | 战斗结束后延迟几秒再开始寻找石化古树，秒 | double | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:13 |
| setting.autoDomainConfig.fragileResinUseCount | 保留；当前契约判定可写性 | 使用脆弱树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:88 |
| setting.autoDomainConfig.leftRightMoveTimes | 保留；当前契约判定可写性 | 寻找古树时，短距离移动的次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:31 |
| setting.autoDomainConfig.originalResin20UseCount | 保留；当前契约判定可写性 | 使用原粹树脂(20)刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:72 |
| setting.autoDomainConfig.originalResin40UseCount | 保留；当前契约判定可写性 | 使用原粹树脂(40)刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:76 |
| setting.autoDomainConfig.originalResinUseCount | 保留；当前契约判定可写性 | 使用原粹树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:69 |
| setting.autoDomainConfig.partyName | 保留；当前契约判定可写性 | 刷副本使用的队伍名称 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:41 |
| setting.autoDomainConfig.resinPriorityList | 保留；当前契约判定可写性 | 自定义使用树脂优先级 | List<string> | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:61 |
| setting.autoDomainConfig.reviveRetryCount | 保留；当前契约判定可写性 | 战斗死亡后重试次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:92 |
| setting.autoDomainConfig.rewardRecognitionEnabled | 保留；当前契约判定可写性 | 是否启用奖励名称识别。默认关闭。 开启后每轮领取奖励时会用 ONNX 图标匹配 + OCR 材料名双路识别奖励名称与数量，秘境结束打印汇总。 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:99 |
| setting.autoDomainConfig.shortMovement | 保留；当前契约判定可写性 | 寻找古树时，短距离移动，用于识别速度过慢的计算机使用 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:19 |
| setting.autoDomainConfig.specifyResinUse | 保留；当前契约判定可写性 | 指定树脂的使用次数 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:57 |
| setting.autoDomainConfig.sundaySelectedValue | 保留；当前契约判定可写性 | 周日奖励序号 | string | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:53 |
| setting.autoDomainConfig.transientResinUseCount | 保留；当前契约判定可写性 | 使用须臾树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:84 |
| setting.autoDomainConfig.walkToF | 保留；当前契约判定可写性 | 寻找古树时，短距离移动，用于识别速度过慢的计算机使用 | bool | BetterGenshinImpact/GameTask/AutoDomain/AutoDomainConfig.cs:25 |
| setting.autoEatConfig.checkInterval | 保留；当前契约判定可写性 | 检测间隔时间（毫秒） | int | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:27 |
| setting.autoEatConfig.defaultAdventurersDishName | 保留；当前契约判定可写性 | 默认的冒险类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:52 |
| setting.autoEatConfig.defaultAtkBoostingDishName | 保留；当前契约判定可写性 | 默认的攻击类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:46 |
| setting.autoEatConfig.defaultDefBoostingDishName | 保留；当前契约判定可写性 | 默认的防御类料理名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:58 |
| setting.autoEatConfig.eatInterval | 保留；当前契约判定可写性 | 吃药间隔时间（毫秒） 防止频繁吃药 | int | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:34 |
| setting.autoEatConfig.enabled | 保留；当前契约判定可写性 | 是否启用自动吃药 | bool | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:15 |
| setting.autoEatConfig.showNotification | 保留；当前契约判定可写性 | 是否显示吃药通知 | bool | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:21 |
| setting.autoEatConfig.testFoodName | 保留；当前契约判定可写性 | 测试食物名称 | string? | BetterGenshinImpact/GameTask/AutoEat/AutoEatConfig.cs:40 |
| setting.autoFightConfig.actionSchedulerByCd | 保留；当前契约判定可写性 | 根据技能CD优化出招人员 根据填入人或人和cd，来决定当此人元素战技cd未结束时，跳过此人出招，来优化战斗流程，可填入人名或人名数字（用逗号分隔）， 多种用分号分隔，例如:白术;钟离,12;，如果人名，则用内置cd检查，如果是人名和数字，则把数字当做出招cd(秒)。 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:33 |
| setting.autoFightConfig.battleThresholdForLoot | 保留；当前契约判定可写性 | 拾取战斗人次阈值,当战斗人次小于一定次数，就结束战斗情况下，不触发拾取掉落物和万叶拾取后拾取，只有不小于2时才生效。 | int? | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:160 |
| setting.autoFightConfig.burstEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:181 |
| setting.autoFightConfig.damageNumberRecognitionMode | 保留；当前契约判定可写性 | 伤害数字识别模式 | DamageNumberRecognitionMode | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:226 |
| setting.autoFightConfig.drawRecognitionResults | 保留；当前契约判定可写性 | 绘制识别结果位置：在遮罩窗口上显示血条、伤害数字等识别结果的边框 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:232 |
| setting.autoFightConfig.enableCombatTargeting | 保留；当前契约判定可写性 | 战斗中持续索敌：战斗过程中情况允许时持续尝试面朝敌人 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:208 |
| setting.autoFightConfig.expBasedPickupEnabled | 保留；当前契约判定可写性 | 基于经验值判断是否执行战后拾取（检测到精英怪经验值图标时才拾取） | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:196 |
| setting.autoFightConfig.fightFinishDetectEnabled | 保留；当前契约判定可写性 | 检测战斗结束 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:26 |
| setting.autoFightConfig.finishDetectConfig | 保留；当前契约判定可写性 | 战斗结束相关配置 | FightFinishDetectConfig | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:142 |
| setting.autoFightConfig.guardianAvatar | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:172 |
| setting.autoFightConfig.guardianAvatarHold | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:178 |
| setting.autoFightConfig.guardianCombatSkip | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:175 |
| setting.autoFightConfig.kazuhaPartyName | 保留；当前契约判定可写性 | 战斗结束后，如果不存在万叶，则切换至存在万叶的队伍（基于开启万叶拾取情况下） | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:187 |
| setting.autoFightConfig.kazuhaPickupEnabled | 保留；当前契约判定可写性 | 战斗结束后，如果存在枫原万叶，则使用该角色捡材料 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:166 |
| setting.autoFightConfig.lockLostWaitTime | 保留；当前契约判定可写性 | 脱锁等待时间（秒）：敌人不可见时等待一定时间后开始旋转索敌 | double | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:214 |
| setting.autoFightConfig.onlyPickEliteDropsMode | 保留；当前契约判定可写性 | 只拾取精英掉落 Closed ：关闭功能 AllowAutoPickupForNonElite: 非精英允许自动拾取：战斗过程中掉落脚下的可以自动拾取，但不会执行万叶拾取和拾取配置逻辑。 DisableAutoPickupForNonElite: 非精英关闭拾取：战斗过程中掉落到脚下的也不会自动拾取。 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:40 |
| setting.autoFightConfig.pickDropsAfterFightEnabled | 保留；当前契约判定可写性 | 战斗结束后光柱扫描掉落物 | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:148 |
| setting.autoFightConfig.pickDropsAfterFightSeconds | 保留；当前契约判定可写性 | 战斗结束后光柱扫描掉落物的持续秒数 | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:154 |
| setting.autoFightConfig.qinDoublePickUp | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:169 |
| setting.autoFightConfig.strategyName | 保留；当前契约判定可写性 | 选择战斗策略；用于战斗 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:16 |
| setting.autoFightConfig.swimmingEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:190 |
| setting.autoFightConfig.targetingDetectionInterval | 保留；当前契约判定可写性 | 索敌识别间隔（毫秒） | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:220 |
| setting.autoFightConfig.teamNames | 保留；当前契约判定可写性 | 英文逗号分割 强制指定队伍角色 | string | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:21 |
| setting.autoFightConfig.timeout | 保留；当前契约判定可写性 | 战斗超时，单位秒 | int | BetterGenshinImpact/GameTask/AutoFight/AutoFightConfig.cs:202 |
| setting.autoFishingConfig.autoThrowRodEnabled | 保留；当前契约判定可写性 | // 鱼儿上钩文字识别区域 // 暂时无用 // | bool | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:30 |
| setting.autoFishingConfig.autoThrowRodTimeOut | 保留；当前契约判定可写性 | 自动抛竿未上钩超时时间(秒) | int | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:35 |
| setting.autoFishingConfig.enabled | 保留；当前契约判定可写性 | 触发器是否启用 启用后： 1. 自动判断是否进入钓鱼状态 2. 自动提杆 3. 自动拉条 | bool | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:19 |
| setting.autoFishingConfig.fishingTimePolicy | 保留；当前契约判定可写性 | 昼夜策略 钓全天的鱼、还是只钓白天或夜晚的鱼 | FishingTimePolicy | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:47 |
| setting.autoFishingConfig.wholeProcessTimeoutSeconds | 保留；当前契约判定可写性 | 整个任务超时时间 | int | BetterGenshinImpact/GameTask/AutoFishing/AutoFishingConfig.cs:40 |
| setting.autoFixWin11BitBlt | 保留；当前契约判定可写性 | 自动修复Win11下BitBlt截图方式不可用的问题 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:89 |
| setting.autoGeniusInvokationConfig.activeCharacterCardSpace | 保留；当前契约判定可写性 | // 角色卡牌区域向左扩展距离，包含HP区域 // | int | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:44 |
| setting.autoGeniusInvokationConfig.characterCardExtendHpRect | 保留；当前契约判定可写性 | HP区域 在 角色卡牌区域 的相对位置 | Rect | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:49 |
| setting.autoGeniusInvokationConfig.defaultCharacterCardRects | 保留；当前契约判定可写性 | — | List<Rect> | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:18 |
| setting.autoGeniusInvokationConfig.myDiceCountRect | 保留；当前契约判定可写性 | 骰子数量文字识别区域 | Rect | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:29 |
| setting.autoGeniusInvokationConfig.sleepDelay | 保留；当前契约判定可写性 | 设置延时（毫秒）；如果频繁出现操作速度过快，操作动画未播放完毕的情况可以添加延时 | int | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:16 |
| setting.autoGeniusInvokationConfig.strategyName | 保留；当前契约判定可写性 | 选择卡组；选择你想要使用的卡组与策略 | string | BetterGenshinImpact/GameTask/AutoGeniusInvokation/AutoGeniusInvokationConfig.cs:14 |
| setting.autoLeyLineOutcropConfig.count | 保留；当前契约判定可写性 | 刷取次数；树脂耗尽模式关闭或统计失败时使用的固定次数。 | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:27 |
| setting.autoLeyLineOutcropConfig.country | 保留；当前契约判定可写性 | 国家；按国家选择刷取对应的地脉花。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:18 |
| setting.autoLeyLineOutcropConfig.fightConfig.actionSchedulerByCd | 保留；当前契约判定可写性 | 根据技能CD优化出招人员。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:25 |
| setting.autoLeyLineOutcropConfig.fightConfig.burstEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:49 |
| setting.autoLeyLineOutcropConfig.fightConfig.fightFinishDetectEnabled | 保留；当前契约判定可写性 | 检测战斗结束。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:20 |
| setting.autoLeyLineOutcropConfig.fightConfig.finishDetectConfig | 保留；当前契约判定可写性 | — | FightFinishDetectConfig | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:45 |
| setting.autoLeyLineOutcropConfig.fightConfig.guardianAvatar | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:46 |
| setting.autoLeyLineOutcropConfig.fightConfig.guardianAvatarHold | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:48 |
| setting.autoLeyLineOutcropConfig.fightConfig.guardianCombatSkip | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:47 |
| setting.autoLeyLineOutcropConfig.fightConfig.kazuhaPickupEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:51 |
| setting.autoLeyLineOutcropConfig.fightConfig.qinDoublePickUp | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:52 |
| setting.autoLeyLineOutcropConfig.fightConfig.seekEnemyEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:54 |
| setting.autoLeyLineOutcropConfig.fightConfig.seekEnemyIntervalSeconds | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:55 |
| setting.autoLeyLineOutcropConfig.fightConfig.seekEnemyRotaryFactor | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:56 |
| setting.autoLeyLineOutcropConfig.fightConfig.strategyName | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:10 |
| setting.autoLeyLineOutcropConfig.fightConfig.swimmingEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:50 |
| setting.autoLeyLineOutcropConfig.fightConfig.teamNames | 保留；当前契约判定可写性 | 英文逗号分割，强制指定队伍角色。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:15 |
| setting.autoLeyLineOutcropConfig.fightConfig.timeout | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropFightConfig.cs:53 |
| setting.autoLeyLineOutcropConfig.friendshipTeam | 保留；当前契约判定可写性 | 好感队名称；领取奖励前切换到该队伍，留空则不切换。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:39 |
| setting.autoLeyLineOutcropConfig.isGoToSynthesizer | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:45 |
| setting.autoLeyLineOutcropConfig.isResinExhaustionMode | 保留；当前契约判定可写性 | 树脂耗尽模式；按当前树脂与库存自动计算可刷次数，结束后自动停止。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:21 |
| setting.autoLeyLineOutcropConfig.leyLineOutcropType | 保留；当前契约判定可写性 | 地脉花类型；选择刷取的地脉花，启示之花（经验书）或藏金之花（摩拉）。 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:15 |
| setting.autoLeyLineOutcropConfig.openModeCountMin | 保留；当前契约判定可写性 | 刷取次数取小值；与手动次数取最小值，避免超过树脂可用次数。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:24 |
| setting.autoLeyLineOutcropConfig.scanDropsAfterRewardEnabled | 保留；当前契约判定可写性 | 是否在领取地脉花奖励后扫描周围掉落物光柱。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:51 |
| setting.autoLeyLineOutcropConfig.scanDropsAfterRewardSeconds | 保留；当前契约判定可写性 | 领取奖励后扫描掉落物的最长时长，单位为秒。 | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:57 |
| setting.autoLeyLineOutcropConfig.team | 保留；当前契约判定可写性 | 战斗队伍名称 | string | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:36 |
| setting.autoLeyLineOutcropConfig.timeout | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:42 |
| setting.autoLeyLineOutcropConfig.useFragileResin | 保留；当前契约判定可写性 | 使用脆弱树脂；原粹与浓缩耗尽后，允许使用脆弱树脂继续刷取。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:33 |
| setting.autoLeyLineOutcropConfig.useTransientResin | 保留；当前契约判定可写性 | 使用须臾树脂；原粹与浓缩耗尽后，允许使用须臾树脂继续刷取。 | bool | BetterGenshinImpact/GameTask/AutoLeyLineOutcrop/AutoLeyLineOutcropConfig.cs:30 |
| setting.autoMusicGameConfig.musicLevel | 保留；当前契约判定可写性 | 乐曲级别 | string | BetterGenshinImpact/GameTask/AutoMusicGame/AutoMusicGameConfig.cs:19 |
| setting.autoMusicGameConfig.mustCanorusLevel | 保留；当前契约判定可写性 | 自动达到大音天籁的级别 | bool | BetterGenshinImpact/GameTask/AutoMusicGame/AutoMusicGameConfig.cs:15 |
| setting.autoPickConfig.blacklistModePickEnabled | 保留；当前契约判定可写性 | 黑名单模式的拾取规则启用状态 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:67 |
| setting.autoPickConfig.enabled | 保留；当前契约判定可写性 | 触发器是否启用 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:22 |
| setting.autoPickConfig.fastModeEnabled | 保留；当前契约判定可写性 | 急速模式 无视文字识别结果，直接拾取 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:52 |
| setting.autoPickConfig.itemIconLeftOffset | 保留；当前契约判定可写性 | 1080p下拾取文字左边的起始偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:27 |
| setting.autoPickConfig.itemTextLeftOffset | 保留；当前契约判定可写性 | 1080p下拾取文字的起始偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:32 |
| setting.autoPickConfig.itemTextRightOffset | 保留；当前契约判定可写性 | 1080p下拾取文字的终止偏移 | int | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:37 |
| setting.autoPickConfig.mode | 保留；当前契约判定可写性 | 自动拾取名单模式 | AutoPickMode | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:62 |
| setting.autoPickConfig.ocrEngine | 保留；当前契约判定可写性 | 文字识别引擎 - Paddle - Yap | string | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:44 |
| setting.autoPickConfig.pickKey | 保留；当前契约判定可写性 | 自定义按键拾取 | string | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:57 |
| setting.autoPickConfig.whitelistModeDoNotPickEnabled | 保留；当前契约判定可写性 | 白名单模式的不拾取规则启用状态 | bool | BetterGenshinImpact/GameTask/AutoPick/AutoPickConfig.cs:71 |
| setting.autoRedeemCodeConfig.clipboardListenerEnabled | 保留；当前契约判定可写性 | 是否启用剪切板监听 | bool | BetterGenshinImpact/GameTask/UseRedeemCode/AutoRedeemCodeConfig.cs:12 |
| setting.autoSkipConfig.afterChooseOptionSleepDelay | 保留；当前契约判定可写性 | 选择选项前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:36 |
| setting.autoSkipConfig.autoGetDailyRewardsEnabled | 保留；当前契约判定可写性 | 自动领取每日委托奖励 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:66 |
| setting.autoSkipConfig.autoHangoutChooseOptionSleepDelay | 保留；当前契约判定可写性 | 自动邀约选择选项前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:117 |
| setting.autoSkipConfig.autoHangoutEndChoose | 保留；当前契约判定可写性 | 自动邀约分支选择 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:111 |
| setting.autoSkipConfig.autoHangoutEventEnabled | 保留；当前契约判定可写性 | 自动邀约启用 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:105 |
| setting.autoSkipConfig.autoHangoutPressSkipEnabled | 保留；当前契约判定可写性 | 自动邀约自动点击跳过按钮 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:123 |
| setting.autoSkipConfig.autoReExploreCharacter | 已删除：源码标记 Obsolete，已移除公共入口 | 自动重新派遣使用角色配置，逗号分割 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:78 |
| setting.autoSkipConfig.autoReExploreEnabled | 保留；当前契约判定可写性 | 自动重新派遣 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:72 |
| setting.autoSkipConfig.autoWaitDialogueOptionVoiceEnabled | 保留；当前契约判定可写性 | 选择剧情选项前，通过游戏进程音频的人声检测自动等待语音结束 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:42 |
| setting.autoSkipConfig.beforeClickConfirmDelay | 保留；当前契约判定可写性 | 点击对话框前的延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:60 |
| setting.autoSkipConfig.bringGameToFrontAfterBackgroundDialogEnabled | 保留；当前契约判定可写性 | 后台剧情结束后切回游戏前台 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:149 |
| setting.autoSkipConfig.clickChatOption | 保留；当前契约判定可写性 | 优先选择第一个选项 优先选择最后一个选项 不选择选项 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:87 |
| setting.autoSkipConfig.closePopupPagedEnabled | 保留；当前契约判定可写性 | 关闭弹出层 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:175 |
| setting.autoSkipConfig.customPriorityOptions | 保留；当前契约判定可写性 | 自定义优先选项文本，每行一个或用分号分隔 | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:99 |
| setting.autoSkipConfig.customPriorityOptionsEnabled | 保留；当前契约判定可写性 | 自定义优先选项启用 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:93 |
| setting.autoSkipConfig.dialogueOptionVoiceMaxWaitSeconds | 保留；当前契约判定可写性 | 人声检测等待语音结束的最大等待时间（秒） | int | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:48 |
| setting.autoSkipConfig.dialogueOptionVoiceVadDiagnosticEnabled | 保留；当前契约判定可写性 | 持续检测游戏进程音频并在设置页显示 Silero VAD 诊断数据 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:54 |
| setting.autoSkipConfig.enabled | 保留；当前契约判定可写性 | 触发器是否启用 启用后： 1. 快速跳过对话 2. 自动点击一个识别到的选项 3. 黑屏过长自动点击跳过 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:20 |
| setting.autoSkipConfig.pictureInPictureEnabled | 保留；当前契约判定可写性 | 游戏失焦时显示画中画 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:161 |
| setting.autoSkipConfig.pictureInPictureSourceType | 保留；当前契约判定可写性 | 画中画的源图像类型 TriggerDispatcher：来自于截图器50ms一次 CaptureLoop：主动获取（60帧） | string | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:169 |
| setting.autoSkipConfig.quicklySkipConversationsEnabled | 保留；当前契约判定可写性 | 快速跳过对话 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:26 |
| setting.autoSkipConfig.runBackgroundEnabled | 保留；当前契约判定可写性 | 后台运行 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:143 |
| setting.autoSkipConfig.skipBuiltInClickOptions | 保留；当前契约判定可写性 | JS调用时跳过内置默认点击选项 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:181 |
| setting.autoSkipConfig.submitGoodsEnabled | 保留；当前契约判定可写性 | 提交物品 | bool | BetterGenshinImpact/GameTask/AutoSkip/AutoSkipConfig.cs:155 |
| setting.autoStygianOnslaughtConfig.autoArtifactSalvage | 保留；当前契约判定可写性 | 结束后是否自动分解圣遗物 | bool | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:18 |
| setting.autoStygianOnslaughtConfig.bossNum | 保留；当前契约判定可写性 | boss 名字 1~3 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:14 |
| setting.autoStygianOnslaughtConfig.condensedResinUseCount | 保留；当前契约判定可写性 | 使用浓缩树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:38 |
| setting.autoStygianOnslaughtConfig.fightTeamName | 保留；当前契约判定可写性 | 指定战斗队伍 | string | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:50 |
| setting.autoStygianOnslaughtConfig.fragileResinUseCount | 保留；当前契约判定可写性 | 使用脆弱树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:46 |
| setting.autoStygianOnslaughtConfig.originalResinUseCount | 保留；当前契约判定可写性 | 使用原粹树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:34 |
| setting.autoStygianOnslaughtConfig.resinPriorityList | 保留；当前契约判定可写性 | 自定义使用树脂优先级 | List<string> | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:26 |
| setting.autoStygianOnslaughtConfig.specifyResinUse | 保留；当前契约判定可写性 | 指定树脂的使用次数 | bool | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:22 |
| setting.autoStygianOnslaughtConfig.strategyName | 保留；当前契约判定可写性 | 选择战斗策略；用于战斗 | string | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:10 |
| setting.autoStygianOnslaughtConfig.transientResinUseCount | 保留；当前契约判定可写性 | 使用须臾树脂刷取副本次数 | int | BetterGenshinImpact/GameTask/AutoStygianOnslaught/AutoStygianOnslaughtConfig.cs:42 |
| setting.autoWoodConfig.afterZSleepDelay | 保留；当前契约判定可写性 | 使用小道具后的额外延迟（毫秒） | int | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:15 |
| setting.autoWoodConfig.useWonderlandRefresh | 保留；当前契约判定可写性 | 使用进出千星奇域刷新CD | bool | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:27 |
| setting.autoWoodConfig.woodCountOcrEnabled | 保留；当前契约判定可写性 | 木材数量OCR是否启用 | bool | BetterGenshinImpact/GameTask/AutoWood/AutoWoodConfig.cs:21 |
| setting.captureMode | 保留；当前契约判定可写性 | 窗口捕获的方式 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:43 |
| setting.childSessionConfig.audioMuted | 保留；当前契约判定可写性 | 桌面分身的 RDP 音频是否静音，不影响主桌面的其他程序。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:54 |
| setting.childSessionConfig.gameMouseModeEnabled | 保留；当前契约判定可写性 | 桌面分身是否启用游戏鼠标模式。默认使用普通鼠标模式。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:48 |
| setting.childSessionConfig.keepAspectRatio | 保留；当前契约判定可写性 | 桌面分身窗口是否保持 16:9 宽高比。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:36 |
| setting.childSessionConfig.normalWindowPosition.left | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:61 |
| setting.childSessionConfig.normalWindowPosition.top | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:63 |
| setting.childSessionConfig.sendSystemShortcutsToRemote | 保留；当前契约判定可写性 | Alt+Tab、Windows 键等系统组合键是否发送到桌面分身。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:42 |
| setting.childSessionConfig.smallWindowPosition.left | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:61 |
| setting.childSessionConfig.smallWindowPosition.top | 保留；当前契约判定可写性 | — | int | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:63 |
| setting.childSessionConfig.smartSizingEnabled | 保留；当前契约判定可写性 | RDP 画面是否自适应窗口。关闭时使用 1:1 显示。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:30 |
| setting.childSessionConfig.topmostEnabled | 保留；当前契约判定可写性 | 桌面分身窗口是否置顶。 | bool | BetterGenshinImpact/Core/Config/ChildSessionConfig.cs:24 |
| setting.commonConfig.currentBackdropType | 保留；当前契约判定可写性 | 主题（旧版主题，兼容性保留） | WindowBackdropType | BetterGenshinImpact/Core/Config/CommonConfig.cs:63 |
| setting.commonConfig.currentThemeType | 保留；当前契约判定可写性 | 当前主题类型（新版主题） | ThemeType | BetterGenshinImpact/Core/Config/CommonConfig.cs:57 |
| setting.commonConfig.exitToTray | 保留；当前契约判定可写性 | 退出时最小化至托盘 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:51 |
| setting.commonConfig.isFirstRun | 保留；当前契约判定可写性 | 是否是第一次运行 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:93 |
| setting.commonConfig.mainBackgroundEnabled | 保留；当前契约判定可写性 | 是否启用主窗口自定义背景图 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:69 |
| setting.commonConfig.mainBackgroundImagePath | 保留；当前契约判定可写性 | 主窗口自定义背景图路径，空字符串表示未设置 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:75 |
| setting.commonConfig.mainBackgroundOpacity | 保留；当前契约判定可写性 | 主窗口背景图不透明度，数值越小背景越淡，文字越清晰 | double | BetterGenshinImpact/Core/Config/CommonConfig.cs:81 |
| setting.commonConfig.mainBackgroundStretch | 保留；当前契约判定可写性 | 主窗口背景图拉伸模式 | Stretch | BetterGenshinImpact/Core/Config/CommonConfig.cs:87 |
| setting.commonConfig.onceHadRunDeviceIdList | 保留；当前契约判定可写性 | 一个设备只运行一次的已运行设备ID列表 | List<string> | BetterGenshinImpact/Core/Config/CommonConfig.cs:105 |
| setting.commonConfig.redeemCodeCnFeedsNotificationEnabled | 保留；当前契约判定可写性 | 国服兑换码更新是否允许通知 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:118 |
| setting.commonConfig.redeemCodeFeedsUpdateVersion | 保留；当前契约判定可写性 | 当前看过的兑换码推送版本 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:112 |
| setting.commonConfig.redeemCodeGlobalFeedsNotificationEnabled | 保留；当前契约判定可写性 | 国际服兑换码更新是否允许通知 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:124 |
| setting.commonConfig.redeemCodeGlobalFeedsUpdateVersion | 保留；当前契约判定可写性 | 当前看过的国际服兑换码推送版本 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:130 |
| setting.commonConfig.rewardRecognitionScreenshotEnabled | 保留；当前契约判定可写性 | 是否保存奖励识别调试截图 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:45 |
| setting.commonConfig.runForVersion | 保留；当前契约判定可写性 | 这个版本是否运行过 | string | BetterGenshinImpact/Core/Config/CommonConfig.cs:99 |
| setting.commonConfig.screenshotEnabled | 保留；当前契约判定可写性 | 是否启用遮罩窗口 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:33 |
| setting.commonConfig.screenshotUidCoverEnabled | 保留；当前契约判定可写性 | UID遮盖是否启用 | bool | BetterGenshinImpact/Core/Config/CommonConfig.cs:39 |
| setting.detailedErrorLogs | 保留；当前契约判定可写性 | 详细的错误日志 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:49 |
| setting.devConfig.recognitionAssetsRootPath | 保留；当前契约判定可写性 | Recognition 模板制作工具最近使用的 Assets 根目录 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:29 |
| setting.devConfig.recognitionAssetsRootPathHistory | 保留；当前契约判定可写性 | Recognition 模板制作工具最近使用的输出文件夹历史，按最近使用顺序保存，最多 10 条。 | List<string> | BetterGenshinImpact/Core/Config/DevConfig.cs:35 |
| setting.devConfig.recognitionJsonPath | 保留；当前契约判定可写性 | Recognition 模板制作工具最近使用的配置文件 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:19 |
| setting.devConfig.recognitionJsonPathHistory | 保留；当前契约判定可写性 | Recognition 模板制作工具最近使用的配置文件历史，按最近使用顺序保存，最多 10 条。 | List<string> | BetterGenshinImpact/Core/Config/DevConfig.cs:25 |
| setting.devConfig.recordMapName | 保留；当前契约判定可写性 | 录制地图名称 | string | BetterGenshinImpact/Core/Config/DevConfig.cs:15 |
| setting.disableInputMonitor | 保留；当前契约判定可写性 | 禁用键鼠监听，需重启 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:104 |
| setting.genshinStartConfig.autoDisableGenshinHdrEnabled | 保留；当前契约判定可写性 | 启动前自动关闭原神 HDR（删除原神 HDR 对应注册表键） | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:54 |
| setting.genshinStartConfig.autoEnterGameEnabled | 保留；当前契约判定可写性 | 自动进入游戏（开门） | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:21 |
| setting.genshinStartConfig.genshinStartArgs | 保留；当前契约判定可写性 | 原神启动参数 | string | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:27 |
| setting.genshinStartConfig.installPath | 保留；当前契约判定可写性 | 原神安装路径 | string | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:33 |
| setting.genshinStartConfig.linkedStartEnabled | 保留；当前契约判定可写性 | 联动启动原神本体 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:39 |
| setting.genshinStartConfig.recordGameTimeEnabled | 保留；当前契约判定可写性 | 使用Starward同步记录时间 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:45 |
| setting.genshinStartConfig.startGameWithCmd | 保留；当前契约判定可写性 | 使用 CMD 启动游戏；如果原神弹窗“检测到非法工具，请重启机器”，请尝试开启此选项 | bool | BetterGenshinImpact/Core/Config/GenshinStartConfig.cs:48 |
| setting.getGridIconsConfig.gridName | 保留；当前契约判定可写性 | Grid界面名称 | GridScreenName | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:13 |
| setting.getGridIconsConfig.lvAsSuffix | 保留；当前契约判定可写性 | 使用等级作为后缀 | bool | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:21 |
| setting.getGridIconsConfig.maxNumToGet | 保留；当前契约判定可写性 | 最多获取多少个图标 | int | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:25 |
| setting.getGridIconsConfig.starAsSuffix | 保留；当前契约判定可写性 | 使用星星作为后缀 | bool | BetterGenshinImpact/GameTask/GetGridIcons/GetGridIconsConfig.cs:17 |
| setting.hardwareAccelerationConfig.additionalPath | 保留；当前契约判定可写性 | 附加path，用;分割。默认为空。 | string | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:33 |
| setting.hardwareAccelerationConfig.autoAppendCudaPath | 保留；当前契约判定可写性 | 自动附加cuda的path。一般情况下用这个就足够了。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:55 |
| setting.hardwareAccelerationConfig.cpuOcr | 保留；当前契约判定可写性 | 是否强制OCR使用CPU推理。在某些环境上使用GPU进行OCR推理会导致性能下降(比如很多使用DirectML推理的情况下)。默认开启。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:19 |
| setting.hardwareAccelerationConfig.cudaDevice | 保留；当前契约判定可写性 | 强制指定cuda设备,默认为0(使用默认设备) | int | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:49 |
| setting.hardwareAccelerationConfig.embedTensorRtCache | 保留；当前契约判定可写性 | 嵌入式引擎缓存。将引擎缓存嵌入到模型中。默认开启。关闭它可能会提高性能(如果不爆炸的话)。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:71 |
| setting.hardwareAccelerationConfig.enableOpenVinoCache | 保留；当前契约判定可写性 | 启用 OpenVINO 缓存。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:86 |
| setting.hardwareAccelerationConfig.enableTensorRtCache | 保留；当前契约判定可写性 | 启用TensorRT缓存。默认开启。不开的话使用TensorRT每次加载模型会卡爆。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:65 |
| setting.hardwareAccelerationConfig.gpuDevice | 保留；当前契约判定可写性 | 强制指定gpu设备,默认为0(使用默认设备) | int | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:27 |
| setting.hardwareAccelerationConfig.inferenceDevice | 保留；当前契约判定可写性 | 推理使用的设备。默认CPU | InferenceDeviceType | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:13 |
| setting.hardwareAccelerationConfig.openVinoDevice | 保留；当前契约判定可写性 | OpenVino 设备参数。 | string | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:80 |
| setting.hardwareAccelerationConfig.optimizedModel | 保留；当前契约判定可写性 | 是否输出优化后的模型文件到缓存。注意:在不支持的执行器上使用会导致异常。默认关闭。 | bool | BetterGenshinImpact/Core/Config/HardwareAccelerationConfig.cs:39 |
| setting.hotKeyConfig.addWaypointHotkey | 保留；当前契约判定可写性 | 添加路径记录点 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:222 |
| setting.hotKeyConfig.addWaypointHotkeyType | 保留；当前契约判定可写性 | 「添加路径点」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:225 |
| setting.hotKeyConfig.autoCookGameHotkey | 保留；当前契约判定可写性 | 自动烹饪开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:180 |
| setting.hotKeyConfig.autoCookGameHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动烹饪」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:183 |
| setting.hotKeyConfig.autoDomainHotkey | 保留；当前契约判定可写性 | 触发「启动/停止自动秘境」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:19 |
| setting.hotKeyConfig.autoDomainHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动秘境」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:22 |
| setting.hotKeyConfig.autoFightHotkey | 保留；当前契约判定可写性 | 触发「启动/停止自动战斗」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:25 |
| setting.hotKeyConfig.autoFightHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动战斗」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:28 |
| setting.hotKeyConfig.autoFishingEnabledHotkey | 保留；当前契约判定可写性 | 触发「自动钓鱼开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:31 |
| setting.hotKeyConfig.autoFishingEnabledHotkeyType | 保留；当前契约判定可写性 | 「自动钓鱼开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:34 |
| setting.hotKeyConfig.autoFishingGameHotkey | 保留；当前契约判定可写性 | 活动音游开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:173 |
| setting.hotKeyConfig.autoFishingGameHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动钓鱼」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:176 |
| setting.hotKeyConfig.autoGeniusInvokationHotkey | 保留；当前契约判定可写性 | 触发「启动/停止自动七圣召唤」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:37 |
| setting.hotKeyConfig.autoGeniusInvokationHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动七圣召唤」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:40 |
| setting.hotKeyConfig.autoMusicGameHotkey | 保留；当前契约判定可写性 | 活动音游开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:166 |
| setting.hotKeyConfig.autoMusicGameHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动音游」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:169 |
| setting.hotKeyConfig.autoPickEnabledHotkey | 保留；当前契约判定可写性 | 触发「自动拾取开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:43 |
| setting.hotKeyConfig.autoPickEnabledHotkeyType | 保留；当前契约判定可写性 | 「自动拾取开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:46 |
| setting.hotKeyConfig.autoSkipEnabledHotkey | 保留；当前契约判定可写性 | 触发「自动剧情开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:49 |
| setting.hotKeyConfig.autoSkipEnabledHotkeyType | 保留；当前契约判定可写性 | 「自动剧情开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:52 |
| setting.hotKeyConfig.autoSkipHangoutEnabledHotkey | 保留；当前契约判定可写性 | 触发「自动邀约开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:55 |
| setting.hotKeyConfig.autoSkipHangoutEnabledHotkeyType | 保留；当前契约判定可写性 | 「自动邀约开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:58 |
| setting.hotKeyConfig.autoTrackHotkey | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:13 |
| setting.hotKeyConfig.autoTrackHotkeyType | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:16 |
| setting.hotKeyConfig.autoTrackPathHotkey | 保留；当前契约判定可写性 | 自动寻路 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:187 |
| setting.hotKeyConfig.autoTrackPathHotkeyType | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:190 |
| setting.hotKeyConfig.autoWoodHotkey | 保留；当前契约判定可写性 | 触发「启动/停止自动伐木」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:61 |
| setting.hotKeyConfig.autoWoodHotkeyType | 保留；当前契约判定可写性 | 「启动/停止自动伐木」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:64 |
| setting.hotKeyConfig.bgiEnabledHotkey | 保留；当前契约判定可写性 | 触发「启动停止 BetterGI」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:67 |
| setting.hotKeyConfig.bgiEnabledHotkeyType | 保留；当前契约判定可写性 | 「启动停止 BetterGI」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:70 |
| setting.hotKeyConfig.cancelTaskHotkey | 保留；当前契约判定可写性 | 停止任意独立任务 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:262 |
| setting.hotKeyConfig.cancelTaskHotkeyType | 保留；当前契约判定可写性 | 「停止当前脚本/独立任务」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:265 |
| setting.hotKeyConfig.clickGenshinCancelButtonHotkey | 保留；当前契约判定可写性 | 点击取消按钮 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:145 |
| setting.hotKeyConfig.clickGenshinCancelButtonHotkeyType | 保留；当前契约判定可写性 | 「快捷点击原神内取消按钮」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:148 |
| setting.hotKeyConfig.clickGenshinConfirmButtonHotkey | 保留；当前契约判定可写性 | 点击确认按钮 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:138 |
| setting.hotKeyConfig.clickGenshinConfirmButtonHotkeyType | 保留；当前契约判定可写性 | 「快捷点击原神内确认按钮」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:141 |
| setting.hotKeyConfig.enhanceArtifactHotkey | 保留；当前契约判定可写性 | 触发「按下快速强化圣遗物」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:73 |
| setting.hotKeyConfig.enhanceArtifactHotkeyType | 保留；当前契约判定可写性 | 「按下快速强化圣遗物」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:76 |
| setting.hotKeyConfig.executePathHotkey | 保留；当前契约判定可写性 | 路径执行 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:229 |
| setting.hotKeyConfig.executePathHotkeyType | 保留；当前契约判定可写性 | 「（测试）播放内存中的路径」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:232 |
| setting.hotKeyConfig.keyMouseMacroRecordHotkey | 保留；当前契约判定可写性 | 键鼠录制/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:250 |
| setting.hotKeyConfig.keyMouseMacroRecordHotkeyType | 保留；当前契约判定可写性 | 「启动/停止键鼠录制」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:253 |
| setting.hotKeyConfig.logBoxDisplayHotkey | 保留；当前契约判定可写性 | 日志与状态窗口展示 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:236 |
| setting.hotKeyConfig.logBoxDisplayHotkeyType | 保留；当前契约判定可写性 | 「日志与状态窗口展示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:239 |
| setting.hotKeyConfig.mapMaskEnabledHotkey | 保留；当前契约判定可写性 | 地图遮罩开关 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:276 |
| setting.hotKeyConfig.mapMaskEnabledHotkeyType | 保留；当前契约判定可写性 | 「地图遮罩开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:279 |
| setting.hotKeyConfig.mapPosRecordHotkey | 保留；当前契约判定可写性 | 地图路线录制开始/停止 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:159 |
| setting.hotKeyConfig.mapPosRecordHotkeyType | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:162 |
| setting.hotKeyConfig.oneKeyClaimRewardHotkey | 保留；当前契约判定可写性 | 触发「一键领取奖励」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:85 |
| setting.hotKeyConfig.oneKeyClaimRewardHotkeyType | 保留；当前契约判定可写性 | 「一键领取奖励」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:88 |
| setting.hotKeyConfig.oneKeyFightHotkey | 保留；当前契约判定可写性 | 一键战斗宏 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:152 |
| setting.hotKeyConfig.oneKeyFightHotkeyType | 保留；当前契约判定可写性 | 「一键战斗宏快捷键」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:155 |
| setting.hotKeyConfig.onedragonHotkey | 保留；当前契约判定可写性 | 停止任意独立任务 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:269 |
| setting.hotKeyConfig.onedragonHotkeyType | 保留；当前契约判定可写性 | 「启动/停止一条龙」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:272 |
| setting.hotKeyConfig.overlayMetricsDisplayHotkey | 保留；当前契约判定可写性 | 遮罩指标栏展示 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:243 |
| setting.hotKeyConfig.overlayMetricsDisplayHotkeyType | 保留；当前契约判定可写性 | 「遮罩指标栏展示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:246 |
| setting.hotKeyConfig.pathRecorderHotkey | 保留；当前契约判定可写性 | 路径记录开始 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:215 |
| setting.hotKeyConfig.pathRecorderHotkeyType | 保留；当前契约判定可写性 | 「启动/停止路径记录器」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:218 |
| setting.hotKeyConfig.quickBuyHotkey | 保留；当前契约判定可写性 | 触发「按下快速购买商店物品」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:79 |
| setting.hotKeyConfig.quickBuyHotkeyType | 保留；当前契约判定可写性 | 「按下快速购买商店物品」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:82 |
| setting.hotKeyConfig.quickSereniteaPotHotkey | 保留；当前契约判定可写性 | 触发「按下快速进出尘歌壶」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:91 |
| setting.hotKeyConfig.quickSereniteaPotHotkeyType | 保留；当前契约判定可写性 | 「按下快速进出尘歌壶」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:94 |
| setting.hotKeyConfig.quickTeleportEnabledHotkey | 保留；当前契约判定可写性 | 触发「快速传送开关」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:97 |
| setting.hotKeyConfig.quickTeleportEnabledHotkeyType | 保留；当前契约判定可写性 | 「快速传送开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:100 |
| setting.hotKeyConfig.quickTeleportTickHotkey | 保留；当前契约判定可写性 | 快捷传送触发 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:104 |
| setting.hotKeyConfig.quickTeleportTickHotkeyType | 保留；当前契约判定可写性 | 「手动触发快速传送触发快捷键（按住起效）」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:107 |
| setting.hotKeyConfig.recBigMapPosHotkey | 保留；当前契约判定可写性 | 路径记录开始 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:208 |
| setting.hotKeyConfig.recBigMapPosHotkeyType | 保留；当前契约判定可写性 | 「（开发）获取当前大地图中心点位置」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:211 |
| setting.hotKeyConfig.recognitionTemplateEditorHotkey | 保留；当前契约判定可写性 | Recognition 模板素材制作 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:125 |
| setting.hotKeyConfig.recognitionTemplateEditorHotkeyType | 保留；当前契约判定可写性 | 「（开发）模板素材制作」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:128 |
| setting.hotKeyConfig.skillCdEnabledHotkey | 保留；当前契约判定可写性 | 技能CD提示开关 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:111 |
| setting.hotKeyConfig.skillCdEnabledHotkeyType | 保留；当前契约判定可写性 | 「冷却提示开关」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:114 |
| setting.hotKeyConfig.suspendHotkey | 保留；当前契约判定可写性 | 暂停 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:256 |
| setting.hotKeyConfig.suspendHotkeyType | 保留；当前契约判定可写性 | 「暂停当前脚本/独立任务」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:259 |
| setting.hotKeyConfig.takeScreenshotHotkey | 保留；当前契约判定可写性 | 截图 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:118 |
| setting.hotKeyConfig.takeScreenshotHotkeyType | 保留；当前契约判定可写性 | 「游戏截图」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:121 |
| setting.hotKeyConfig.test1Hotkey | 保留；当前契约判定可写性 | 测试 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:194 |
| setting.hotKeyConfig.test1HotkeyType | 保留；当前契约判定可写性 | 「（测试）测试」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:197 |
| setting.hotKeyConfig.test2Hotkey | 保留；当前契约判定可写性 | 测试2 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:201 |
| setting.hotKeyConfig.test2HotkeyType | 保留；当前契约判定可写性 | 「（测试）测试2」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:204 |
| setting.hotKeyConfig.turnAroundHotkey | 保留；当前契约判定可写性 | 触发「长按旋转视角 - 那维莱特转圈」的快捷键组合；空字符串表示未绑定。修改绑定不等于执行该操作。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:131 |
| setting.hotKeyConfig.turnAroundHotkeyType | 保留；当前契约判定可写性 | 「长按旋转视角 - 那维莱特转圈」快捷键的监听/注册方式，取值必须来自目录枚举；不改变快捷键组合本身。 | string | BetterGenshinImpact/Core/Config/HotKeyConfig.cs:134 |
| setting.keyBindingsConfig.abandonChallenge | 保留；当前契约判定可写性 | 中断挑战 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:130 |
| setting.keyBindingsConfig.checkTutorialDetails | 保留；当前契约判定可写性 | 查看教程详情 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:272 |
| setting.keyBindingsConfig.drop | 保留；当前契约判定可写性 | 落下 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:100 |
| setting.keyBindingsConfig.elementalBurst | 保留；当前契约判定可写性 | 元素爆发 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:70 |
| setting.keyBindingsConfig.elementalSight | 保留；当前契约判定可写性 | 长按打开元素视野 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:278 |
| setting.keyBindingsConfig.elementalSkill | 保留；当前契约判定可写性 | 元素战技 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:64 |
| setting.keyBindingsConfig.globalKeyMappingEnabled | 保留；当前契约判定可写性 | 是否启用全局按键映射 | bool | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:20 |
| setting.keyBindingsConfig.hideUI | 保留；当前契约判定可写性 | 隐藏主界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:302 |
| setting.keyBindingsConfig.interactionInSomeMode | 保留；当前契约判定可写性 | 特定玩法内交互操作 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:118 |
| setting.keyBindingsConfig.jump | 保留；当前契约判定可写性 | 跳跃；特定操作模式下向上移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:94 |
| setting.keyBindingsConfig.moveBackward | 保留；当前契约判定可写性 | 向后移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:34 |
| setting.keyBindingsConfig.moveForward | 保留；当前契约判定可写性 | 向前移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:28 |
| setting.keyBindingsConfig.moveLeft | 保留；当前契约判定可写性 | 向左移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:40 |
| setting.keyBindingsConfig.moveRight | 保留；当前契约判定可写性 | 向右移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:46 |
| setting.keyBindingsConfig.normalAttack | 保留；当前契约判定可写性 | 普通攻击 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:58 |
| setting.keyBindingsConfig.openAdventurerHandbook | 保留；当前契约判定可写性 | 打开冒险之证界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:200 |
| setting.keyBindingsConfig.openBattlePassScreen | 保留；当前契约判定可写性 | 打开纪行界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:218 |
| setting.keyBindingsConfig.openCharacterScreen | 保留；当前契约判定可写性 | 打开角色界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:182 |
| setting.keyBindingsConfig.openChatScreen | 保留；当前契约判定可写性 | 打开聊天界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:260 |
| setting.keyBindingsConfig.openCoOpScreen | 保留；当前契约判定可写性 | 打开多人游戏界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:206 |
| setting.keyBindingsConfig.openFriendsScreen | 保留；当前契约判定可写性 | 打开好友界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:296 |
| setting.keyBindingsConfig.openInventory | 保留；当前契约判定可写性 | 打开背包 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:176 |
| setting.keyBindingsConfig.openMap | 保留；当前契约判定可写性 | 打开地图 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:188 |
| setting.keyBindingsConfig.openNotificationDetails | 保留；当前契约判定可写性 | 打开通知详情 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:254 |
| setting.keyBindingsConfig.openPaimonMenu | 保留；当前契约判定可写性 | 打开派蒙界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:194 |
| setting.keyBindingsConfig.openPartySetupScreen | 保留；当前契约判定可写性 | 打开队伍配置界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:290 |
| setting.keyBindingsConfig.openQuestMenu | 保留；当前契约判定可写性 | 开关任务菜单 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:248 |
| setting.keyBindingsConfig.openSpecialEnvironmentInformation | 保留；当前契约判定可写性 | 打开特殊环境说明 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:266 |
| setting.keyBindingsConfig.openStellarReunion | 保留；当前契约判定可写性 | 打开星之归还（条件符合期间生效） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:242 |
| setting.keyBindingsConfig.openTheEventsMenu | 保留；当前契约判定可写性 | 打开活动面板 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:224 |
| setting.keyBindingsConfig.openTheFurnishingScreen | 保留；当前契约判定可写性 | 打开摆设界面（尘歌壶内） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:236 |
| setting.keyBindingsConfig.openTheSettingsMenu | 保留；当前契约判定可写性 | 打开玩法系统界面（尘歌壶内猫尾酒馆内） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:230 |
| setting.keyBindingsConfig.openWishScreen | 保留；当前契约判定可写性 | 打开祈愿界面 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:212 |
| setting.keyBindingsConfig.pickUpOrInteract | 保留；当前契约判定可写性 | 拾取/交互（自动拾取由AutoPick模块管理） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:106 |
| setting.keyBindingsConfig.questNavigation | 保留；当前契约判定可写性 | 开启任务追踪 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:124 |
| setting.keyBindingsConfig.quickUseGadget | 保留；当前契约判定可写性 | 快捷使用小道具 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:112 |
| setting.keyBindingsConfig.shortcutWheel | 保留；当前契约判定可写性 | 呼出快捷轮盘 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:166 |
| setting.keyBindingsConfig.showCursor | 保留；当前契约判定可写性 | 呼出鼠标 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:284 |
| setting.keyBindingsConfig.sprintKeyboard | 保留；当前契约判定可写性 | 冲刺（键盘） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:76 |
| setting.keyBindingsConfig.sprintMouse | 保留；当前契约判定可写性 | 冲刺（鼠标） | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:82 |
| setting.keyBindingsConfig.switchAimingMode | 保留；当前契约判定可写性 | 切换瞄准模式 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:88 |
| setting.keyBindingsConfig.switchMember1 | 保留；当前契约判定可写性 | 切换小队角色1 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:136 |
| setting.keyBindingsConfig.switchMember2 | 保留；当前契约判定可写性 | 切换小队角色2 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:142 |
| setting.keyBindingsConfig.switchMember3 | 保留；当前契约判定可写性 | 切换小队角色3 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:148 |
| setting.keyBindingsConfig.switchMember4 | 保留；当前契约判定可写性 | 切换小队角色4 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:154 |
| setting.keyBindingsConfig.switchMember5 | 保留；当前契约判定可写性 | 切换小队角色5 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:160 |
| setting.keyBindingsConfig.switchToWalkOrRun | 保留；当前契约判定可写性 | 切换走/跑；特定操作模式下向下移动 | KeyId | BetterGenshinImpact/Core/Config/KeyBindingsConfig.cs:52 |
| setting.macroConfig.combatMacroEnabled | 保留；当前契约判定可写性 | 一键战斗宏启用状态 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:57 |
| setting.macroConfig.combatMacroHotkeyMode | 保留；当前契约判定可写性 | 一键战斗宏快捷键模式 | string | BetterGenshinImpact/Core/Config/MacroConfig.cs:63 |
| setting.macroConfig.combatMacroPriority | 保留；当前契约判定可写性 | 一键战斗宏优先级 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:69 |
| setting.macroConfig.enhanceWaitDelay | 保留；当前契约判定可写性 | 高延迟下强化的额外等待时间 https://github.com/babalae/better-genshin-impact/issues/9 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:15 |
| setting.macroConfig.fFireInterval | 保留；当前契约判定可写性 | F连发时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:21 |
| setting.macroConfig.fPressHoldToContinuationEnabled | 保留；当前契约判定可写性 | 长按F变F连发 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:27 |
| setting.macroConfig.oneKeyClaimRewardHotkeyMode | 保留；当前契约判定可写性 | 一键领取奖励快捷键模式 | string | BetterGenshinImpact/Core/Config/MacroConfig.cs:75 |
| setting.macroConfig.oneKeyClaimRewardScrollDownAmount | 保留；当前契约判定可写性 | 一键领取奖励滚轮下滑幅度 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:87 |
| setting.macroConfig.oneKeyClaimRewardScrollDownEnabled | 保留；当前契约判定可写性 | 一键领取奖励未找到领取图标时滚轮下滑 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:81 |
| setting.macroConfig.runaroundInterval | 保留；当前契约判定可写性 | 转圈圈时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:33 |
| setting.macroConfig.runaroundMouseXInterval | 保留；当前契约判定可写性 | 转圈圈鼠标右移长度 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:39 |
| setting.macroConfig.spaceFireInterval | 保留；当前契约判定可写性 | 空格连发时间间隔 | int | BetterGenshinImpact/Core/Config/MacroConfig.cs:45 |
| setting.macroConfig.spacePressHoldToContinuationEnabled | 保留；当前契约判定可写性 | 长按空格变空格连发 | bool | BetterGenshinImpact/Core/Config/MacroConfig.cs:51 |
| setting.mapMaskConfig.enabled | 保留；当前契约判定可写性 | 是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:20 |
| setting.mapMaskConfig.hoYoLabLanguage | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:46 |
| setting.mapMaskConfig.mapPointApiProvider | 保留；当前契约判定可写性 | — | MapPointApiProvider | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:39 |
| setting.mapMaskConfig.miniMapMaskEnabled | 保留；当前契约判定可写性 | 小地图遮罩是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:26 |
| setting.mapMaskConfig.pathAutoRecordEnabled | 保留；当前契约判定可写性 | 自动记录路径功能是否启用 | bool | BetterGenshinImpact/GameTask/MapMask/MapMaskConfig.cs:32 |
| setting.maskWindowConfig.crosshairColor | 保留；当前契约判定可写性 | 准星颜色（十六进制） | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:115 |
| setting.maskWindowConfig.crosshairEnabled | 保留；当前契约判定可写性 | 准星是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:103 |
| setting.maskWindowConfig.crosshairGap | 保留；当前契约判定可写性 | 中心点与十字线的间隔（仅 DotCrosshair 类型） | int；有变更钩子 | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:133 |
| setting.maskWindowConfig.crosshairImagePath | 保留；当前契约判定可写性 | 自定义准星图片路径 | string? | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:166 |
| setting.maskWindowConfig.crosshairLineWidth | 保留；当前契约判定可写性 | 准星线宽 | int；有变更钩子 | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:121 |
| setting.maskWindowConfig.crosshairScaleMode | 保留；当前契约判定可写性 | 自定义图片缩放方式 | CrosshairScaleMode | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:172 |
| setting.maskWindowConfig.crosshairSize | 保留；当前契约判定可写性 | 准星大小 | int；有变更钩子 | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:127 |
| setting.maskWindowConfig.crosshairType | 保留；当前契约判定可写性 | 准星类型 | CrosshairType | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:109 |
| setting.maskWindowConfig.customHtmlMaskAutoReloadOnSave | 保留；当前契约判定可写性 | 保存后自动刷新 HTML：开启后保存 HTML 会立即刷新已经打开的自定义遮罩。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:430 |
| setting.maskWindowConfig.customHtmlMaskClickThrough | 保留；当前契约判定可写性 | HTML 遮罩鼠标穿透：开启后鼠标可穿透遮罩；关闭后 HTML 内容可以接收点击和输入。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:427 |
| setting.maskWindowConfig.customHtmlMaskEnabled | 保留；当前契约判定可写性 | — | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:424 |
| setting.maskWindowConfig.directionFontSize | 保留；当前契约判定可写性 | 方位文字大小：小地图方位文字字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:388 |
| setting.maskWindowConfig.directionShadowBlurRadius | 保留；当前契约判定可写性 | 方位文字阴影模糊半径：方位文字阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:400 |
| setting.maskWindowConfig.directionShadowColor | 保留；当前契约判定可写性 | 方位文字阴影颜色：方位文字阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:394 |
| setting.maskWindowConfig.directionShadowEnabled | 保留；当前契约判定可写性 | 显示方位文字阴影：开启后方位文字在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:391 |
| setting.maskWindowConfig.directionShadowOpacity | 保留；当前契约判定可写性 | 方位文字阴影透明度：方位文字阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:397 |
| setting.maskWindowConfig.directionTextColor | 保留；当前契约判定可写性 | 方位文字颜色：小地图周围方位文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:385 |
| setting.maskWindowConfig.directionsEnabled | 保留；当前契约判定可写性 | 方位提示是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:56 |
| setting.maskWindowConfig.displayRecognitionResultsOnMask | 保留；当前契约判定可写性 | 是否在遮罩窗口上显示识别结果 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:62 |
| setting.maskWindowConfig.logFontFamily | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:298 |
| setting.maskWindowConfig.logFontScale | 保留；当前契约判定可写性 | 遮罩 UI 缩放率 (0.5-3.0)，叠加到日志、状态和 FPS 的基础字号上。 | double；有变更钩子 | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:205 |
| setting.maskWindowConfig.logFontSize | 保留；当前契约判定可写性 | 日志文字大小：日志内容字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:301 |
| setting.maskWindowConfig.logPanelBackgroundColor | 保留；当前契约判定可写性 | 日志区域背景色：日志窗口底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:286 |
| setting.maskWindowConfig.logPanelBorderColor | 保留；当前契约判定可写性 | 日志区域边框颜色：日志窗口边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:289 |
| setting.maskWindowConfig.logPanelBorderThickness | 保留；当前契约判定可写性 | 日志区域边框粗细：日志窗口边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:292 |
| setting.maskWindowConfig.logShadowBlurRadius | 保留；当前契约判定可写性 | 日志阴影模糊半径：日志阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:313 |
| setting.maskWindowConfig.logShadowColor | 保留；当前契约判定可写性 | 日志阴影颜色：日志阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:307 |
| setting.maskWindowConfig.logShadowEnabled | 保留；当前契约判定可写性 | 显示日志阴影：开启后文字和区域更容易从游戏背景中区分出来。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:304 |
| setting.maskWindowConfig.logShadowOpacity | 保留；当前契约判定可写性 | 日志阴影透明度：日志阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:310 |
| setting.maskWindowConfig.logTextBoxHeightRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:445 |
| setting.maskWindowConfig.logTextBoxLeftRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:436 |
| setting.maskWindowConfig.logTextBoxTopRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:439 |
| setting.maskWindowConfig.logTextBoxWidthRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:442 |
| setting.maskWindowConfig.logTextColor | 保留；当前契约判定可写性 | 日志文字颜色：日志内容的文字颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:295 |
| setting.maskWindowConfig.maskEnabled | 保留；当前契约判定可写性 | 是否启用遮罩窗口 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:68 |
| setting.maskWindowConfig.metricsFontFamily | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:358 |
| setting.maskWindowConfig.metricsFontScale | 保留；当前契约判定可写性 | 指标栏缩放率 (0.5-3.0)，叠加到指标栏的基础字号和布局尺寸上。 独立于遮罩 UI 缩放率。 | double；有变更钩子 | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:212 |
| setting.maskWindowConfig.metricsFontSize | 保留；当前契约判定可写性 | 指标文字大小：指标栏字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:361 |
| setting.maskWindowConfig.metricsHeightRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:469 |
| setting.maskWindowConfig.metricsItemWidth | 保留；当前契约判定可写性 | 单个指标项宽度：每个指标项占用的宽度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:367 |
| setting.maskWindowConfig.metricsLeftRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:460 |
| setting.maskWindowConfig.metricsLineHeight | 保留；当前契约判定可写性 | 指标单行高度：每一行指标占用的高度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:364 |
| setting.maskWindowConfig.metricsNameColumnWidth | 保留；当前契约判定可写性 | 指标名称列宽度：指标名称这一列的宽度。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:370 |
| setting.maskWindowConfig.metricsPanelBackgroundColor | 保留；当前契约判定可写性 | 指标栏背景色：指标栏底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:346 |
| setting.maskWindowConfig.metricsPanelBorderColor | 保留；当前契约判定可写性 | 指标栏边框颜色：指标栏边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:349 |
| setting.maskWindowConfig.metricsPanelBorderThickness | 保留；当前契约判定可写性 | 指标栏边框粗细：指标栏边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:352 |
| setting.maskWindowConfig.metricsShadowBlurRadius | 保留；当前契约判定可写性 | 指标栏阴影模糊半径：指标栏阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:382 |
| setting.maskWindowConfig.metricsShadowColor | 保留；当前契约判定可写性 | 指标栏阴影颜色：指标栏阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:376 |
| setting.maskWindowConfig.metricsShadowEnabled | 保留；当前契约判定可写性 | 显示指标栏阴影：开启后指标栏在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:373 |
| setting.maskWindowConfig.metricsShadowOpacity | 保留；当前契约判定可写性 | 指标栏阴影透明度：指标栏阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:379 |
| setting.maskWindowConfig.metricsTextColor | 保留；当前契约判定可写性 | 指标文字颜色：指标文字颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:355 |
| setting.maskWindowConfig.metricsTopRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:463 |
| setting.maskWindowConfig.metricsWidthRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:466 |
| setting.maskWindowConfig.overlayLayoutEditEnabled | 保留；当前契约判定可写性 | 启用拖拽调整位置大小；开启后可以拖拽调整日志、状态栏与指标栏位置，并调整大小 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:433 |
| setting.maskWindowConfig.overlayMetricItems | 保留；当前契约判定可写性 | 配置文件里使用 string key 便于兼容旧版本，读取后由 EnsureOverlayMetricItems 约束回固定枚举集合。 | Dictionary<string, bool> | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:188 |
| setting.maskWindowConfig.overlayScalingEnabled | 保留；当前契约判定可写性 | 是否启用遮罩 UI 缩放。关闭后直接使用各遮罩元素的基础尺寸。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:199 |
| setting.maskWindowConfig.overlayWindowBackgroundColor | 保留；当前契约判定可写性 | 主遮罩窗口背景色：遮罩窗口本身的背景色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:280 |
| setting.maskWindowConfig.recognitionLineStrokeColor | 保留；当前契约判定可写性 | 识别线条颜色：开启统一颜色后，识别线条使用的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:412 |
| setting.maskWindowConfig.recognitionLineStrokeThickness | 保留；当前契约判定可写性 | 识别线条线宽：开启统一颜色后，识别线条的线宽。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:415 |
| setting.maskWindowConfig.recognitionRectStrokeColor | 保留；当前契约判定可写性 | 识别矩形边框颜色：开启统一颜色后，识别矩形框使用的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:406 |
| setting.maskWindowConfig.recognitionRectStrokeThickness | 保留；当前契约判定可写性 | 识别矩形线宽：开启统一颜色后，识别矩形框的线宽。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:409 |
| setting.maskWindowConfig.recognitionTextColor | 保留；当前契约判定可写性 | 识别文字颜色：普通识别结果文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:418 |
| setting.maskWindowConfig.recognitionTextFontSize | 保留；当前契约判定可写性 | 识别文字大小：普通识别结果文字的基础字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:421 |
| setting.maskWindowConfig.recognitionUseDrawableStyle | 保留；当前契约判定可写性 | 统一识别框线颜色：关闭时保留任务自己指定的颜色；开启后使用下面设置的统一颜色。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:403 |
| setting.maskWindowConfig.showFps | 保留；当前契约判定可写性 | 显示FPS | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:178 |
| setting.maskWindowConfig.showLogBox | 保留；当前契约判定可写性 | // 显示遮罩窗口边框 // | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:79 |
| setting.maskWindowConfig.showOverlayMetrics | 保留；当前契约判定可写性 | 显示遮罩指标栏 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:184 |
| setting.maskWindowConfig.showStatus | 保留；当前契约判定可写性 | 显示状态指示 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:85 |
| setting.maskWindowConfig.statusDisabledTextColor | 保留；当前契约判定可写性 | 未启用状态文字颜色：任务未启用时图标和文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:325 |
| setting.maskWindowConfig.statusEnabledTextColor | 保留；当前契约判定可写性 | 已启用状态文字颜色：任务启用时图标和文字的颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:328 |
| setting.maskWindowConfig.statusFontSize | 保留；当前契约判定可写性 | 状态文字大小：状态栏字号。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:331 |
| setting.maskWindowConfig.statusListHeightRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:457 |
| setting.maskWindowConfig.statusListLeftRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:448 |
| setting.maskWindowConfig.statusListTopRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:451 |
| setting.maskWindowConfig.statusListWidthRatio | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:454 |
| setting.maskWindowConfig.statusPanelBackgroundColor | 保留；当前契约判定可写性 | 任务状态栏背景色：状态栏底色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:316 |
| setting.maskWindowConfig.statusPanelBorderColor | 保留；当前契约判定可写性 | 任务状态栏边框颜色：状态栏边框颜色。边框粗细为 0 时不会显示边框。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:319 |
| setting.maskWindowConfig.statusPanelBorderThickness | 保留；当前契约判定可写性 | 任务状态栏边框粗细：状态栏边框线宽。填 0 表示不显示边框。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:322 |
| setting.maskWindowConfig.statusShadowBlurRadius | 保留；当前契约判定可写性 | 状态栏阴影模糊半径：状态栏阴影的扩散范围，数值越大阴影越柔和。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:343 |
| setting.maskWindowConfig.statusShadowColor | 保留；当前契约判定可写性 | 状态栏阴影颜色：状态栏阴影颜色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:337 |
| setting.maskWindowConfig.statusShadowEnabled | 保留；当前契约判定可写性 | 显示状态栏阴影：开启后状态栏在复杂背景上更容易看清。 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:334 |
| setting.maskWindowConfig.statusShadowOpacity | 保留；当前契约判定可写性 | 状态栏阴影透明度：状态栏阴影强度，0 表示没有阴影，1 表示最明显。 | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:340 |
| setting.maskWindowConfig.textOpacity | 保留；当前契约判定可写性 | 遮罩文本透明度 (0.0-1.0) | double | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:193 |
| setting.maskWindowConfig.uidCoverEnabled | 保留；当前契约判定可写性 | UID遮盖是否启用 | bool | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:91 |
| setting.maskWindowConfig.wineOverlayBackgroundColor | 保留；当前契约判定可写性 | Wine 兼容背景色：仅在 Wine 环境下使用的兼容背景色。 | string | BetterGenshinImpact/Core/Config/MaskWindowConfig.cs:283 |
| setting.musicConfig.autoSwitchInstrument | 保留；当前契约判定可写性 | 演奏前是否自动切换到当前曲目的输出乐器 | bool | BetterGenshinImpact/Core/Config/MusicConfig.cs:48 |
| setting.musicConfig.customBpm | 保留；当前契约判定可写性 | 自定义 BPM | double | BetterGenshinImpact/Core/Config/MusicConfig.cs:42 |
| setting.musicConfig.inputMode | 保留；当前契约判定可写性 | 默认使用后台 PostMessage 演奏 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:18 |
| setting.musicConfig.musicFolder | 保留；当前契约判定可写性 | 曲谱扫描根目录 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:12 |
| setting.musicConfig.playbackMode | 保留；当前契约判定可写性 | 播放模式 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:24 |
| setting.musicConfig.selectedInstrumentProfile | 保留；当前契约判定可写性 | 默认输出乐器档案 | string | BetterGenshinImpact/Core/Config/MusicConfig.cs:54 |
| setting.musicConfig.speed | 保留；当前契约判定可写性 | 播放速度 | double | BetterGenshinImpact/Core/Config/MusicConfig.cs:30 |
| setting.musicConfig.useCustomBpm | 保留；当前契约判定可写性 | 是否使用自定义 BPM 覆盖曲谱的基准速度 | bool | BetterGenshinImpact/Core/Config/MusicConfig.cs:36 |
| setting.nextScheduledTask | 保留；当前契约判定可写性 | /// <summary> /// 推理使用的设备 /// </summary> [ObservableProperty] private string _inferenceDevice = "CPU"; | List<ValueTuple<string, int, string, string>> | BetterGenshinImpact/Core/Config/AllConfig.cs:98 |
| setting.notShowNewVersionNoticeEndVersion | 保留；当前契约判定可写性 | 不展示新版本提示的最新版本 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:55 |
| setting.notificationConfig.barkAction | 保留；当前契约判定可写性 | 传"none"时，点击推送不会弹窗 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:20 |
| setting.notificationConfig.barkApiEndpoint | 保留；当前契约判定可写性 | Bark API 端点；填写 Bark API 端点，例如：api.day.app | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:22 |
| setting.notificationConfig.barkAutoCopy | 保留；当前契约判定可写性 | iOS14.5以下自动复制推送内容，1为开启 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:27 |
| setting.notificationConfig.barkBadge | 保留；当前契约判定可写性 | 推送角标，可以是任意数字 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:32 |
| setting.notificationConfig.barkCall | 保留；当前契约判定可写性 | 通知铃声重复播放，1为开启 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:37 |
| setting.notificationConfig.barkCiphertext | 保留；当前契约判定可写性 | 加密密钥；推送内容加密密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:39 |
| setting.notificationConfig.barkCopy | 保留；当前契约判定可写性 | 复制推送时指定复制的内容 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:44 |
| setting.notificationConfig.barkDeviceKeys | 保留；当前契约判定可写性 | 设备 Key；多个设备使用英文逗号、分号或空格分隔 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:46 |
| setting.notificationConfig.barkGroup | 保留；当前契约判定可写性 | 对消息进行分组，推送将按group分组显示在通知中心中 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:51 |
| setting.notificationConfig.barkIcon | 保留；当前契约判定可写性 | 为推送设置自定义图标URL | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:56 |
| setting.notificationConfig.barkIsArchive | 保留；当前契约判定可写性 | 传1保存推送，传其他的不保存推送 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:61 |
| setting.notificationConfig.barkLevel | 保留；当前契约判定可写性 | 推送中断级别：critical(重要警告), active(默认值), timeSensitive(时效性通知), passive(仅添加到通知列表) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:66 |
| setting.notificationConfig.barkNotificationEnabled | 保留；当前契约判定可写性 | Bark移动推送通知配置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:71 |
| setting.notificationConfig.barkSound | 保留；当前契约判定可写性 | 通知声音 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:76 |
| setting.notificationConfig.barkSubtitle | 保留；当前契约判定可写性 | 推送副标题 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:83 |
| setting.notificationConfig.barkUrl | 保留；当前契约判定可写性 | 点击推送时跳转的URL | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:88 |
| setting.notificationConfig.barkVolume | 保留；当前契约判定可写性 | 重要警告的通知音量，取值范围: 0-10 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:93 |
| setting.notificationConfig.dingDingSecret | 保留；当前契约判定可写性 | 钉钉Webhook密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:98 |
| setting.notificationConfig.dingDingwebhookNotificationEnabled | 保留；当前契约判定可写性 | dindin 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:103 |
| setting.notificationConfig.dingdingWebhookUrl | 保留；当前契约判定可写性 | 钉钉Webhook地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:108 |
| setting.notificationConfig.discordWebhookAvatarUrl | 保留；当前契约判定可写性 | Discord Webhook头像地址 Default url from https://www.bettergi.com/ | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:278 |
| setting.notificationConfig.discordWebhookImageEncoder | 保留；当前契约判定可写性 | 截图编码；PNG 无损，JPEG 快速压缩，WebP 档案小，按需选择 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:283 |
| setting.notificationConfig.discordWebhookNotificationEnabled | 保留；当前契约判定可写性 | Discord Webhook推送通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:262 |
| setting.notificationConfig.discordWebhookUrl | 保留；当前契约判定可写性 | Discord Webhook地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:267 |
| setting.notificationConfig.discordWebhookUsername | 保留；当前契约判定可写性 | Discord Webhook用户名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:272 |
| setting.notificationConfig.dragonEndSummaryEnabled | 保留；当前契约判定可写性 | 一条龙结束后发送汇总通知（体力+委托奖励，附拼接截图） | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:113 |
| setting.notificationConfig.emailNotificationEnabled | 保留；当前契约判定可写性 | Email 通知配置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:116 |
| setting.notificationConfig.feishuAppId | 保留；当前契约判定可写性 | 飞书AppId；若填写AppId、AppSecret则发送图片 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:152 |
| setting.notificationConfig.feishuAppSecret | 保留；当前契约判定可写性 | 飞书AppSecret；若填写AppId、AppSecret则发送图片 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:153 |
| setting.notificationConfig.feishuNotificationEnabled | 保留；当前契约判定可写性 | 飞书通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:144 |
| setting.notificationConfig.feishuWebhookUrl | 保留；当前契约判定可写性 | 飞书通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:150 |
| setting.notificationConfig.fromEmail | 保留；当前契约判定可写性 | 发件人邮箱；填写发件人邮箱 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:118 |
| setting.notificationConfig.fromName | 保留；当前契约判定可写性 | 发件人姓名；填写发件人姓名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:120 |
| setting.notificationConfig.gotifyAppToken | 保留；当前契约判定可写性 | Gotify服务APP Token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:323 |
| setting.notificationConfig.gotifyNotificationEnabled | 保留；当前契约判定可写性 | Gotify通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:313 |
| setting.notificationConfig.gotifyNotifyLevel | 保留；当前契约判定可写性 | Gotify通知优先级 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:328 |
| setting.notificationConfig.gotifyUrl | 保留；当前契约判定可写性 | Gotify服务地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:318 |
| setting.notificationConfig.includeScreenShot | 保留；当前契约判定可写性 | 是否包含截图 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:126 |
| setting.notificationConfig.jsNotificationEnabled | 保留；当前契约判定可写性 | 是否允许 js 发送通知 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:15 |
| setting.notificationConfig.meowNickname | 保留；当前契约判定可写性 | MeoW用户昵称 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:303 |
| setting.notificationConfig.meowNotificationEnabled | 保留；当前契约判定可写性 | MeoW通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:298 |
| setting.notificationConfig.meowTitle | 保留；当前契约判定可写性 | MeoW消息标题（可选，路径参数） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:308 |
| setting.notificationConfig.notificationEventSubscribe | 保留；当前契约判定可写性 | — | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:129 |
| setting.notificationConfig.oneBotEndpoint | 保留；当前契约判定可写性 | OneBot通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:166 |
| setting.notificationConfig.oneBotGroupId | 保留；当前契约判定可写性 | 群号；填写接收消息的群号 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:169 |
| setting.notificationConfig.oneBotNotificationEnabled | 保留；当前契约判定可写性 | OneBot通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:160 |
| setting.notificationConfig.oneBotToken | 保留；当前契约判定可写性 | Token；填写 OneBot Token（可选） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:170 |
| setting.notificationConfig.oneBotUserId | 保留；当前契约判定可写性 | QQ 号；填写接收消息的 QQ 号 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:168 |
| setting.notificationConfig.qqAppId | 保留；当前契约判定可写性 | QQ开放平台 AppID | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:338 |
| setting.notificationConfig.qqClientSecret | 保留；当前契约判定可写性 | QQ开放平台 AppSecret | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:343 |
| setting.notificationConfig.qqGroupOpenId | 保留；当前契约判定可写性 | 群聊 OpenID（群聊场景） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:353 |
| setting.notificationConfig.qqNotificationEnabled | 保留；当前契约判定可写性 | QQ通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:333 |
| setting.notificationConfig.qqOpenId | 保留；当前契约判定可写性 | 用户的 C2C OpenID（单聊场景） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:348 |
| setting.notificationConfig.serverChanNotificationEnabled | 保留；当前契约判定可写性 | ServerChan通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:288 |
| setting.notificationConfig.serverChanSendKey | 保留；当前契约判定可写性 | ServerChan SendKey | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:293 |
| setting.notificationConfig.smtpPassword | 保留；当前契约判定可写性 | SMTP 密码；填写 SMTP 密码 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:131 |
| setting.notificationConfig.smtpPort | 保留；当前契约判定可写性 | SMTP 服务器端口；填写 SMTP 服务器端口，一般为：587 | int | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:133 |
| setting.notificationConfig.smtpServer | 保留；当前契约判定可写性 | SMTP 服务器；填写 SMTP 服务器 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:135 |
| setting.notificationConfig.smtpUsername | 保留；当前契约判定可写性 | SMTP 用户名；填写 SMTP 用户名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:137 |
| setting.notificationConfig.telegramApiBaseUrl | 保留；当前契约判定可写性 | Telegram API基础URL(可选，留空使用官方API) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:175 |
| setting.notificationConfig.telegramBotToken | 保留；当前契约判定可写性 | Telegram机器人Token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:190 |
| setting.notificationConfig.telegramChatId | 保留；当前契约判定可写性 | Telegram聊天ID | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:195 |
| setting.notificationConfig.telegramNotificationEnabled | 保留；当前契约判定可写性 | Telegram通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:201 |
| setting.notificationConfig.telegramProxyEnabled | 保留；当前契约判定可写性 | 是否启用Telegram代理 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:185 |
| setting.notificationConfig.telegramProxyUrl | 保留；当前契约判定可写性 | Telegram代理地址(可选，格式：http://127.0.0.1:7890) | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:180 |
| setting.notificationConfig.toEmail | 保留；当前契约判定可写性 | 收件人邮箱；填写收件人邮箱 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:203 |
| setting.notificationConfig.webSocketEndpoint | 保留；当前契约判定可写性 | WebSocket 端点；填写 WebSocket 端点 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:216 |
| setting.notificationConfig.webSocketNotificationEnabled | 保留；当前契约判定可写性 | 启用 WebSocket；WebSocket 相关设置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:218 |
| setting.notificationConfig.webhookEnabled | 保留；当前契约判定可写性 | 启用 Webhook；Webhook 相关设置 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:207 |
| setting.notificationConfig.webhookEndpoint | 保留；当前契约判定可写性 | Webhook 端点；填写 Webhook 端点 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:211 |
| setting.notificationConfig.webhookSendTo | 保留；当前契约判定可写性 | 修改属性名 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:214 |
| setting.notificationConfig.wechatClawbotBaseUrl | 保留；当前契约判定可写性 | 微信 Clawbot API 基础地址（登录响应 baseurl；为空时使用默认 https://ilinkai.weixin.qq.com） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:373 |
| setting.notificationConfig.wechatClawbotBotToken | 保留；当前契约判定可写性 | 微信 Clawbot 扫码登录获得的 bot_token | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:363 |
| setting.notificationConfig.wechatClawbotNotificationEnabled | 保留；当前契约判定可写性 | 微信 Clawbot 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:358 |
| setting.notificationConfig.wechatClawbotToUserId | 保留；当前契约判定可写性 | 微信 Clawbot 推送目标用户 ID（用户给机器人发消息后自动获取） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:368 |
| setting.notificationConfig.windowsUwpNotificationEnabled | 保留；当前契约判定可写性 | windows uwp 通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:223 |
| setting.notificationConfig.workweixinNotificationEnabled | 保留；当前契约判定可写性 | 企业微信通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:230 |
| setting.notificationConfig.workweixinWebhookUrl | 保留；当前契约判定可写性 | 企业微信通知通知地址 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:236 |
| setting.notificationConfig.xxtuiApiKey | 保留；当前契约判定可写性 | xx信息推送通知API密钥 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:242 |
| setting.notificationConfig.xxtuiChannels | 保留；当前契约判定可写性 | xx信息推送通知渠道（WX_MP,WX_QY_ROBOT,DING_ROBOT,BARK） | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:247 |
| setting.notificationConfig.xxtuiFrom | 保留；当前契约判定可写性 | xx信息推送通知来源 | string | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:252 |
| setting.notificationConfig.xxtuiNotificationEnabled | 保留；当前契约判定可写性 | 信息推送通知是否启用 | bool | BetterGenshinImpact/Service/Notification/NotificationConfig.cs:257 |
| setting.otherConfig.autoFetchDispatchAdventurersGuildCountry | 保留；当前契约判定可写性 | 自动领取派遣任务城市 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:20 |
| setting.otherConfig.autoRestartConfig | 保留；当前契约判定可写性 | — | AutoRestart | BetterGenshinImpact/Core/Config/OtherConfig.cs:28 |
| setting.otherConfig.farmingPlanConfig | 保留；当前契约判定可写性 | 锄地规划 | FarmingPlan | BetterGenshinImpact/Core/Config/OtherConfig.cs:31 |
| setting.otherConfig.gameCultureInfoName | 保留；当前契约判定可写性 | 游戏语言名称 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:123 |
| setting.otherConfig.itemIconRecognitionMode | 保留；当前契约判定可写性 | 物品图标识别模型 | ItemIconRecognitionMode | BetterGenshinImpact/Core/Config/OtherConfig.cs:26 |
| setting.otherConfig.miyousheConfig | 保留；当前契约判定可写性 | — | Miyoushe | BetterGenshinImpact/Core/Config/OtherConfig.cs:34 |
| setting.otherConfig.ocrConfig | 保留；当前契约判定可写性 | OCR配置 | Ocr | BetterGenshinImpact/Core/Config/OtherConfig.cs:37 |
| setting.otherConfig.restoreFocusOnLostEnabled | 保留；当前契约判定可写性 | 调度器任务和部分独立任务，失去焦点，自动激活游戏窗口 | bool | BetterGenshinImpact/Core/Config/OtherConfig.cs:14 |
| setting.otherConfig.serverTimeZoneOffset | 保留；当前契约判定可写性 | 服务器时区偏移量 | TimeSpan | BetterGenshinImpact/Core/Config/OtherConfig.cs:23 |
| setting.otherConfig.uiCultureInfoName | 保留；当前契约判定可写性 | BGI界面语言名称 | string | BetterGenshinImpact/Core/Config/OtherConfig.cs:129 |
| setting.otherConfig.windowClassDetectPreferred | 保留；当前契约判定可写性 | 窗口类名优先检测：原版仅靠进程名+MainWindowHandle 查找游戏窗口，该句柄为 0 或指向错误窗口时会找不到。开启后优先按窗口类名枚举检测，未命中再按进程枚举取客户区最大的可见窗口，仍不命中才回退原版方式。即时生效（每次查找窗口时读取），默认关闭，关闭时行为与旧版完全一致 | bool | BetterGenshinImpact/Core/Config/OtherConfig.cs:17 |
| setting.pathingConditionConfig.autoEatEnabled | 保留；当前契约判定可写性 | 启用自动吃药功能 | bool | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:55 |
| setting.pathingConditionConfig.avatarConditions | 保留；当前契约判定可写性 | — | ObservableCollection<Condition> | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:26 |
| setting.pathingConditionConfig.mapMatchingMethod | 保留；当前契约判定可写性 | 地图追踪优先使用的特征匹配方式；影响所有地图追踪功能，重启后生效 | string | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:19 |
| setting.pathingConditionConfig.onlyInTeleportRecover | 保留；当前契约判定可写性 | 只在传送传送点时复活 | bool | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:30 |
| setting.pathingConditionConfig.partyConditions | 保留；当前契约判定可写性 | 地图追踪条件配置 | ObservableCollection<Condition> | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:23 |
| setting.pathingConditionConfig.recoverTiming | 保留；当前契约判定可写性 | 低血量回复时机；选择低血量时的回复策略：任何路径点/只在传送点/不回复 | RecoverTiming | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:36 |
| setting.pathingConditionConfig.useGadgetIntervalMs | 保留；当前契约判定可写性 | 使用小道具的间隔时间(ms) | int | BetterGenshinImpact/Core/Config/PathingConditionConfig.cs:51 |
| setting.quickTeleportConfig.enabled | 保留；当前契约判定可写性 | 快速传送是否启用 | bool | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:15 |
| setting.quickTeleportConfig.hotkeyTpEnabled | 保留；当前契约判定可写性 | 使用快捷键传送 | bool | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:31 |
| setting.quickTeleportConfig.teleportListClickDelay | 保留；当前契约判定可写性 | 点击候选列表传送点的间隔时间(ms) | int | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:20 |
| setting.quickTeleportConfig.waitTeleportPanelDelay | 保留；当前契约判定可写性 | 等待右侧传送弹出界面的时间(ms) 0.24 版本后，这个值可以设置为 0，因为识图时间变久了。0.24 版本前，建议设置为 100 | int | BetterGenshinImpact/GameTask/QuickTeleport/QuickTeleportConfig.cs:26 |
| setting.recordConfig.angle2DirectInputX | 保留；当前契约判定可写性 | 视角每移动1度，需要DirectInput移动的单位 | double | BetterGenshinImpact/Core/Config/RecordConfig.cs:19 |
| setting.recordConfig.angle2MouseMoveByX | 保留；当前契约判定可写性 | 视角每移动1度，需要MouseMoveBy的距离 用作脚本记录度数后转化的鼠标移动距离 | double | BetterGenshinImpact/Core/Config/RecordConfig.cs:13 |
| setting.recordConfig.isRecordCameraOrientation | 保留；当前契约判定可写性 | 图像识别记录相机视角朝向 | bool | BetterGenshinImpact/Core/Config/RecordConfig.cs:25 |
| setting.scriptConfig.autoUpdateBeforeCommandLineRun | 保留；当前契约判定可写性 | 命令行启动时是否先自动更新已订阅脚本再执行命令 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:58 |
| setting.scriptConfig.autoUpdateScriptRepoPeriod | 保留；当前契约判定可写性 | 自动更新脚本仓库周期（天） | int | BetterGenshinImpact/Core/Config/ScriptConfig.cs:15 |
| setting.scriptConfig.autoUpdateSubscribedScripts | 保留；当前契约判定可写性 | 是否在启动时自动更新已订阅的脚本 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:55 |
| setting.scriptConfig.customRepoUrl | 保留；当前契约判定可写性 | 自定义渠道的URL | string | BetterGenshinImpact/Core/Config/ScriptConfig.cs:34 |
| setting.scriptConfig.guideStatus | 保留；当前契约判定可写性 | 仓库新手教程是否已阅读 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:52 |
| setting.scriptConfig.lastUpdateScriptRepoTime | 保留；当前契约判定可写性 | 上次更新脚本仓库时间 | DateTime | BetterGenshinImpact/Core/Config/ScriptConfig.cs:19 |
| setting.scriptConfig.scriptRepoHintDotVisible | 保留；当前契约判定可写性 | 脚本仓库按钮红点是否展示 | bool | BetterGenshinImpact/Core/Config/ScriptConfig.cs:23 |
| setting.scriptConfig.selectedChannelName | 保留；当前契约判定可写性 | 选择的更新渠道名称 | string | BetterGenshinImpact/Core/Config/ScriptConfig.cs:31 |
| setting.scriptConfig.subscribedScriptPaths | 保留；当前契约判定可写性 | 已订阅的脚本路径列表 | List<string> | BetterGenshinImpact/Core/Config/ScriptConfig.cs:27 |
| setting.scriptConfig.webviewHeight | 保留；当前契约判定可写性 | 仓库页面高度 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:40 |
| setting.scriptConfig.webviewLeft | 保留；当前契约判定可写性 | 仓库页面横向位置 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:43 |
| setting.scriptConfig.webviewState | 保留；当前契约判定可写性 | 仓库页面是否最大化 | WindowState | BetterGenshinImpact/Core/Config/ScriptConfig.cs:49 |
| setting.scriptConfig.webviewTop | 保留；当前契约判定可写性 | 仓库页面纵向位置 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:46 |
| setting.scriptConfig.webviewWidth | 保留；当前契约判定可写性 | 仓库页面宽度 | double | BetterGenshinImpact/Core/Config/ScriptConfig.cs:37 |
| setting.selectedOneDragonFlowConfigName | 保留；当前契约判定可写性 | 一条龙选中使用的配置 | string | BetterGenshinImpact/Core/Config/AllConfig.cs:116 |
| setting.skillCdConfig.backgroundNormalColor | 保留；当前契约判定可写性 | CD大于0.8s时计时器背景色（默认白色 #FFFFFFFF） | string；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:87 |
| setting.skillCdConfig.backgroundReadyColor | 保留；当前契约判定可写性 | CD小于0.8s时计时器背景色（默认白色 #FFFFFFFF） | string；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:115 |
| setting.skillCdConfig.customCdList | 保留；当前契约判定可写性 | 特定角色CD修正配置列表 | System.Collections.Generic.List<SkillCdRule> | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:21 |
| setting.skillCdConfig.enabled | 保留；当前契约判定可写性 | 是否启用 | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:15 |
| setting.skillCdConfig.gap | 保留；当前契约判定可写性 | 计时器间隔 | double；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:63 |
| setting.skillCdConfig.hideWhenZero | 保留；当前契约判定可写性 | 冷却为0时隐藏 | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:33 |
| setting.skillCdConfig.px | 保留；当前契约判定可写性 | 横坐标 | double；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:39 |
| setting.skillCdConfig.py | 保留；当前契约判定可写性 | 纵坐标 | double；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:51 |
| setting.skillCdConfig.scale | 保留；当前契约判定可写性 | 计时器缩放 | double；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:75 |
| setting.skillCdConfig.textNormalColor | 保留；当前契约判定可写性 | CD大于0.8s时计时器文本色（默认 #DA4A23） | string；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:101 |
| setting.skillCdConfig.textReadyColor | 保留；当前契约判定可写性 | CD小于0.8s时计时器文本色（默认 #5DCC17） | string；有变更钩子 | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:129 |
| setting.skillCdConfig.triggerOnSkillUse | 保留；当前契约判定可写性 | 使用战技时触发（E键） | bool | BetterGenshinImpact/GameTask/SkillCd/SkillCdConfig.cs:27 |
| setting.tpConfig.hpRestoreDuration | 保留；当前契约判定可写性 | 回血等待时间 | double；有变更钩子 | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:112 |
| setting.tpConfig.mapDragUseRelativeMove | 保留；当前契约判定可写性 | 大地图拖动使用相对鼠标移动 | bool | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:18 |
| setting.tpConfig.mapScaleFactor | 保留；当前契约判定可写性 | 游戏坐标和 mapZoomLevel=1 时的像素比例因子。 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:150 |
| setting.tpConfig.mapZoomEnabled | 保留；当前契约判定可写性 | 地图缩放开关 | bool | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:15 |
| setting.tpConfig.mapZoomInDistance | 保留；当前契约判定可写性 | 地图放大的最大距离，单位：像素 | int；有变更钩子 | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:34 |
| setting.tpConfig.mapZoomOutDistance | 保留；当前契约判定可写性 | 地图缩小的最小距离，单位：像素 | int；有变更钩子 | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:21 |
| setting.tpConfig.maxIterations | 保留；当前契约判定可写性 | 移动最大次数 | int | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:145 |
| setting.tpConfig.maxZoomLevel | 保留；当前契约判定可写性 | 最大缩放等级 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:84 |
| setting.tpConfig.reviveStatueOfTheSeven.country | 保留；当前契约判定可写性 | 所在国家 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:35 |
| setting.tpConfig.reviveStatueOfTheSeven.id | 保留；当前契约判定可写性 | 基本属性 | string | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:32 |
| setting.tpConfig.reviveStatueOfTheSeven.name | 保留；当前契约判定可写性 | tp 名称 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:33 |
| setting.tpConfig.reviveStatueOfTheSeven.tranPosition | 保留；当前契约判定可写性 | 实际传送的坐标 | decimal[] | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:44 |
| setting.tpConfig.reviveStatueOfTheSeven.tranX | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:45 |
| setting.tpConfig.reviveStatueOfTheSeven.tranY | 保留；当前契约判定可写性 | — | double | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:46 |
| setting.tpConfig.reviveStatueOfTheSeven.type | 保留；当前契约判定可写性 | tp 类型 | string? | BetterGenshinImpact/GameTask/AutoTrackPath/Model/GiWorldPosition.cs:34 |
| setting.tpConfig.reviveStatueOfTheSevenArea | 保留；当前契约判定可写性 | 七天神像所在区域 | string | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:95 |
| setting.tpConfig.reviveStatueOfTheSevenCountry | 保留；当前契约判定可写性 | 七天神像所在国家 | string | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:98 |
| setting.tpConfig.reviveStatueOfTheSevenPointX | 保留；当前契约判定可写性 | 七天神像点位X坐标 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:89 |
| setting.tpConfig.reviveStatueOfTheSevenPointY | 保留；当前契约判定可写性 | 七天神像点位Y坐标 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:92 |
| setting.tpConfig.teleportOperationDelayMilliseconds | 保留；当前契约判定可写性 | 传送操作速度基准间隔，单位：ms | int；有变更钩子 | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:46 |
| setting.tpConfig.tolerance | 保留；当前契约判定可写性 | 允许的移动误差 | double | BetterGenshinImpact/GameTask/AutoTrackPath/TpConfig.cs:140 |
| setting.triggerInterval | 保留；当前契约判定可写性 | 触发器触发频率(ms) | int | BetterGenshinImpact/Core/Config/AllConfig.cs:61 |
| setting.wgcMinUpdateIntervalMs | 保留；当前契约判定可写性 | WGC V2 帧率上限（毫秒，即最小更新间隔，限制 DWM 推帧频率以降低 GPU 占用） 0 = 不启用限流（默认）；仅 Windows 11 24H2 及以上系统生效 | int | BetterGenshinImpact/Core/Config/AllConfig.cs:68 |
| setting.wgcV2UseCpuConvert | 保留；当前契约判定可写性 | WGC V2 使用 CPU 颜色转换（BGRA→BGR 由 CPU CvtColor 完成） 默认关闭 = GPU compute shader 打包 BGR24（回读量更小、CPU 零转换）；重启捕获后生效 | bool | BetterGenshinImpact/Core/Config/AllConfig.cs:75 |

## 脚本 API、资源字段与页面

脚本 API 和资源字段属于 JS 运行环境，不是 Agent 直接工具。完整逐项清单仍保留于 [源码功能清单](feature-coverage.md)，结构化审阅表包含全部 140 个脚本 API、35 个资源字段及 68 个页面。已弃用项目不生成 Agent 功能卡；源码盘点保留事实与删除原因。

[当前完整调用链路图](agent-chain.md)已同步删除旧补接实现。
