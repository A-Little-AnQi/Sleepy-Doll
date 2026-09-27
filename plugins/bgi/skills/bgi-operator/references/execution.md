# 任务准备、执行与核验

## 执行任务

1. “跑个 X”“运行下 X”默认把 X 当作资源目标，先调用一次 `bgi.user.resolve`，使用用户原话或目标名称。它在本机未命中时自动查当前全仓索引。不要先 `bgi.api.search`，也不要列 AutoPathing 或逐条读取路线 JSON。用户明确指定本体动作或一条龙时才走对应原生动作。
2. `verdict=run` 时直接读取 `bgi.run_script_group` 契约并传入精确 `groupName`。路径缺失时运行时会拒绝执行；不要在 `repair` 状态下调用它。
3. `verdict=repair` 时只处理 `missing` 列出的路径：更新仓库或订阅后再次 `resolve`。
3a. `resourceFound` 表示已有中央仓库证据。直接用 `repository.items`，不用重复查或刷新；采集选择一个完整作者包并核对角色前提。describe/invoke `bgi.subscribe_script_resources` 安装该目录，再 `bgi.prepare_pathing_group`，使用返回的 groupName 运行。JS 候选按定义读取参数、订阅和配置。不能逐条读、重写或重命名叶子路线 JSON。
3b. `notFound` 表示本机与当前全仓索引均未命中，核对名称后，确需更新才刷新一次再 resolve。`lookupFailed` 是仓库查询失败，处理返回的错误，不能报告资源不存在。单独使用 `bgi.repo.search` 时分类无结果也不能代表全仓不存在。
3c. `create` 的 candidates 明确区分 `Pathing` 与 `Javascript`。JS 先 inspect_script，settings 来源是 manifest 的 settingsUi；读取参数、README 与必要源码后，用 `bgi.prepare_js_group` 的 folderName/settings 建组，随后运行返回的 groupName。本机自建 JS 可以执行，不要求中央仓库同名条目。地图追踪不能套 JS 设置流程。
4. 只有准备提交执行时才调用一次 `bgi.state.get`，检查截图器、游戏句柄、任务锁和窗口状态。纯查询或文件编辑不需要状态快照。`ready` 以**进入游戏主界面**为准：截图器就绪但仍在登录或加载画面时调用会被拒绝，等 `bgi.get_status` 的 `ready=true` 再提交。`gameResolution.sixteenToNine=false` 时先把游戏或远程桌面会话调到 16:9（如 1920x1080），启动参数 `-screen-width` 对已初始化过的原神不生效。
4b. 未就绪时直接 describe/invoke `bgi.start_game`，再 `bgi.wait_ready` 一次阻塞等待（默认 120 秒，超时后报告，不由模型轮询）；不要先搜索截图器 ViewModel 命令，更不能明知未就绪仍试跑。游戏已就绪直接 describe/invoke `bgi.run_script_group`，不用另找运行命令。
4a. 目标是每日/清体力/周常一条龙时用 `bgi.run_one_dragon`（宿主原生任务链，`configName` 可选），不要为它建调度器配置组；它收尾可能自动退出游戏。用户要求退出原神或任务链收尾时用 `bgi.exit_game`（正常关闭，超时强结束），核验 `gameHandle` 回落后即完成。
5. 其他动作若已知道精确 `methodId`，直接 `bgi.api.describe`；否则只在 `command` 组按一个动作词搜索一次。
6. 契约必须同时满足：`callable=true`、参数可提供、接口确实作用于目标对象。其他低层命令仍依赖界面当前选择时，不得声称能按名称执行。
7. 配置组普通运行使用 `waitForCompletion=false`（默认），只等宿主接受启动交接，立即总结并结束对话，不等整组脚本跑完，不查询 Job、状态、日志、截图或睡眠轮询。只有用户明确要等结果、后续步骤或跟踪时才用 true。调用由运行时阻塞，期间不调用模型。返回的 verified/verificationScope=launch 只核验交接，不代表业务成功；accepted=true 写“已提交运行”，不能写“全部完成”。快捷入口用 true 跟踪运行／停止，确定性执行器不调用模型。
8. 接口不可调用时直接说明唯一阻塞项。不要继续换中英文、查插件、搜生命周期接口或让用户重复提供已经查到的信息。


## 动态命令的完整链路

- `callable=true` 仅表示契约可提交，还必须确认参数、当前选择和验证证据。稳定业务入口优先；动态命令需要对象时，describe/read `bgi.list_command_targets`，按真实 `contextId/objectId` 绑定，不能让用户手动选中来替代。
- 当前窗口或子对象尚未创建时，按返回的构造契约 invoke `bgi.create_command_target`；原服务依赖来自宿主容器。开放泛型和抽象基类使用已有具体上下文，不猜类型。新增参数对象只用 `bgi.create_command_argument` 声明的类型；不能指定任意 CLR 类型。
- `selection` 按当前 Schema 绑定 Selected 字段；多实例必须显式给 contextId。旧引用、类型不符、未匹配的构造器和缺少弹窗输入应根据错误重新取得必要证据，不能扫描 DLL 或换无关接口绕过。
- 需要输入的命令提供 `dialogInput`：名称用 text，文件／目录用 filePath，资源选择用真实 Tag 的 selectedValues，确认用明确 confirm。作用域只接管本次新建窗口，不能操作用户已有窗口。`dialogsHandled` 不是业务成功标记，修改后仍回读目标。
- 使用结束后释放桥创建的无用上下文；用户已有实例保留。空实现、弃用功能和内部 UI 事件已从公共目录移除，不能反射恢复或寻找旧教程绕过。
- 当前可见编辑器、列表行或模态窗口保留原生链路，按需读取 native-ui.md：ui.read/write/options/reorder/invoke/respond/operation 完成输入、排序、保存与终态，不因需要弹窗而删掉功能。
- 独立任务的目标参数先走 settings 读取与事务，例如首领名称、指定次数模式和次数。联动字段在 preview 的 differences 中一并核对，不能只写 runCount 却遗漏 specifyRunCount。
- 设置的 writable=false 是直接事务限制；当前可见字段可走 ui.read/write 的真实控件绑定、原生校验和保存。不通过 workspace 或磁盘全局配置绕过。
- 打开页面是明确 UI 目标；使用 bgi.list_pages/open_page。对采集、运行、删除请求，页面导航不能代替操作结果。
- 普通停止优先取消已关联的 Job；没有 Job ID 时用 bgi.stop_current_task。结果为 timeout 就仍在停止，不能回答“已停止”；音乐播放、录制与外部绑定还要核对各自的停止命令。
