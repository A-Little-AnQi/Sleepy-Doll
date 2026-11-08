# MaaEnd 独立适配插件

C++ 原生桥包装 MaaEnd（MXU）已有本地 API，使用与 Tauri GUI 共享的 MaaState 和实时任务声明。它不是对 Rust 私有对象进行 CLR 反射，也不需要 Python、Node 或额外 .NET 运行时。桥、清单和技能仅属于本插件。

## 使用

交付目录为 `dist/plugins/maaend/`，在 Sleepy-Doll 中导入该目录后按需启用。由用户手动启动 MaaEnd、启用本机 Web 服务，并准备实例、控制器和资源；桥不自动启动 GUI、不扫描端口或新建实例。

为桥和 Sleepy-Doll 设置同一份进程环境变量 `MAAEND_BRIDGE_TOKEN`。在交付目录运行 `start-bridge.ps1`，后端端口不是默认 12701 时使用 `-UpstreamPort`。桥默认监听 `127.0.0.1:23121`。令牌不进入插件清单或包体。

开发构建使用 `bridge/build.cmd`，输出在 `target/bridge/maaend/`；`--test` 只编译夹具，不运行。生产包无需源码和编译器，不依赖本机 `test/local-adapters/` 资料。

## 任务与选项

工具注册为 `maaend.http.*`：status、catalog.search、catalog.describe、read、invoke、task.stop、config.read、config.update。通用插件/MCP 生态仍由主程序负责，本桥没有上游 MCP 依赖。

- invoke 需要实时目录 hash、实例 ID 与任务名称，读取该实例既有 selectedTasks 和 optionValues；同名任务重复时必须给出 selectedTaskId，不猜第一条。停用、未绑定或不兼容的任务明确拒绝。
- 按 GUI 的顺序生成任务自身、全局、资源、控制器、任务及嵌套选项的 pipeline_override，处理 select、switch、checkbox、input 和 hotkey；未保存值使用声明默认值，未知类型在提交前拒绝。
- 热键映射数据依据 MXU 提交 d556cce 的键码表，区分 Win32、MacOS、Adb、Linux，并遵守 Linux 的 use_win32_vk_code 标记；支持主键、两枚修饰键与声明占位符。这里只生成数值参数，桥不发送键鼠输入。
- 若 interface.agent 有声明，使用现有 MaaEnd 后端的 tasks/start 路径准备必要 Agent 并提交任务；Agent 参数和 PI 环境来自实时声明及后端状态，模型不能提供任意命令、cwd、agent_configs 或原始 pipeline。无 Agent 声明使用 tasks/run。
- 只有与提交数量一致的 taskIds 回执才算 accepted。HTTP 2xx 但回执损坏、提交/停止响应丢失均记为结果未知，不自动重发；接纳不等于业务完成。
- 任务需要的密码若只保存为加密密文，桥不猜解密方式，也不按空值代替；此类声明会在提交前报告不支持。特殊前端内置任务及未知选项类型同样明确拒绝。

## 配置与连接边界

配置更新用 catalogHash 和 baseRevision 双证据，通过官方版本化保存 API 更新一个实例内既有任务的顺序、启停与已声明的安全选项。热键值按声明和键名集合校验；密码、文件路径、预执行程序、Agent 声明、原始 pipeline、计划等不开放给模型写入。其它实例、未知字段及凭据原样保留，并回读核验。

只读配置是经过筛选和脱敏的视图。敏感输入与密文不进入模型，正常热键只在声明与结构校验通过后展示。缺少 config/snapshot 时只读可明确回退，版本化写入与依赖保存配置的任务运行拒绝降级。

WinHTTP 仅连接指定回环端口，关闭代理和重定向；WinSock 仅绑定 127.0.0.1，检查 Host、Origin、令牌和请求大小，限制并发与读写等待。没有任意 URL、任意命令、预任务、提权重启、截图/点击或实例增删透传。

## 验证状态

契约依据为 MaaEnd v3.0.0 源码、官方 Windows 包，以及 MXU 源码提交 d556cce。包内 EXE 的静态路由标记与字段名已核对；这不能证明运行接口已经通过验收。

原生程序和纯夹具分别构建。`tests/plugins/run-native.ps1 -Plugin maaend` 检查夹具导入表，并在私有桌面、清空凭据环境、单进程 Job 中执行 FakeTransport 测试。测试包括选项实际进入请求、官方全局 Keymap 声明、重复任务精确选择、Agent 参数来源、坏回执 unknown、CAS、字段保留和实例隔离。

本轮未启动真实 MaaEnd、生产桥或游戏，不把编译和夹具成功当作真实宿主、Agent 启动或游戏执行已验证。交付包只含本插件清单、技能、原生桥与说明，不含第三方工具、源码、测试程序或用户配置。
