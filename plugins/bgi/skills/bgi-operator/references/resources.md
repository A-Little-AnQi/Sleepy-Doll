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


## 查询现有配置

1. 用一次 `bgi.user.list` 找到目标目录。只要名称时不传 `jsonKeys`；需要梳理 JSON 内容时，在同一次调用传 `jsonKeys`。配置组通常投影 `name,index,projects`，只有分析运行参数时才读取 `config`。
2. 仅当某个文件没有返回 `data`、内容被截断或需要完整写回时，再对该文件调用 `bgi.user.read`。多个独立文件在同一轮并行读取。
3. 汇总磁盘事实，不再调用 `bgi.api.search`、`plugins.list` 或重复列目录。
4. 区分启用状态、调度周期、任务类型和脚本内部参数；不要把文件名或组名当作结论。


## 创建用户资源

1. 确认目标名称在对应目录中不存在；若用户没指定名称，使用能表达目标且不冲突的名称，不为普通命名再次提问。
2. 读取一个最接近的现有对象作为结构样板。若用户明确要创建同类组，可保留样板的 `config`，只替换组身份和用户要求的 tasks；不要再读 User/config.json 或 AutoFight 重建同一份宿主配置。样板不能继承用户未要求的额外任务、账号、配队或通知权限。
3. `folderName` 已被现有配置组引用时，仍须确认对应目录或文件还在；缺失则更新/订阅，不得视为已安装。
4. JS 任务必须核对脚本参数定义；没有可靠默认值的必填参数才构成用户缺项。
5. 计算顺序字段时复用目录投影里的 `index`；不要再逐文件读取文件头。
6. 调用 `bgi.user.write` 提交完整文件。替换已有文件时传 `expectedSha256`；新建时省略。运行时按实际改动范围决定要不要确认一次：改几个字段、新建一个对象都直接执行；删除文件或大范围改配置（同一意图内累计 10 个以上配置叶字段、3 个以上对象，或整份替换）才弹一次范围确认。不在对话里重复索要许可。
7. 写后重新读取目标文件，核对名称、任务数、引用和关键参数。保留返回的 `backup`；需要撤销时调用 `bgi.user.restore`，并传当前文件的 `sha256`，不要手工覆盖。


## 删除配置组

用户明确要求删除某个配置组时，删除就是目标，不用“禁用任务”替代，也不要求先在 BetterGI 选中它。

1. 用配置组目录列表或现有证据定位一个精确目标，读取 `name` 和该文件的 `sha256`；同名候选无法唯一定位时才询问。
2. 直接 describe `bgi.delete_script_group`，invoke 传 groupName 和 expectedSha256。它不需要游戏或截图器，不查 lifecycle、导航或地图追踪页面。
3. 运行时按当前权限模式审批，不另外在聊天里重复问一次。版本冲突时重新读取，不忽略校验；任务正运行时先报告冲突，不擅自停止。
4. `deleted=true` 且 `verified=true` 才说明删除完成。保留 backup；路线、JS 和订阅仍在。原 `DeleteScriptGroupCommand` 不可调用只是缺少对象绑定，不能推导为无法删除配置组。
5. 撤销时读取 backup，通过 user.write 在原 path 新建完整原始内容；目标路径已有其他文件时不能覆盖。
