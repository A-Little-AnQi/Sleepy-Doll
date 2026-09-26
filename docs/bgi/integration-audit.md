# BGI 适配链路审计

审计以现有 `E:/BetterGIProject/better-genshin-impact` 源码为依据，宿主提交为 `7e02dc8cee57f264aa8d5f3efe4f18a956c39611`。Sleepy Doll 源码事实源为本仓库；未复制或修改 BGI 仓库。

## 审计范围与判定

[全量功能清单](feature-coverage.md)列出源码命令、AllConfig 设置、页面／窗口及稳定桥入口；[结构化清单](feature-coverage.json)保留定位与依赖信息。界面事件、编辑器功能也列出，不能把它们当作无参数业务操作。

完整链路必须同时具备：目标定位、输入来源与校验、当前版本可调用契约、符合权限的提交、真实宿主执行、结果核验、取消／失败处理。存在类、存在方法、HTTP 成功、命令处理器返回或 Job 完成，均不能单独证明用户目标已完成。

上一阶段取得旧驻留桥的只读快照，共 931 个接口；以下分类仅描述该快照，新版检查见文末。源码清单中的 319 个命令有 181 个已登记可调用、59 个当前不可调用、79 个当前目录未登记；622 个设置索引项有 615 个匹配当前目录、7 个需核对复杂对象展开或版本差异。未登记命令主要来自编辑器、弹窗、遮罩和未实例化的子 ViewModel，不等于对应业务完全不存在。所有源码清单中的稳定入口均已登记。源码索引、现场只读契约、隔离测试与真实游戏验证分别记录，不混为一个通过状态。

已完成源码静态全量盘点：319 个命令、622 个全局设置叶节点、68 个页面／窗口、140 个脚本 API 方法、35 个脚本资源模型字段。当前保留全部可见界面的原生链路；仅移除 41 个弃用／空置／内部旧命令与 2 个弃用设置；完整处理原因见 [全部接口审阅表](interface-review.md)。这是源码与契约审阅，不是实机全功能通过。

## 已发现的缺口

| 范围 | 根因 | 影响 | 修复／验证状态 |
|---|---|---|---|
| 删除配置组 | 原命令需要 ScriptGroup 对象，JSON 参数无法绑定；没有按名称稳定入口 | 已定位目标仍要求用户手动删除 | 已增加 `bgi.delete_script_group`；隔离验证版本冲突、重名、原生失败、备份、列表及选择 |
| 删除时影响其他组 | 宿主 ScriptGroups 的集合变化会重排并重写全部组 | 删除一个组可能覆盖其他组的未知字段 | 桥仅暂停宿主整表保存回调，保留 WPF 通知；其他文件字节不变、保存回调恢复均已验证 |
| 对话框命令 | 无参数命令内部弹出文件／名称输入框，契约却可调用 | Agent 提交空参数后把弹窗出现当成完成 | 源码索引增加输入依赖，目录禁止把需输入的弹窗当自动化入口 |
| 停止运行 | 桥 Job CTS 没有接到宿主 CancellationContext | 请求取消后游戏任务仍可继续 | 已加入当前任务取消联动与 `bgi.stop_current_task`；验证启动前取消、传递、旧作用域不取消新任务 |
| 首领次数与模式 | 所有 OnXChanged 被统一视为不可写；次数是边界校验，模式会清除树脂补充字段 | “执行两次”无法正确配置 | 开放受限次数 Schema，模式联动字段纳入预览、提交与回退；已验证 |
| 字典设置 | ValueContract 没有字符串键字典契约 | 遮罩指标开关等复合叶子只能读 | 已补字典值与属性数校验；类型错误和超限拒绝已验证 |
| 本机 JS 独立目标 | user.resolve 主要定位配置组和 AutoPathing；自建 JS 不一定在中央仓库 | 已安装 JS 无组时可能被报为不存在 | 本机 manifest 标题定位、settings_ui 读取、prepare_js_group 原生准备；默认值、选项、未知键及复用已验证 |
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
python bgi-bridge/dev/feature-guide-builder.py
```

运行桥快照只读接口目录，不读取模型密钥或当前敏感设置，不提交游戏任务；缓存位于 `target/bgi-coverage/`。

## 验证与可用性边界

上一阶段隔离检查包含 65 项断言；本次清理后的断言数见文末；另外验证了 Rust 的本机 JS 无仓库／无配置组定位。测试使用独立 WPF Dispatcher、假宿主服务与临时 User 目录，不接触真实配置组或游戏。源码检查与隔离测试可以验证适配逻辑，不能代替游戏内识别、队伍、奖励、通知接收方与桌面分身环境验收。

尚不能据此宣称全自动验收的项目包括编辑器的鼠标／布局操作、录制过程中的人工操作、外部平台验证码绑定，以及尚未在真实宿主验证的原生对象／文件选择适配。对象选择与弹窗输入已增加桥链路；部分目标也可以改走资源读写、设置事务或稳定入口。不得把这些项目描述成已完整自动化，也不得因为其中一个界面命令不可调用就否定整个业务功能。

现场只读目录可以核对登记、参数和可调用限制，但不能确认所有独立任务在游戏中完成。后续版本复查需保持 BGI 连接并重新获取目录；不自动试运行全部任务、不修改通知渠道或跨会话配置。

真实仓库只读回归已找到 `pathing/地方特产/稻妻/血斛`（1 个完整目标目录），以及历练点相关 JS（4 个候选），并成功从中央仓库读取 `js/使用历练点完成每日委托/manifest.json` 的 20 行文本。这验证了实际仓库查询与源码读取链路，没有订阅、修改配置或运行游戏；“本机未装”不能再被当作全仓没有。

运行隔离检查：

```powershell
dotnet build bgi-bridge/managed/BgiBridge.csproj -c Release -o target/bridge
dotnet build bgi-bridge/dev/CoverageChecks.csproj -c Release -o "$env:TEMP/sleepy-doll-bgi-coverage-validation/bin"
dotnet "$env:TEMP/sleepy-doll-bgi-coverage-validation/bin/BetterGI.dll"
```

测试程序集名称用于宿主类型定位，使用 dotnet 运行而不启动名为 BetterGI.exe 的假进程，以免触发实际桌面程序的宿主重连。结束后清理登记的临时目录。

## Agent 理解与渐进式披露

功能覆盖清单同时生成插件资源 `plugins/bgi/resources/feature-index.json`：2,638 个条目（覆盖可见界面声明、实际 JS 别名和目标流程；目录数量以生成器输出为准），其中包含 16 条用户目标流程，以及每个命令、设置、页面、脚本 API、资源字段和稳定入口的单项卡片。卡片保留输入来源、依赖、执行步骤、限制分支、核验方法、参考资料和源码版本。脚本 API 是脚本运行环境能力，不能当作同名 Agent 工具；界面选择、弹窗、内部事件和 async void 的限制仍明确标记。

运行时只常驻简短的 `bgi-assistant` 目标入口。`bgi-operator` 已拆成资源、设置、执行、仓库、源码排障和缺项说明六份参考资料，不再常驻完整操作手册。流程为：

```mermaid
flowchart LR
  A[简短目标入口] --> B[feature.search 摘要]
  B --> C[feature.read 单项卡片]
  C --> D[当前任务的 skill 参考资料]
  D --> E[api.describe 当前契约]
  E --> F[提交并按证据核验]
```

明确的资源运行请求仍直接 `user.resolve`，脚本问题直接读取脚本；不强制每次经过全部层级。功能总览只分页读取流程摘要。搜索最多返回 12 条摘要，单项卡片完整落在默认工具结果预算内；不会把整份索引、全量接口 Schema 或无关手册装入模型上下文。源码知识和检索实现属于 BGI 提供方；通用 Agent 只增加现有 BGI 只读分发中的两个工具名，没有修改通用规划、技能匹配或其他插件行为。

全量检查逐项核对源码清单与卡片、所有参考资料的实际 SkillRegistry 读取、16 类目标检索、摘要分页和单项结果大小。CI 执行生成器 `--check`，防止覆盖清单更新后插件索引未同步。

实际 Supervisor 配合当前配置的 `deepseek-flash` 做隔离行为回归：普通知识没有 BGI 调用；功能总览只读流程摘要；删除组按文件版本调用 `delete_script_group`；JS 参数解释读到本机定义及入口源码；“帮我跑下血斛”先 resolve 再调用正确运行入口，并使用终态证据。实际系统上下文约 2,438 字符，JS 任务约 4,964 字符，均未常驻完整索引或操作手册。隔离桥只操作系统临时目录，模型密钥仅在代理进程内存中；不连接真实 BGI、不运行游戏。这证明实际加载与所测目标路由，不证明所有模型、所有措辞或真实游戏结果都正确。

回归过程中修复了实际命名不一致：原生 BGI 的 Manifest 使用 SnakeCaseLower，JSON 字段为 `settings_ui`。本机检查器现在优先读取该字段，并兼容旧 `settingsUi`；C# 原生构造器测试也改用正确的原生序列化规则，上一阶段 65 项断言重新通过。

复现实际 Agent 验收（使用配置中的当前模型，会发出模型请求）：

```powershell
cargo build --no-default-features --features dev-ipc --bin bgi-agent-check
python bgi-bridge/dev/disclosure-check.py --config dist/Sleepy-Doll/user/config.json
```

测试工具只支持当前验收的 Anthropic Messages 协议。它登记并创建系统临时目录 `sleepy-doll-bgi-disclosure-validation`，监听本机随机端口，运行真实 AppController/Supervisor；退出时关闭本次进程、服务器并清理配置和数据库，不保留模型密钥或真实宿主数据。

## 当前可见功能与 JS API 补齐

先前把 17 个需要模态交互的现有功能摘出公共目录，不符合“所有当前可见功能都有链路”的要求。本次恢复这些真实命令，并增加原生 UI 的 read/write/options/reorder/invoke/respond/operation/close/cancel。字段来自实际 WPF 绑定，列表顺序使用原生 Move，按钮触发真实 OnClick 与异步执行任务。未展开的分组、菜单或页签可先展开／定位；字段、选项、绑定目标和版本变化均重新取证。

模态命令先返回 operationId，随后读取当前窗口、填写字段、点击保存／确认或补充本次弹窗输入，再读取操作终态与业务结果。一份弹窗输入只确认一个阶段，不把旧输入自动用到后续窗口。双向绑定、原生校验、单向显示不可写、敏感值遮蔽、失效对象和集合通知都有隔离回归。Lifecycle 由真实窗口和控件触发，不直接伪造 Loaded/Closing。

源码界面索引保留全部 1,365 个绑定／事件声明，每项都有原生 UI 流程卡，包含非 AllConfig 编辑器、列表行和动态字段。有效 AllConfig 字段仍可查询；直接事务受限的可见字段可走实际控件与保存链路。弃用旧跟踪、空测试、无产品调用的旧入口保持删除，不复活旧业务。

JS API 不再只列 Core.Script.Dependence 包装方法：从 EngineExtend 与 ScriptProject 的实际 AST 提取 68 个注入入口／别名，展开 140 个相关源码类型，并提供运行时 js_api.search/read。真实程序集检查覆盖全部 68 个入口，包括 ImageRegion/Region 的继承方法、RecognitionObject OCR 重载、BvPage/BvLocator/BvImage、OpenCV 导出集合、Task/Promise、默认参数、ref/out、params 与返回对象。只读检查不初始化 Host、不运行游戏或 JS；检查器使用兼容宿主依赖的 .NET 10，产品运行时与通用 Agent 未改。

当前真实程序集目录有 280 个动态命令与 47 个稳定入口，已删除入口没有复活，保留项无遗漏。117 项隔离断言通过；Rust 12 项通过、2 项现有环境检查跳过。生成器与功能卡逐项测试核对每个 UI 声明和 JS 别名，按需披露，不常驻全目录。

[全部接口审阅表](interface-review.md)、[结构化表](interface-review.json)及[完整调用链路图](agent-chain.md)均同步更新。真实已运行 BGI 的新版注入及游戏内全部业务分支尚未现场验收，不能用类型契约或隔离检查冒充实机全通过。

```powershell
dotnet run --project bgi-bridge/dev/SourceCommandChecks.csproj -- E:/BetterGIProject/better-genshin-impact/BetterGenshinImpact/bin/x64/Debug/net8.0-windows10.0.22621.0/BetterGI.dll docs/bgi/feature-coverage.json
```
