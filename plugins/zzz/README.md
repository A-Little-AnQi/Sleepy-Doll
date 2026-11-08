# 绝区零一条龙原生反射桥（zzz）

Sleepy Doll 与用户自己启动的绝区零一条龙（ZenlessZoneZero-OneDragon）
GUI 进程之间的本机桥。桥是一个原生 C++ DLL（`zzz_bridge.dll`），由
`zzz_injector.exe` 注入到用户明确指定的现存进程，在宿主进程内监听
`127.0.0.1:23101`，通过 Sleepy Doll 插件清单（`.sleepy-doll-plugin/plugin.json`）
注册为 `zzz.http.*` 七个 HTTP 工具。

本桥**不是** MCP 服务，也不连接任何上游 MCP；旧版 23001 端口的 MCP 入口已废弃。

## 组成

| 文件 | 作用 |
| --- | --- |
| `bridge/zzz_pure.hpp/.cpp` | 纯逻辑单元：键/值安全规则、规范 hash、HTTP 解析、job 状态机；测试目标只链接它。 |
| `bridge/zzz_core.cpp` | 生产核心：CPython C API 运行时解析（GetProcAddress）、白名单反射、七个工具路由、WinSock 服务。 |
| `bridge/control_ipc.hpp` | 注入器与 DLL 之间的本地命名管道控制面协议（start/stop + 真实结果回执）。 |
| `bridge/zzz_dll.cpp` | DLL 入口：DllMain 只拉起控制面 worker；不导出任何远程线程可调用的入口。 |
| `bridge/injector.cpp` | 注入器：全文件只有一次指向系统 LoadLibraryW 的远程线程；经控制面下发启动。 |
| `bridge/tests.cpp` | 纯逻辑测试：伪 pending 回调队列驱动 job 状态机，不含任何宿主接口。 |
| `bridge/build.cmd` | MSVC C++20（静态 CRT /MT）构建入口，输出 `target/bridge/zzz/`。 |

## 主线程派发（Qt 队列 poster）

目录 hash 同时绑定本次附着会话的随机标识；停止、重连后，旧连接与旧目录凭据不能继续提交。停止会取消未开始的回调，已执行中的操作保持未知语义，不会因断开连接而被冒充为未执行。解释器函数表及必要上下文引用保持到宿主进程退出，避免重连与迟到回调互相释放对象。

`Py_AddPendingCall` 只在 Python 字节码边界执行；Qt GUI 长期停留在 C++ 事件循环
时，pending call 可能一直等到用户活动或下一个 Python 定时回调，BGI 式空闲窗口
无法保证响应。生产 poster 因此改用宿主已加载的
`PySide6.QtCore.QTimer.singleShot(0, context, functor)` 重载（官方文档明确
支持 msec/context/functor 形态，带 context 时可在任意线程安全调用，functor 在
context 所属 GUI 线程、持有 GIL 时执行）：

- context 取自 `PySide6.QtWidgets.QApplication.instance()`——只读取现存实例，
  绝不创建 App；`instance()` 为 None（无 GUI）即拒绝启动。
- 回调是纯 C++ 的 `PyCFunction`：静态 `PyMethodDef`（METH_NOARGS，DLL pin 生命
  周期）+ `PyCFunction_NewEx`，状态经 `PyCapsule` 携带；全程只用 CPython C API
  反射已加载对象，不 import 业务模块、不执行任何 Python 源码字符串、不链接
  Qt SDK。
- 生命周期：`consumed` 原子标志保证执行与丢弃二选一恰一次；Qt context 销毁
  丢弃回调时，capsule 析构经 discard 钩子取消并释放 ticket（等待方持有的
  `shared_ptr` 保证此间 job 与事件句柄存活），无 UAF、无二次释放。
- GIL 只在创建与投递期间持有，等待回调期间已释放；拿到 GIL 前已超请求期限的
  写操作不再晚投递，明确失败。
- 投递失败（无 PySide6 上下文、期限超限、Qt 拒绝）一律明确报错，不降级到会
  饥饿的 pending 路线；`PyCFunction_NewEx` / `PyCapsule_*` 等导出缺失时启动
  即拒绝（fail closed）。

## 注入方式（与 BGI 一致的单次 LoadLibraryW）

对刚映射进宿主的注入 DLL 再开第二个指向其导出函数的远程线程，在启用 CFG 的
宿主会以 0xC0000409/0xA 终止宿主（见 `bgi-bridge/native/injector.cpp` 顶部记载）。
本桥因此不导出任何函数，注入链路为：

1. 注入器做**唯一一次** `CreateRemoteThread`，目标只是系统 `LoadLibraryW`，
   把 `zzz_bridge.dll` 映射进目标进程；等待该线程退出确认映射后才释放远程
   路径内存，超时则保留分配直到宿主退出并如实报告结果未知。
2. DLL 的 DllMain 只禁用线程通知并拉起控制面 worker（新线程要等加载器锁
   释放后才开始运行，worker 全程 try/catch）。
3. worker 在锁外 pin 本模块，创建命名管道
   `\\.\pipe\SleepyDoll.ZzzBridge.<宿主PID>`：`PIPE_REJECT_REMOTE_CLIENTS` +
   仅当前用户 DACL；不写宿主文件、不落令牌文件、不启动系统服务。
4. 注入器连接该管道，**先 `GetNamedPipeServerProcessId` 核对服务器进程就是
   显式指定的目标 PID，之后才发送令牌**，再下发启动配置；DLL 回执
   `StartBridgeCore` 的真实结果码。控制面常驻，支持 HTTP 停桥后再次启动。
5. 注入器最后通过 `/status` 验证桥已定位到唯一 `ZContext`。

## 构建

```
plugins\zzz\bridge\build.cmd          # 全量：zzz_bridge.dll、zzz_injector.exe、zzz_tests.exe
plugins\zzz\bridge\build.cmd --test   # 只构建纯测试 exe，不在本地运行（由隔离 runner 执行）
```

要求：MSVC 2022 x64（自动定位本机 `vcvars64.bat` 或 vswhere）、Windows SDK
（C++/WinRT `Windows.Data.Json`）、链接 `ws2_32.lib`、`bcrypt.lib`、
`windowsapp.lib`、`advapi32.lib`。全部产物使用静态 CRT（/MT），用户机器无需
安装 VC Runtime。中间 obj 与产物都在 `target/bridge/zzz/`；发布打包按此文件名
放入 `dist/plugins/zzz/bridge`（见 `scripts/package-adapters.ps1`）。

## 使用流程

1. 用户自己启动绝区零一条龙 GUI（源码或带 `src`+runtime 的发布包）。
2. 在宿主与 Sleepy Doll 两侧设置相同的令牌环境变量（默认名 `ZZZ_BRIDGE_TOKEN`，
   可见 ASCII，≥16 字符）。Sleepy Doll 侧的插件清单通过
   `X-Sleepy-Token: ${ENV:ZZZ_BRIDGE_TOKEN}` 读取它。
3. 以与宿主相同的权限运行注入器：

   ```
   zzz_injector.exe --pid <一条龙GUI进程PID> [--token-env ZZZ_BRIDGE_TOKEN] [--port 23101]
   ```

   `start-service.ps1 -TargetPid <PID>` 是同样的入口。注入前校验双方 x64、宿主已
   加载 `python3xx.dll`；同路径 DLL 已加载时跳过重复注入直接走控制面；注入后
   通过 `/status` 验证桥已定位到唯一 `ZContext` 才算成功。
4. Sleepy Doll 侧重载扩展，`zzz.http.status` 可用即连接成功。

## 支持形态与限制

- 支持：源码运行、带 `.runtime` 与 `src` 的发布包（宿主进程内嵌 CPython 且为
  x64，且已加载 PySide6 并存在 QApplication 实例——一条龙 GUI 天然满足；本桥
  不要求用户安装任何 Python 或 Qt 环境）。宿主身份检查为：x64、已加载
  python3xx.dll、PySide6 调度上下文可用、注入后 `/status` 附着验证。
- 不支持：纯冻结 exe（无法注入）；非 3.11/3.12 的 CPython 未经核实，绑定解析
  失败会拒绝附着而不是硬跑。
- 原生程序的限制（如实说明）：超时只能返回"结果未知"，不能终止宿主内的线程
  （绝不 `TerminateThread`）；加载超时的远程路径内存保留到宿主退出；DLL 被
  pin 到宿主进程生命周期，从不主动卸载；控制面与桥的工作线程随宿主进程退出
  由操作系统回收，不做优雅关闭；宿主 GUI 退出事件循环后 QApplication 失效，
  此窗口内到达的新请求可能投递失败——桥会如实报错，不静默降级。
- 卸载：关闭宿主 GUI 即可。

## 安全语义

- 桥只反射真实 `ZContext`（`sys.modules` + `gc.get_objects()` 定位唯一实例，
  找不到或多个实例一律拒绝，不造假上下文），绝不构造新上下文、不调用
  `ctx.init()`、不执行任何 Python 源码字符串（无 PyRun/eval/exec）。
- 所有宿主读写经已加载 PySide6 的 context 队列投递到 GUI 主线程闭包内完成；
  排队超时跳过、执行中超时如实报 unknown，均不自动重试。闭包只触碰固定
  白名单属性与方法。
- 游戏执行只发生在用户明确调用 `zzz.http.api.invoke`（method=run/stop）时；
  `accepted/submitted` 只代表请求已提交，不代表执行完成。`stop_running` 是
  逻辑停止（运行状态机收口），不代表后台工作线程已结束；确认窗口内未观察到
  停止则如实返回 `unknown`。
- 配置端只读写 `factory._config_cache[f"{instance_idx}_{group_id}"]` 真实缓存
  （不调用 `get_config` 懒创建，只读操作不落盘）；`configHash` 校验、写入与
  回读核验在同一个主线程闭包内完成，失败恢复快照。通知账号与密钥类字段值被
  屏蔽且禁止读写；路径、脚本、流水线、启动命令类字段禁止写入；值只接受安全
  简单标量与浅层集合。
- HTTP 仅监听回环；校验 Host 精确匹配、拒绝携带 Origin/Referer 的浏览器请求、
  常量时间令牌比较、请求大小与读写超时上限、并发连接上限；桥停止期间新请求
  一律拒绝。

## 错误码（控制面回执 / 注入器退出信息）

| 码 | 含义 |
| --- | --- |
| 0 | 启动成功（实际结果以 `/status` 验证为准） |
| 101/102/103/104 | ABI/端口/令牌过短/令牌含非可见 ASCII |
| 105 | 宿主没有可用的 PySide6 GUI 调度上下文（无 PySide6 或无现存 QApplication） |
| 110/111/112 | 无已加载 CPython / C API 解析失败 / 解释器未初始化 |
| 120/121/122 | WinSock 初始化 / socket / 绑定监听失败 |
| 130 | 桥已在启动、运行或停止中（拒绝双启动） |
| 140 | 控制面管道未就绪（15 秒内无实例） |
| 141/143/144 | 管道连接 / 写入 / 回执协议失败 |
| 142 | 管道服务器 PID 与显式目标不符（拒绝发送令牌） |

## 验证状态

- 已验证：源码编译（MSVC C++20、静态 CRT，零警告）；纯逻辑测试由主代理的
  私有桌面隔离 runner（`tests/plugins/run-native.ps1 -Plugin zzz`）实跑
  **105 passed / 0 failed**（含 Qt poster 的 fake 场景：稍后执行、迟到回调取消、
  context 销毁丢弃、投递失败、运行中超时）；导入表守卫通过——测试 exe 无
  socket、无管道、无注入 API；桥 DLL 与注入器均为静态 CRT 且不链接任何 Python
  或 Qt DLL（poster 全部经运行时 C API 反射）；源码层面全插件只有一次
  `CreateRemoteThread`（目标为系统 `LoadLibraryW`），DLL 无任何供远程线程调用
  的导出。
- 未验证：真实宿主注入与运行（未注入任何实际一条龙进程，未在真实 PySide6
  上调用 `QTimer.singleShot`，未连接真实 `ZContext`，未运行任何游戏任务）。
  原生宿主和游戏端到端验收保留为未完成；本轮遵守不运行目标工具的限制。
