# JS 参数与源码排障

## 理解和修改脚本任务

1. 问本机任务时从配置组任务的 `folderName` 定位；只问脚本含义时直接用 `bgi.repo.search` 搜索脚本标题，使用返回的精确 path，不要求先订阅。
2. 已知本机脚本用一次 `bgi.user.inspect_script` 读取资料；未安装脚本用 `bgi.repo.read` 直接读中央 Git 对象中的 manifest、README 和 settings。仓库候选携带 `readingGuide` 时按其中的 `bgi-javascript` 技能追踪源码。不要把 User 目录未命中当成仓库没有该脚本。
3. `jsScriptSettingsObject` 的键必须来自 `settings.json.name`；值满足对应类型、选项和默认值。
4. 参数定义只说明用途；涉及动作先后、切回、默认值、生效条件或报错时按 `bgi-javascript` 阅读 main 指定的入口、字段全部引用和必要模块。只问输入类型或选项列表时定义足够。需要账户文件或子资源时按定义路径读取。
5. 修改配置组时读取完整目标文件，只改目标字段，保留未知字段；写入时传该次读取返回的 `sha256`，防止覆盖期间出现的新改动。


## 排查脚本报错

宿主把脚本失败写进按天日志，报错原文、出错脚本名和涉及的文件都能直接取到。不要先让用户复制日志或截图。

1. `bgi.get_script_errors` 是固定稳定接口，直接 `bgi.api.describe` 后 `bgi.api.read`，不先 `bgi.api.search`。默认读当天的日志；失败发生在更早的日期时传 `date`，可用日期在 `availableDates` 里。
2. 每条 `failure` 已经归并过：`error` 是宿主给出的一手报错原文，`jsError` 非空表示错误由 JS 引擎抛出，`script` 是从同一次运行的日志里读到的脚本名，`context` 是这次运行前后的记录。
3. 按 `locations` 逐项核对，不再搜索接口：
   - `kind=userFile` 是相对 `User\` 的路径，直接交给 `bgi.user.read`。缺失的路线、数据或配置文件通常出现在这里，报错原文会说它找不到什么。
   - `kind=hostSource` 是宿主源码位置（`BetterGenshinImpact/Service/ScriptService.cs:548`），与 `bgi.api.describe` 返回的 source 字段同一形式。它说明失败发生在宿主哪一步，不是用户能改的文件，别把它当成要修的对象。
   - `kind=absolute` 是打包机路径，本机不一定存在。
4. 报错指向脚本本身时读源码：`bgi.user.read` 读 `User\JsScript\<folderName>\main.js`，`folderName` 取配置组任务里的字段，不按显示名猜目录。参数与运行前提以 `README.md`、`manifest.json`、`settings.json` 为准。
5. 需要这一次运行的完整过程时，用 `bgi.read_host_log` 传 `failure.thread` 读出同一线程的记录：脚本自己的 `log()` 输出、开始与结束行、宿主异常都在其中。
6. `script` 为空表示日志里没有能对上号的脚本名，不要猜：按 `thread` 读原始日志，或回到配置组按用户说的任务名核对 `folderName`。
7. 结论必须落在可核对的位置：报错原文、出错的脚本或文件、下一步动作（改哪个文件、补哪条订阅、改哪个参数）。日志里确实没有记录的失败，直说没有记录，不编造原因。
