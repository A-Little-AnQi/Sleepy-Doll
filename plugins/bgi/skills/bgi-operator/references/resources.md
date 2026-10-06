# 用户资源与配置组

## BetterGI 对象模型

| 对象 | 用户目录中的位置 | 关键事实 |
|---|---|---|
| 调度器配置组 | `ScriptGroup\<组名>.json` | `projects[]` 才是实际任务清单 |
| JS 脚本 | `JsScript\<目录>\` | 目录名由任务的 `folderName` 引用 |
| 地图追踪路线 | `AutoPathing\...` | 层级和名称由已安装数据决定 |
| 键鼠脚本 | `KeyMouseScript\` | 录制与回放文件 |
| 一条龙 | `OneDragon\` | 与调度器配置组不是同一种对象；运行走 `bgi.run_one_dragon` |
| 战斗/卡牌/音乐等资源 | 对应 `AutoFight\`、`AutoGeniusInvokation\`、`Music\` | 以磁盘实际内容为准 |
| 全局设置 | `config.json` | 运行中以内存 `AllConfig` 为准，修改走设置事务 |

配置组包含 `name`、`index`、`config`、`projects[]`。任务常用字段包括 `name`、`folderName`、`index`、`type`、`schedule`、`status`、`runNum`、`jsScriptSettingsObject`。字段形状以用户现有同类文件和脚本自己的参数定义为准。

`schedule` 是 BetterGI 的调度周期。脚本内部还可能按星期、账号状态或运行记录自行跳过，因此组名和 `schedule` 都不能单独证明任务何时实际执行。

## 路线运行参数在哪一层

| 层 | 位置 | 适用 |
|---|---|---|
| 配置组 | 组 JSON 的 `config.pathingConfig`（`enabled`、`partyName`、autoFightConfig、拾取／恢复等） | 原生路线及允许继承组设置的 JS |
| 单 JS 任务 | `projects[].jsScriptSettingsObject` | 该 JS 自己的自定义参数 |
| 全局 | `User/config.json` | 全局默认，修改走设置事务，不用它替代组配置 |
| 路线内容 | 路线 JSON 的 `info`／`positions` | 路点与路径本身，不是组运行设置，不改叶子路线 |

现成路线要换队伍或赶路角色时：复用已有组或 `bgi.prepare_pathing_group` 建包装组，用 `bgi.set_pathing_party` 写入（partyName 自动联动 enabled，角色俗称由接口做数据映射），回读核验。运行前用 `bgi.inspect_group_effective` 读当前有效战斗策略与原文要求，按已明确配队适配组配置；`bgi.sync_group_effective_config` 只恢复确需继承的字段，不覆盖用户有意自定义。战斗／拾取／恢复等其余运行参数：读取该组真实完整 `config` 后按需改点名字段，写后回读证明已保存、下次运行生效。不创建切队 JS，不改全局，不改叶子路线。不同任务不同队伍保留各自独立配置组；一个快捷入口仍可组合多组，不因独立组配置要求拆入口。


## 查询现有配置

1. 用一次 `bgi.user.list` 找到目标目录。只有本轮只需要名字或任务清单时才能省略 `config`（如投影 `name,index,projects`）；一旦涉及运行参数（队伍、战斗、拾取、恢复等 `config.pathingConfig` 或脚本参数），必须在同一次调用包含 `config`，不能先省略再补读。已经知道精确文件路径时直接 `bgi.user.read`，不重复 list 定位。
2. 仅当某个文件没有返回 `data`、内容被截断或需要完整写回时，再对该文件调用 `bgi.user.read`。多个独立文件在同一轮并行读取。`droppedKeys` 显示 `config` 被投影掉、`truncated` 或分页时，字段只是未返回，不能当作不存在；要修改运行参数时 read 必须包含 `config` 或完整文件。
3. 汇总磁盘事实，不再调用 `bgi.api.search`、`plugins.list` 或重复列目录。
4. 区分启用状态、调度周期、任务类型和脚本内部参数；不要把文件名或组名当作结论。


## 创建用户资源

1. 确认目标名称在对应目录中不存在；若用户没指定名称，使用能表达目标且不冲突的名称，不为普通命名再次提问。
2. 读取一个最接近的现有对象作为结构样板。若用户明确要创建同类组，可保留样板的 `config`，只替换组身份和用户要求的 tasks；不要再读 User/config.json 或 AutoFight 重建同一份宿主配置。样板不能继承用户未要求的额外任务、账号、配队或通知权限。
3. `folderName` 已被现有配置组引用时，仍须确认对应目录或文件还在；缺失则更新/订阅，不得视为已安装。
4. JS 任务必须核对脚本参数定义；没有可靠默认值的必填参数才构成用户缺项。
5. 计算顺序字段时复用目录投影里的 `index`；不要再逐文件读取文件头。
6. 调用 `bgi.user.write` 提交完整文件。写前必须在本轮完整读取目标文件——历史轮读取的内容和 SHA 不是当前证据——保留所有未知字段及其它 project、索引、周期与权限；SHA 取同一次完整读取的结果，不能用投影重建大 JSON。替换已有文件时传 `expectedSha256`；新建时省略。返回版本冲突（ok:false，未写入）就重新读取最新文件，保留其中新出现的变更，只改用户目标的字段后再次提交；确定不了改法才问用户，不要用旧内容覆盖或重复提交同一 SHA。运行时按实际改动范围决定要不要确认一次：改几个字段、新建一个对象都直接执行；删除文件或大范围改配置（同一意图内累计 10 个以上配置叶字段、3 个以上对象，或整份替换）才弹一次范围确认。不在对话里重复索要许可。
7. 写后重新读取目标文件，核对名称、任务数、引用和关键参数。保留返回的 `backup`；需要撤销时调用 `bgi.user.restore`，并传当前文件的 `sha256`，不要手工覆盖。


## 删除配置组

用户明确要求删除某个配置组时，删除就是目标，不用“禁用任务”替代，也不要求先在 BetterGI 选中它。

1. 用配置组目录列表或现有证据定位一个精确目标，读取 `name` 和该文件的 `sha256`；同名候选无法唯一定位时才询问。
2. 直接 describe `bgi.delete_script_group`，invoke 传 groupName 和 expectedSha256。它不需要游戏或截图器，不查 lifecycle、导航或地图追踪页面。
3. 运行时按当前权限模式审批，不另外在聊天里重复问一次。版本冲突时重新读取，不忽略校验；任务正运行时先报告冲突，不擅自停止。
4. `deleted=true` 且 `verified=true` 才说明删除完成。保留 backup；路线、JS 和订阅仍在。原 `DeleteScriptGroupCommand` 不可调用只是缺少对象绑定，不能推导为无法删除配置组。

## 删除路线或脚本资源

明确要求删除资源时，读取到真实 User 路径就直接进入资源删除，不再从 UI 重新发现相同目标。user.list/read 的 resourceLifecycle 给出稳定入口。

1. describe/read `bgi.inspect_local_resource`，path 使用已定位的完整材料目录／作者包／单文件。一次取得全目录文件数、内容版本、其他组引用及订阅覆盖；不逐条读取 5 条路线来寻找删除接口。
2. 用户同时要求删配置组和路线，先按名称与 SHA 删除目标配置组，再 inspect 路线目录，避免自己刚删的组被当作仍存在的引用。
3. describe/invoke `bgi.delete_local_resource`，path 与 expectedVersion 使用本次检查结果。若其他组仍引用，按用户授权范围处理这些组；不能默认 allowBrokenReferences=true。
4. result.deleted=true、result.verified=true 即文件删除核验完成；保留 backupId。其他资源、组和订阅保留。coveringSubscriptions 非空时明确说明后续订阅更新可能重新导入，不擅自扩大为取消父目录订阅。
5. 恢复只走 `bgi.restore_local_resource` 的 backupId，原位置有新内容时拒绝覆盖，不与配置组文件备份混用。当前桥缺少稳定入口时说明需要更新桥，不连续试页面、绑定、树节点或选项。只有用户目标本身要求界面操作时才走原生 UI。
