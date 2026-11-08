# 三月七小助手适配插件

独立的 C++ 原生桥、工具清单与领域技能。只服务三月七，不依赖其它适配插件，不要求额外 Python 环境，不使用 Python 桥或脚本文本执行。

本轮验证覆盖编译、产品插件加载器及伪宿主隔离夹具；没有启动、注入真实三月七，也没有执行游戏。宿主契约来自官方 v2026.11.7 源码和发布包静态分析。包内 python314.dll 文件版本为 3.14.8；版本与导出表检查不是运行兼容性证明。

## 连接与使用

独立交付包位于 `dist/plugins/march7th/`，在 Sleepy-Doll 中导入该目录，按需启用。源码目录可用于开发，生产连接不依赖 `test/local-adapters/`。

1. 由用户手动启动三月七 GUI。桥不会启动工具、游戏、更新器或安装程序。
2. 为 Sleepy-Doll 和注入器设置相同的进程环境变量 `MARCH7TH_BRIDGE_TOKEN`（16..256 个可打印 ASCII 字符）；不要把值写进插件包。
3. 从任务管理器取得 GUI 进程 ID，在交付目录执行 `start-bridge.ps1 -TargetProcessId <进程 ID>`。开发构建可执行 `target/bridge/march7th/march7th_injector.exe --pid <进程 ID>`。
4. 模型先读取 `march7th.http.status`，再通过任务目录与配置描述确定实际参数。默认桥地址为 `http://127.0.0.1:23111`。

每次写操作必须携带本次状态返回的 `instanceId` 与 `catalogVersion`；配置写还需要 `config.read` 返回的 `revision`。重连后旧会话参数不能复用。路径、解释器、外部命令、计划任务等不能通过配置工具写入。

## 原生链路

- 注入器只对明确指定、架构匹配且已加载 CPython 的现有进程附着。唯一远程线程指向系统 LoadLibraryW，不远程调用新 DLL 导出函数，不使用 TerminateThread。
- DllMain 只禁用线程通知并拉起 worker，立即返回。worker 在加载器锁外 pin 模块，建立按工具名与宿主 PID 命名的本地管道。管道拒绝远程客户端，仅允许当前用户；注入器验证服务端 PID 后才发送令牌和端口。
- 通过已加载 CPython 的 C API 访问现存 MainWindow、logInterface、cfg 和 AVAILABLE_TASKS，不构造宿主对象、不导入业务模块、不执行 PyRun/eval/exec 字符串。
- GUI 访问经已加载 PySide6 的 `QTimer.singleShot(0, context, nativeCallable)` 投递。context 来自现存 QApplication；回调为 C++ PyCFunction，状态由 capsule 和共享所有权管理，等待期间不持 GIL。不依赖 Python 字节码边界才处理 pending call。
- 排队超时会取消，迟到回调不再执行；执行中超时报告未知。每次操作独立保存参数副本、结果与错误；事件句柄在最后一个持有者释放时关闭。
- 停桥只停止桥的监听，不退出三月七。DLL 与必要对象引用保持到宿主退出，避免迟到回调访问已卸载代码；常驻控制面支持重新连接。

## 功能与证据边界

九个 httpTools 提供状态、任务目录搜索/描述、配置读取/描述/写入、任务启动/停止、日志尾部，注册为 `march7th.http.*`。没有上游 MCP 依赖。

`taskState` 区分 stopped、starting、running、unknown。accepted 只表示已观察到运行接纳，不证明游戏完成；启动未确认返回 unconfirmed。停止依据真实进程状态与外部任务活动，处于 Starting 或无法确认时不能声称停止。unknown 不自动重发。日志读取调用已有控件的 toPlainText()。

配置更新的“校验修订号 → 宿主 set_value → 回读”在一个 GUI 闭包内执行。宿主 setter 自身会重新读取并保存文件，因此这不是针对任意外部编辑器的跨进程原子事务。敏感键在嵌套对象、数组内同样脱敏，不提供还原凭据的通道。

## 构建与验证

`bridge/build.cmd` 使用 MSVC x64 C++20 与 Windows SDK，静态 CRT，无第三方包；`--test` 只编译夹具。输出进入 `target/bridge/march7th/`，临时文件进入 `target/.tmp/local-adapters/`。

`tests/plugins/run-native.ps1 -Plugin march7th` 检查夹具导入表，再在清空凭据环境、私有桌面、单进程 Job 内执行。夹具仅链接核心、路由与伪宿主，不包含真实注入、进程启动、网络连接或键鼠输入路径。

生产包只包含清单、技能、说明、注入器和桥 DLL，排除测试、源码、第三方工具及用户数据。真实冻结 GUI 的注入、Qt 回调与游戏行为尚未运行验收，不能把编译或夹具通过解释为真实宿主通过。
