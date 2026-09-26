# BGI 适配链路审计

审计以现有 `E:/BetterGIProject/better-genshin-impact` 源码为依据，宿主提交为 `7e02dc8cee57f264aa8d5f3efe4f18a956c39611`。Sleepy Doll 源码事实源为本仓库；未复制或修改 BGI 仓库。

## 审计范围与判定

[全量功能清单](feature-coverage.md)列出源码命令、AllConfig 设置、页面／窗口及稳定桥入口；[结构化清单](feature-coverage.json)保留定位与依赖信息。界面事件、编辑器功能也列出，不能把它们当作无参数业务操作。

完整链路必须同时具备：目标定位、输入来源与校验、当前版本可调用契约、符合权限的提交、真实宿主执行、结果核验、取消／失败处理。存在类、存在方法、HTTP 成功、命令处理器返回或 Job 完成，均不能单独证明用户目标已完成。

本次读取两套安装配置的桥地址时均连接被拒绝。当前报告中的“运行桥尚未核验”是缺少现场快照，不代表接口不存在；源码索引和隔离测试与真实游戏验证分别记录，不将它们混为一个通过状态。

已完成源码静态全量盘点与链路分类：319 个命令、622 个全局设置叶节点、68 个页面／窗口、140 个脚本 API 方法、35 个脚本资源模型字段。319 个命令中，217 项可按动态命令机制继续核验，33 项需要宿主对象绑定，24 项需要当前选择证据，21 项依赖弹窗输入，22 项是内部事件／空实现，2 项使用不可跟踪的 async void。这是源码分类，不是 217 项实机通过。

## 已发现的缺口

| 范围 | 根因 | 影响 | 修复／验证状态 |
|---|---|---|---|
| 删除配置组 | 原命令需要 ScriptGroup 对象，JSON 参数无法绑定；没有按名称稳定入口 | 已定位目标仍要求用户手动删除 | 已增加 `bgi.delete_script_group`；隔离验证版本冲突、重名、原生失败、备份、列表及选择 |
| 删除时影响其他组 | 宿主 ScriptGroups 的集合变化会重排并重写全部组 | 删除一个组可能覆盖其他组的未知字段 | 桥仅暂停宿主整表保存回调，保留 WPF 通知；其他文件字节不变、保存回调恢复均已验证 |
| 对话框命令 | 无参数命令内部弹出文件／名称输入框，契约却可调用 | Agent 提交空参数后把弹窗出现当成完成 | 源码索引增加输入依赖，目录禁止把需输入的弹窗当自动化入口 |
| 停止运行 | 桥 Job CTS 没有接到宿主 CancellationContext | 请求取消后游戏任务仍可继续 | 已加入当前任务取消联动与 `bgi.stop_current_task`；验证启动前取消、传递、旧作用域不取消新任务 |
| 首领次数与模式 | 所有 OnXChanged 被统一视为不可写；次数是边界校验，模式会清除树脂补充字段 | “执行两次”无法正确配置 | 开放受限次数 Schema，模式联动字段纳入预览、提交与回退；已验证 |
| 字典设置 | ValueContract 没有字符串键字典契约 | 遮罩指标开关等复合叶子只能读 | 已补字典值与属性数校验；类型错误和超限拒绝已验证 |
| 本机 JS 独立目标 | user.resolve 主要定位配置组和 AutoPathing；自建 JS 不一定在中央仓库 | 已安装 JS 无组时可能被报为不存在 | 本机 manifest 标题定位、settingsUi 读取、prepare_js_group 原生准备；默认值、选项、未知键及复用已验证 |
| 当前界面选择 | 部分命令依赖 SelectedConfig、SelectedScriptProject 或 SelectedTrack | 可调用不等于能作用于用户点名目标 | 全量记录选择依赖，需逐类核对稳定替代入口与 skill 指引 |
| 联动设置 | 所有钩子一律拒绝，包含仅边界限制的 19 项和首领模式 1 项 | 日常次数、缩放、颜色等无法配置 | 已核对源实现；19 项按数值／颜色边界开放，模式纳入关联字段事务；范围与格式已验证 |
| 页面导航 | 源码导航服务存在，桥没有按目标页面入口 | 搜索“导航”未命中后无可执行入口 | 新增 list_pages/open_page，明确 14 个主页面；导航选择与窗口可见性已验证 |
| 一条龙配置绑定 | 只替换 SelectedConfig，没有同步 TaskList，未刷新配置文件 | 指定配置可能使用旧任务列表 | 刷新配置并走 SetSomeSelectedConfig；指定 B 配置同步 B 任务列表已验证 |
| 配置组运行 | 为加载新建组调用 ReadScriptGroup，会重写所有组文件 | 执行目标前可能改变其他组 | 直接从磁盘构造指定组并调用 StartGroups，不刷新整表；绑定测试通过 |

## 功能域与链路

| 功能域 | 源码入口 | 资源／参数来源 | 桥链路 | skill 与验证 |
|---|---|---|---|---|
| 启动、截图、窗口、分辨率 | HomePage、GenshinStartConfig | 实时状态、安装路径与显示记录 | start_game / wait_ready / get_status / set_game_resolution | bgi-operator 执行前检查；核对真实窗口和主界面 |
| 实时拾取、剧情、传送、钓鱼、吃药 | TriggerSettingsPage 与对应 GameTask Config | 当前 AllConfig 叶节点 | settings 搜索、read、preview/commit | 读取 setting 契约及 writable；回读版本和值 |
| 战斗、秘境、首领、地脉、伐木、烹饪 | TaskSettingsPage 与对应 GameTask | 当前任务设置与策略 | 对应 Switch* 命令；停止走宿主取消上下文 | 参数先读设置，运行 Job 与宿主结果分别核对 |
| 七圣召唤、音游、千音雅集、幽境、分解、兑换码 | TaskSettingsPage 与对应 GameTask | 策略／曲谱／物品规则 | 动态命令与设置事务 | 不能把策略选择、文件弹窗或处理器返回当任务结束 |
| 调度器配置组与项目 | ScriptControl、Core/Script/Group | User/ScriptGroup，精确 name、路径和 SHA | resolve / read/write / prepare_pathing_group / run_script_group / delete_script_group | bgi-operator；目标文件、列表、引用和运行日志 |
| 一条龙与日常任务 | OneDragonFlow 与 OneDragon 子 ViewModel | User/OneDragon、配置名和启用任务 | run_one_dragon，配置读写 | 确认所选配置；日志与收尾状态 |
| 地图追踪 | MapPathing、PathingPartyConfig | 本机目录或仓库父节点／作者包 | repo.search/read / subscribe / prepare / run | collection-workflow；保持完整父目录 |
| JavaScript | JsList、Core/Script/Dependence、Project | manifest、settingsUi、README、入口与模块 | user.inspect_script/read、repo.read、prepare_js_group、运行组 | bgi-javascript；读真实定义，不将未订阅当不存在 |
| 键鼠录制与回放 | KeyMouseRecordPage | User/KeyMouseScript 与录制参数 | 字符串参数命令或配置组 | 录制结束核验文件，回放核验任务状态 |
| 脚本仓库与订阅 | ScriptRepoUpdater、RepoWebBridge | 当前渠道 Git 索引与 Subscriptions | search/read、subscribe、update_subscribed_scripts | repositoryOnly 与订阅更新分开 |
| 宏与快捷键、按键绑定 | MacroSettingsPage、HotKeyPage、KeyBindings | AllConfig 的对应分区 | settings 事务及相应命令 | 改绑定不等于执行功能 |
| 音乐曲谱与播放 | MusicPage | 曲谱目录、当前轨道、播放模式 | 字符串参数或有选择依赖的命令 | 播放、暂停和独立任务取消分别核验 |
| 通知与绑定 | NotificationSettingsPage | 用户授权的接收方及渠道配置 | settings、TestWebhook、绑定／取消绑定命令 | 敏感值遮蔽；绑定仍可能需要用户在外部平台交互 |
| 遮罩、地图点位、技能 CD | MaskWindow、MapMask、SkillCd | 全局设置、地图数据与点位选择 | settings 与相关 UI 命令 | 点位编辑、拖拽等 UI 事件不冒充完整业务接口 |
| 桌面分身与子会话 | ChildSessionWindow、ChildSessionService | 会话状态和启动配置 | 当前注册的子会话命令 | 跨 Windows 会话的前置需单独确认；未实机验收 |
| 日志、统计、养成与任务进度 | HostLog、TaskProgress、LogParse | 宿主日志、配置与任务记录 | get_script_errors / read_host_log、资源读取及 UI 命令 | 打开统计网页不等于已经读取其结果 |
| 开发工具、编辑器、模板、地图制作 | 编辑器窗口、开发 ViewModel | 用户提供的点位、模板和素材 | 开发可见接口、现有脚本 API | 不编造点位或把内部鼠标事件当可靠自动化 |
| 软件更新、缓存、外观、语言 | CommonSettingsPage、MainWindow、Settings | 宿主配置与版本 | settings 与明确的更新／界面命令 | 文件选择、下载、重启等阶段各自核验 |

## 复查命令

```powershell
dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- E:/BetterGIProject/better-genshin-impact bgi-bridge/managed/generated/host-documentation.json
python bgi-bridge/dev/coverage-audit.py E:/BetterGIProject/better-genshin-impact --live-config dist/Sleepy-Doll/user/config.json
```

运行桥快照只读接口目录，不读取模型密钥或当前敏感设置，不提交游戏任务；缓存位于 `target/bgi-coverage/`。

## 验证与可用性边界

隔离检查包含 65 项断言；另外验证了 Rust 的本机 JS 无仓库／无配置组定位。测试使用独立 WPF Dispatcher、假宿主服务与临时 User 目录，不接触真实配置组或游戏。源码检查与隔离测试可以验证适配逻辑，不能代替游戏内识别、队伍、奖励、通知接收方与桌面分身环境验收。

仍不具备无交互完整入口的项目，在全量表中明确标记：编辑器的鼠标／布局操作、录制过程中的人工操作、部分对象选择、外部平台验证码绑定和宿主文件选择对话框。部分目标可以改走资源读写、设置事务或新稳定入口；没有替代入口的项目仍需专门适配。不得把这些项目描述成已完整自动化，也不得因为其中一个界面命令不可调用就否定整个业务功能。

在真实桥未连接时，无法核验当前安装版本是否登记全部源码命令，也不能确认本机所有独立任务可执行。复查需要保持 BGI 连接，然后重新获取只读目录；不自动启动游戏、不试运行全部任务、不修改用户通知渠道或跨会话配置。

运行隔离检查：

```powershell
dotnet build bgi-bridge/managed/BgiBridge.csproj -c Release -o target/bridge
dotnet build bgi-bridge/dev/CoverageChecks.csproj -c Release -o "$env:TEMP/sleepy-doll-bgi-coverage-validation/bin"
dotnet "$env:TEMP/sleepy-doll-bgi-coverage-validation/bin/BetterGI.dll"
```

测试程序集名称用于宿主类型定位，使用 dotnet 运行而不启动名为 BetterGI.exe 的假进程，以免触发实际桌面程序的宿主重连。结束后清理登记的临时目录。
