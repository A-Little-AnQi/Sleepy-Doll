# BGI 自动能力暴露与垂类 Agent 完整技术方案

版本：1.0｜日期：2026-09-07｜性质：实施设计，不代表相关功能已经实现。

代码核对基线：`babalae/better-genshin-impact`，提交 `b3e46b3004b8e4a1065846243a3a2a518b9a214d`。文中的现有类来自该提交；`Remote*`、`Capability*`、`Agent*` 等新增组件及所有 `/bridge/v1`、`/agent/v1` 接口均为本方案设计。代码片段用于说明实现结构，不是经过编译的完整补丁。

## 一、先定结论与边界

最终结构是：BGI 本体内嵌 ASP.NET Core 服务，自动发现并调用内部可远程化能力；本体外的自研 Agent 负责能力语义、任务规划、确定性执行、状态验证与对话；其他第三方 Agent 通过外层的稳定接口使用这些能力。

不再增加一个独立的“RPC 框架层”。ASP.NET Core 本身是 Web 框架，而不是 RPC 协议；但使用它实现统一的方法调用端点，就已经构成基于 HTTP 的 RPC（Remote Procedure Call，远程过程调用）。本方案直接使用 ASP.NET Core + HTTP + JSON，不要求 JSON-RPC、gRPC、MCP、动态代理框架或专用 RPC SDK。[ASP.NET Core 官方介绍](https://learn.microsoft.com/en-us/aspnet/core/)

本方案遵守以下边界：

- 自动暴露是本体的基础能力，不按传送、脚本、寻路逐个手写 HTTP Controller；新增符合发现规则的 Service/方法自动进入目录。
- 不修改 ViewModel，不把界面命令当作 Agent 接口，不强行重构原有 Service；确实缺失的能力新增专门的 Remote Service。
- LLM、业务描述、用户偏好、任务配方、模型工具定义均在本体外维护，减少向 BGI 提交 PR 的频率。
- 复用 BGI 已有传送、导航、脚本、战斗和识别实现，不让 LLM 重新操作每次按键或逐帧控制角色。
- 自己写 Agent 运行时，不依赖 Agent 编排框架；继续使用 .NET、HTTP、JSON、SQLite 等基础库，不把“全套自己写”理解成重写数据库、HTTP 服务或基础模型。
- 广泛可发现不等于任何 CLR 对象均能安全远程调用。无法序列化、依赖 UI 或缺少创建上下文的方法必须明确报告限制，不能假装已完整支持。

这里的“第三方 Agent”包括你自己写的外部垂类 Agent，以及未来接入它的其他 Agent。二者都不会成为 BGI 本体的一部分。

## 二、总体分层与部署

| 层 | 进程/位置 | 负责什么 | 明确不负责什么 |
| --- | --- | --- | --- |
| BGI 原有执行能力 | BGI Windows 进程 | 识别、传送、追踪、脚本和任务执行 | 对话、LLM 规划 |
| Bridge | 同一个 BGI 进程 | 自动发现、参数绑定、权限、调用、Job、状态快照 | 业务提示词和用户意图 |
| Capability 层 | 外部 Agent 进程中的模块 | 原始目录缓存、语义补充、别名、配方、结果判定、资源索引 | 逐帧游戏操作 |
| Agent Runtime | 同一外部进程 | 会话、LLM、计划、状态机、恢复、审批、持久化 | 扫描 BGI 内存或直接调用其 CLR 对象 |
| Agent API / CLI | 外部进程 API；CLI 为薄客户端 | 给用户和其他 Agent 提供任务、工具、状态入口 | 另建执行状态机 |

默认只有两个长期进程：BGI 和 Agent。Capability 层不单独部署成微服务，CLI、第三方接口和对话共用同一套运行时。

本地部署时，Agent 通过回环地址调用 Bridge。远程部署时，优先让 Agent 留在游戏电脑，远端只访问 Agent API；若 Agent 必须运行在服务器上，再通过受保护的隧道接入本地 Bridge。不要把拥有键鼠和文件能力的 Bridge 直接绑定公网。

### 技术选型

| 项目 | 选型及理由 |
| --- | --- |
| 本体 | 沿用当前 WPF + C# + .NET 8 Windows 项目，不为了 Agent 单独升级本体 |
| 本体 HTTP 服务 | ASP.NET Core Kestrel + 端点路由，和现有 Generic Host 共用生命周期及 DI |
| Bridge JSON | Newtonsoft.Json 处理新协议；已有配置等 STJ 模型继续使用原 System.Text.Json 配置 |
| 能力发现 | 启动时反射与服务注册描述快照；运行时不反复扫描程序集 |
| 任务队列 | `System.Threading.Channels` 有界队列 + 后台 Worker；持久化记录才是事实源 |
| 外部 Agent | 建议 C# / .NET 10 LTS 独立进程；HTTP 协议不要求与本体运行时同版本 |
| 数据存储 | 本机 SQLite；Agent 保存对话/计划/步骤/资源索引，Bridge 保存 Job/去重/事件 |
| 事件 | SSE（Server-Sent Events，服务器发送事件）+ 普通 HTTP 查询兜底 |
| 模型接入 | 自写 `ILlmClient` 和厂商协议适配器，使用基础 HTTP 客户端或官方基础 SDK |
| 测试 | 现有测试工程或独立 xUnit 工程；Bridge 契约测试、Fake Executor、Agent 回放测试 |

当前项目确为 `net8.0-windows10.0.22621.0`。新建外部 Agent 不必继续锁在 .NET 8；按当前官方支持表，.NET 8 支持至 2026-11-10，.NET 10 LTS 支持至 2028-11-14。本体之后的升级属于其自身维护计划。[BGI 项目文件](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/BetterGenshinImpact.csproj)、[.NET 支持政策](https://dotnet.microsoft.com/en-us/platform/support/policy/dotnet-core)

## 三、当前代码实际情况：不能只扫描 Service

### 3.1 宿主已经存在

`App.xaml.cs` 已通过 `Host.CreateDefaultBuilder()` 构建 Generic Host，并集中注册 `IConfigService`、`IScriptService`、`TaskTriggerDispatcher`、地图服务和其他对象。应接入这一个宿主，不新建一套 BGI 单例，也不对现有 `IServiceCollection` 再调用 `BuildServiceProvider()`。[App.xaml.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/App.xaml.cs)

### 3.2 能力来源至少有四种

| 来源 | 当前例子 | 自动发现/接入方式 |
| --- | --- | --- |
| DI 注册服务 | `IScriptService`、地图 API Service、配置 Service | 保存注册描述，扫描允许的契约类型，不主动实例化整个容器 |
| 脚本宿主对象 | `Genshin`、`AutoPathingScript`、`Dispatcher` | 类型级 Provider + 延迟工厂，自动扫描该类型的方法 |
| 静态能力 | `Bv`、`TaskControl` 的相关识别函数 | 仅扫描明确允许的类型；不把所有底层工具全部开放 |
| 独立任务和非远程化对象 | `PathExecutor`、`TpTask`、`ScriptProject` | 参数简单的类型可工厂接入；依赖上下文/结果不可靠者新增薄适配 Service |

`EngineExtend.InitHost` 当前就是脚本对象的集中装配位置。其宿主对象不是“DI 里全都有”。第一版可在 Remote 模块内维护这几个类型的 Provider，不修改 `EngineExtend`；同一类型以后新增公共方法会自动发现，新类别首次出现才需要新增 Provider 或规则。[EngineExtend.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Core/Script/EngineExtend.cs)

`Genshin` 当前提供的是 `Tp(double x, double y, ...)` 等重载，不能把此前对话里的 `sceneId + pointId` 示例当成现有调用签名。如果对外希望使用传送点 ID，应由外层资源索引把 ID 解析成经过核实的地图名与坐标，再绑定实际签名。[Genshin.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Core/Script/Dependence/Genshin.cs)

### 3.3 有些现有入口不适合直接充当远程执行入口

`ScriptService.StartGameTask()` 依赖 `HomePageViewModel`；`RunMulti()` 的部分逻辑访问 `ScriptControlViewModel`；脚本 `Dispatcher.RunTask()` 的部分分支读取 `TaskSettingsPageViewModel`。自动扫描可以发现它们，但不会消除这些依赖。需要可靠无 UI 入口时，新增 `RemoteSessionService`、`RemoteScriptService`、`RemoteTaskService`，读取文件/配置并调用下层能力；保留原 Service 原样。[ScriptService.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Service/ScriptService.cs)、[Dispatcher.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Core/Script/Dependence/Dispatcher.cs)

### 3.4 返回了不一定成功

`TaskRunner.RunCurrentAsync()` 在拿锁失败时直接返回，也会捕获并记录部分执行异常。`AutoPathingScript.Run()` 会捕获并记录路径执行异常。不能通过 `await` 正常返回判定业务成功。[TaskRunner.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/TaskRunner.cs)、[AutoPathingScript.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Core/Script/Dependence/AutoPathingScript.cs)

甚至 `PathExecutor.SuccessEnd` 也不能独自证明路线完整成功：当前 `HandledException` 分支会将它设为 `true`，但某些不可继续场景也会走到此类异常。外层必须区分调用完成、执行器报告与实际目标达成。[PathExecutor.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/AutoPathing/PathExecutor.cs)

## 四、本体实现：一个 Remote 模块，一组通用接口

### 4.1 文件组织

以下均为建议新增路径，位于 `BetterGenshinImpact/Remote/`：

| 子目录 | 主要文件/类 | 职责 |
| --- | --- | --- |
| Hosting | `RemoteHostExtensions`、`RemoteEndpointMapping`、`RemoteOptions` | 接入宿主，端点、安全、启动发现文件 |
| Discovery | `ServiceRegistrationSnapshot`、`ICapabilitySource`、`DiCapabilitySource`、`FactoryCapabilitySource`、`StaticCapabilitySource` | 能力来源与候选目录 |
| Contracts | `MethodDescriptor`、`InvokeRequest`、`JobSnapshot`、`GameStateSnapshot` | 网络 DTO，与 UI/CLR 实例隔离 |
| Binding | `MethodIdBuilder`、`JsonContractBuilder`、`ArgumentBinder`、`ResultMapper` | 唯一方法标识、参数和结果转换 |
| Execution | `RemoteInvoker`、`RemoteExecutionService`、`JobWorker`、`JobStore`、`EventJournal` | 执行、任务所有权、持久化与事件 |
| Services | `RemoteSessionService`、`RemoteStateService`、`RemotePathingService`、`RemoteScriptService`、`RemoteConfigService` | 只补必要的远程适配能力 |
| Security | `RemoteAuthentication`、`RemoteAuthorization`、`ExportPolicy`、`PathPolicy` | 身份、访问范围、暴露策略、文件边界 |

既有文件的初始修改集中在 `.csproj` 和 `App.xaml.cs`：增加 ASP.NET Core 框架引用，并接入 Remote 注册/宿主配置。原业务 Service 与 ViewModel 不修改。若后续要求 UI 启动和远程执行可任意混用，涉及执行所有权的额外协调另见第七节，不能藏在“零改动”承诺中。

### 4.2 ASP.NET Core 接入方式

项目增加：

```xml
<ItemGroup>
  <FrameworkReference Include="Microsoft.AspNetCore.App" />
</ItemGroup>
```

WPF 项目可以通过上述框架引用使用 ASP.NET Core API，不必把项目改成 Web SDK。[Microsoft 关于 WPF/非 Web 项目托管的说明](https://learn.microsoft.com/en-us/aspnet/core/grpc/aspnetcore?view=aspnetcore-10.0)

宿主结构示意：

```csharp
Host.CreateDefaultBuilder()
    // 保留原来的 CheckIntegration / UseElevated / UseInstanceIpc 等调用
    .ConfigureServices((context, services) =>
    {
        RegisterExistingServices(context, services); // 表示原有注册块，不要求抽走
        services.AddBgiRemoteServices();

        // 延迟到 Provider 构建完成后读取描述；只保存描述，不解析服务实例。
        services.AddSingleton(sp =>
            ServiceRegistrationSnapshot.From(services));
    })
    .ConfigureWebHostDefaults(web =>
    {
        web.UseKestrel(options =>
            options.Listen(System.Net.IPAddress.Loopback, remotePort));
        web.Configure(app =>
        {
            app.UseRouting();
            app.UseMiddleware<RemoteAuthentication>();
            app.UseMiddleware<RemoteAuthorization>();
            app.UseEndpoints(endpoints => endpoints.MapBgiRemote());
        });
    })
    .Build();
```

上面是结构示意：真实补丁需处理启用开关、端口配置来源和现有扩展调用次序。`ServiceRegistrationSnapshot` 在所有注册完成后由工厂创建；完成后 `IServiceCollection` 不再修改。发现仅保留匹配 BGI 命名空间/契约规则的描述，忽略 ASP.NET 自身服务。

端口使用独立配置，不预设 3499；支持配置端口或回环动态端口。动态端口启动后通过实际监听地址写入每实例发现文件，不能把配置中的 `0` 当可连接端口。

Bridge 启动不代表游戏已准备好：元数据接口可用，执行接口可能返回 `GAME_NOT_READY`。端口冲突应给出明确启动诊断；Remote 默认关闭。上线前验证宿主启停、BGI 退出清理、Wine/多实例路径与打包。

新增框架引用也影响部署：原先只安装桌面运行时不一定包含 ASP.NET Core 共享运行时。框架依赖发布要检查 ASP.NET Core Runtime；或更新自包含发布流程。必须在干净 Windows 环境验证，不能只看开发机能编译。

### 4.3 自动发现算法

发现过程只操作类型元数据：

1. 取得 DI 注册描述快照、受控程序集类型和预设 Provider 列表。
2. 按命名空间/程序集/类型规则筛选候选，排除 View、ViewModel、导航、窗口、框架基础设施。
3. 对候选契约扫描公共实例方法；对受控静态类型扫描公共静态方法。
4. 排除属性访问器、事件、`System.Object` 方法、释放方法、开放泛型、`async void`、`ref/out`、指针、委托等不支持签名。
5. 生成参数/返回值契约，注入型参数不出现在客户端输入中；完成方法唯一标识、默认调度策略。
6. 产生不可变 Catalog；不可调用候选进入诊断目录，保留原因。
7. 按规范化内容计算 `catalogVersion`；启动或明确重载时原子切换，执行中的 Job 继续持有原描述快照。

DI 服务优先扫描注册契约。例如通过 `IExampleService` 注册的工厂，只扫描该接口，不为了发现实现类方法而提前运行工厂。具体类注册则扫描具体类。若一个类型被多个契约重复注册，使用注册键和稳定规则去重；相同契约的多实现必须有稳定 Provider ID，不依赖注册顺序猜实例。

服务获取规则：Singleton 使用原容器实例，Scoped/Transient 在实际 Job 的 Scope 内解析，Scope 一直存活到任务执行与结果转换结束。不要缓存短生命周期实例，也不要手动 Dispose 容器拥有的 Singleton。[.NET DI 生命周期](https://learn.microsoft.com/en-us/dotnet/core/extensions/dependency-injection/overview)

自建宿主对象使用类型级工厂。例如 `Genshin` 构造时读取 `TaskContext.SystemInfo`，只能在游戏初始化后创建，不能扫描目录时 `new Genshin()`。`AutoPathingScript` 需要根目录和配置，由经过校验的执行上下文注入，不让客户端任意提交构造函数参数。

### 4.4 如何保证“新增服务自动暴露”

示意策略文件 `remote-export.json`：

```json
{
  "discovery": {
    "diContractPrefixes": [
      "BetterGenshinImpact.Service.",
      "BetterGenshinImpact.Remote.Services."
    ],
    "excludeTypePrefixes": [
      "BetterGenshinImpact.View.",
      "BetterGenshinImpact.ViewModel."
    ],
    "excludeTypeNames": ["PageService", "ApplicationHostService"]
  },
  "defaults": {
    "effect": "unknown",
    "executionMode": "job",
    "concurrency": "gameExclusive",
    "automaticRetry": false,
    "thirdPartyExposure": false
  }
}
```

这是自定义策略结构，不是 ASP.NET 原生配置。实现时应扩充 UI/基础设施排除规则，并通过整个候选目录审核验证，不声称上面三条规则已过滤完所有危险方法。

符合规则的新 DI 服务、新方法自动被发现并获得技术调用契约，不需要为每个方法添加 `[Remote]` 特性。用户授权的本机 owner/raw 客户端可以调用其可绑定方法；新方法默认不能自动变成普通第三方 Agent 的工具，也不获得自动重试或读权限。外层补充语义和授权，不需要向 BGI 提交接口 PR。

准确区分三件事：`discovered` 表示找到，`callable` 表示技术上能绑定执行，`authorized` 表示当前凭证有权调用。目录按身份过滤，不向普通客户端泄露所有内部接口。

自动化边界：

| 变化 | 需要做什么 |
| --- | --- |
| 已发现服务新增可绑定公共方法 | 自动新增目录项，无端点代码改动 |
| 在约定命名空间注册新的普通 DI 服务 | 自动发现，无逐服务 HTTP 接口 |
| 同一脚本宿主类型新增方法 | 类型 Provider 自动发现 |
| 新能力只是新增脚本/路线文件 | 外层资源索引更新，不改本体 |
| 首次出现需要特殊构造上下文的宿主对象 | 新增类型级 Provider，而不是逐方法接口 |
| 参数首次出现新的 CLR 特殊类型 | 增加转换器或专用薄适配 Service |
| 仅修改工具描述、别名、任务配方 | 只改外层配置 |
| 方法依赖 UI、吞错或没有可验证结果 | 保留诊断，新增适配或显式声明结果未知 |

### 4.5 方法唯一标识与重载

调用不使用只有 `service + method` 的模糊名称。将 Provider ID、契约全名、方法名、完整参数类型、返回类型、泛型实参规范化后计算签名摘要，生成 `methodId`。程序集版本号不进入签名，否则每次发版全部变更；参数名和默认值仍进入 schema/catalog 版本。展示名称只用于人读。

例如 `Genshin.Tp(double,double)` 与 `Genshin.Tp(string,string)` 是两个 ID。客户端按 `methodId` 调用，不采用运行时“最像哪个重载”的猜测。稳定的 `game.teleport` 别名由外层绑定到目标 ID，BGI 升级出现破坏性变化时绑定失败并要求修复。

### 4.6 参数绑定与结果序列化

支持集合从小而明确开始：字符串、布尔、有限数值、枚举、可空值、数组/List、字符串键字典、经审核的 POCO/record。使用 Newtonsoft 的实际 `JsonContract` 生成契约，处理命名规则、忽略字段和转换器；nullable 元数据可以辅助判断，但不能替代显式验证。

对于现有 STJ 特有的配置类型，走对应适配 Service 和原 `ConfigService.JsonOptions`，不要用另一套序列化规则直接往返原对象。

| 类型/情况 | 处理 |
| --- | --- |
| `CancellationToken` / 可空 Token | 注入当前 Job Token，客户端不能提交 |
| 可选参数与 nullable | 区分缺失、显式 null、默认值；缺失必填参数拒绝 |
| `Task` / `Task<T>` / `ValueTask` / `ValueTask<T>` | 完整 await 后解包；注册时为每种返回类型缓存解包器 |
| 已知 OpenCV 点/矩形值类型 | 显式转 JSON 坐标 DTO，同时标明坐标系 |
| `Mat`、Bitmap、ImageRegion、Stream | 不默认暴露；经适配输出有期限的 Artifact ID |
| `object`、dynamic、ScriptObject | 不默认当任意 JSON；需要显式契约或薄适配 |
| Type、委托、指针、COM、UI 对象、CTS | 不支持远程传入；诊断目录说明原因 |
| `IAsyncEnumerable<T>` | 第一版不通用支持，后续以 Artifact/事件适配 |
| POCO 内含危险或无法绑定的成员 | 整个契约判为需适配，不能只看最外层类型 |

统一拒绝重复 JSON 属性、未知参数、非有限浮点数、数值溢出、超深嵌套和超大集合。设置 `TypeNameHandling.None`，禁止 `$type` 驱动的任意 CLR 类型构造；不支持通过请求加载程序集、执行反射表达式或调用客户端传来的完整类型名。

目录 schema 可输出 JSON Schema 的受限兼容子集，包括 `type/properties/required/items/enum/additionalProperties/$defs/$ref` 和实际使用的约束。实现一个和绑定器共用契约模型的校验器，不宣称手写校验器支持全部 JSON Schema。递归模型使用引用和深度限制；无法处理的契约不标 `callable`。

调用路径是：鉴权 → Catalog 查找 → 权限/策略 → 契约验证 → 幂等接纳/持久化 → Worker 调度 → Scope/实例解析 → `MethodInfo.Invoke` → await 解包 → DTO 转换 → 结果落库。反射调用开销相比游戏操作和识别通常不是优先优化项；需要时再对热方法编译委托。

## 五、HTTP 协议：技术目录与统一调用

### 5.1 端点集合

| 端点 | 功能 | 关键约束 |
| --- | --- | --- |
| `GET /bridge/v1/info` | 协议、本体版本、实例、支持特性 | 不返回 Token |
| `GET /bridge/v1/catalog` | 分页列出能力摘要 | 支持 ETag、类别、可调用过滤 |
| `GET /bridge/v1/catalog/{methodId}` | 完整参数/结果 schema 与技术属性 | 按当前身份授权 |
| `GET /bridge/v1/diagnostics/discovery` | 不可远程化方法及原因 | 仅 owner |
| `POST /bridge/v1/invoke` | 所有方法的统一调用 | 写/未知能力默认创建 Job |
| `GET /bridge/v1/jobs/{jobId}` | 查询 Job/结果/证据引用 | 所有权校验 |
| `POST /bridge/v1/jobs/{jobId}/cancel` | 协作式取消 | 与执行队列分离 |
| `GET /bridge/v1/events` | SSE 事件与断线重放 | 事件序号、身份过滤、保留窗口 |
| `GET /bridge/v1/state` | 状态快照 | 被动观测，返回时间与可用性 |
| `GET /bridge/v1/artifacts/{artifactId}` | 按需取截图等证据 | 短期、鉴权、不能拼接任意路径 |

状态、Job 等固定端点是控制面的基础设施，不是给每个业务功能手写接口。后台业务能力继续只走统一 `invoke`。

### 5.2 元数据示例

以下 ID 为说明用，不是现有 BGI 已生成值：

```json
{
  "methodId": "genshin.tp.f64-f64-map-force.example",
  "displayName": "Genshin.Tp(double,double,string,bool)",
  "providerId": "script-host.genshin",
  "catalogVersion": "sha256:example",
  "inputSchema": {
    "type": "object",
    "properties": {
      "x": {"type": "number"},
      "y": {"type": "number"},
      "mapName": {"type": "string"},
      "force": {"type": "boolean"}
    },
    "required": ["x", "y", "mapName", "force"],
    "additionalProperties": false
  },
  "executionMode": "job",
  "concurrency": "gameExclusive",
  "effect": "gameWrite",
  "cancellation": "cooperative",
  "resultReliability": "completionOnly",
  "callable": true
}
```

反射自动提供技术字段；`effect`、结果可靠性等语义来自本体保守默认与受信策略覆盖，不能靠方法名推断。外层可以进一步收紧，不能通过请求擅自降低本体的权限/互斥限制。

### 5.3 调用示例

```http
POST /bridge/v1/invoke
Authorization: Bearer <bridge-token>
Content-Type: application/json
Idempotency-Key: <stable-key-for-this-step-attempt>
```

```json
{
  "requestId": "req-example",
  "instanceId": "bgi-instance-example",
  "catalogVersion": "sha256:example",
  "methodId": "genshin.tp.f64-f64-map-force.example",
  "arguments": {
    "x": 123.4,
    "y": 567.8,
    "mapName": "Teyvat",
    "force": false
  },
  "execution": {
    "deadlineMs": 120000,
    "onDisconnect": "continue"
  }
}
```

坐标仅为协议示例，不能拿来实机执行。业务动作返回 `202 Accepted`：

```json
{
  "requestId": "req-example",
  "jobId": "job-example",
  "state": "queued",
  "acceptedCatalogVersion": "sha256:example"
}
```

明确白名单中的快速纯查询可直接返回 `200 + result`；否则统一走 Job，不能根据返回类型是 `Task` 就判断一定是长任务或写操作。

### 5.4 Job 与业务成功分离

Job 生命周期：`queued → starting → running → cancelling → cancelled`，或从活动状态进入 `completed/failed/interrupted`。`stoppingUnconfirmed` 是仍可能持有执行权的非终态，不应当作已取消释放队列。

`completed` 只表示调用正常结束；业务判定放在独立的 `verification` 字段：

```json
{
  "jobId": "job-example",
  "state": "completed",
  "result": null,
  "verification": {
    "status": "unknown",
    "reason": "The method completed without an authoritative goal result"
  },
  "timestamps": {
    "acceptedAt": "2026-09-07T10:00:00Z",
    "endedAt": "2026-09-07T10:00:20Z"
  }
}
```

Agent 之后可通过独立观测将步骤判定为 `verifiedSucceeded`、`verifiedFailed` 或 `needsReview`；不能通过修改本体 Job 的原始结果制造成功。

### 5.5 错误协议

| 错误码 | 含义 | 处理 |
| --- | --- | --- |
| `METHOD_NOT_FOUND` | 方法或绑定不存在 | 刷新目录，不猜测替代方法 |
| `CATALOG_MISMATCH` | 契约版本不匹配 | 重新解析/校验计划 |
| `INVALID_ARGUMENT` | 参数不合规 | 本地修复或询问用户 |
| `UNSUPPORTED_SIGNATURE` | 类型无法远程化 | 使用适配能力 |
| `GAME_NOT_READY` | 截图/游戏未初始化 | 调用受控初始化或等待 |
| `GAME_BUSY` | 有互斥执行 | 有界等待或告知用户 |
| `PERMISSION_DENIED` | 权限不足 | 不重试，不换通道绕过 |
| `IDEMPOTENCY_CONFLICT` | 同键不同请求体 | 视为客户端错误 |
| `QUEUE_FULL` | 队列已满 | 不接受任务，退避后原键重试 |
| `EXECUTION_FAILED` | 执行异常可确认 | 按语义策略决定恢复 |
| `OUTCOME_UNKNOWN` | 无法确认业务结果 | 先观察，不盲目重发 |
| `STOP_NOT_CONFIRMED` | 底层未响应取消 | 保留互斥，人工处理 |

HTTP 状态与错误码同时保留：参数 400，认证 401，权限 403，冲突 409，容量/频率 429，服务未就绪 503，未处理服务端错误 500。客户端只收到经过清理的错误详情和 `traceId`，不直接暴露完整栈、密钥或任意本地路径。

错误响应体是平铺的 `{code, message}`（未处理异常为 `code=INTERNAL`，客户端根本认不出的响应才是 `REQUEST_FAILED`）。客户端必须把 `message` 一起交给 Agent：缺项、拒绝原因和被拒后该做什么都在里面，只报错误码等于把可执行的信息丢掉。

### 5.6 幂等、崩溃与事件

去重键至少绑定 `principalId + instanceId + idempotencyKey`，保存规范化请求 Hash。接纳 Job、记录去重映射在同一 SQLite 事务中完成；并发重复请求由唯一约束裁决。同键同内容返回原 Job，同键不同内容返回 409。

网络重发使用原键；用户明确重新执行、或经规则允许的下一次业务尝试使用新 attempt 和新键。去重防止重复接纳，不证明游戏操作具备幂等性。

先持久化 Job，再交给有界 Channel 唤醒 Worker。若落库后入队前崩溃，重启扫描 `queued` 恢复；若执行后落结果前崩溃，标记 `interrupted/outcomeUnknown` 并重新观测，绝不承诺物理操作的 exactly-once。SQLite 与游戏世界不存在共同事务。

SSE 事件包含 `eventId/jobId/traceId/type/timestamp/data`。为每个订阅者提供独立有界缓冲，不能让所有订阅者读取同一个竞争消费 Channel。状态事件先写 EventJournal 再广播；高频日志可合并/丢弃。断线使用 `Last-Event-ID` 重放；序号过旧则明确返回缺口提示，客户端重新读取 Job/状态快照。取消接口不能排在一个长任务后面等待执行。[.NET Channels](https://learn.microsoft.com/en-us/dotnet/core/extensions/channels)

## 六、专门补齐游戏状态与非 UI 入口

### 6.1 RemoteSessionService：初始化真实运行环境

不调用 `HomePageViewModel.OnStartTriggerAsync()`。新增 Session Service，根据本实例允许的游戏进程选择窗口，使用已有窗口定位/配置逻辑，调用 `TaskTriggerDispatcher.Start(hWnd, mode, interval)` 初始化截图和 `TaskContext`。多窗口存在歧义时要求绑定实例，不靠窗口标题随便选一个。

当前 `TaskTriggerDispatcher.Start()` 会初始化截图器、任务上下文、触发器和窗口事件钩子，是可以复用的底层入口。生命周期操作按实际线程要求派发；其内部仍可能使用现有遮罩设施，本方案“不操作 UI”指不通过 UI 命令编排，不意味着把 WPF 程序瞬间变成完全无界面后台服务。[TaskTriggerDispatcher.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/TaskTriggerDispatcher.cs)

`EnsureReady` 必须幂等，检查已有 GameCapture/窗口/上下文是否一致。失败时清理本次创建的资源，不重复注册窗口钩子；不能只看 `TaskContext.IsInitialized` 一个布尔值。第一版不承诺无人值守自动登录，登录/更新界面可返回 `needsUserAction`。

### 6.2 RemoteStateService：观测而不是猜测

至少返回以下层次：

| 状态 | 来源与限制 |
| --- | --- |
| BGI 实例、窗口/进程、截图可用性 | `InstanceContext`、`TaskContext`、截图器和窗口检查 |
| 当前 Bridge Job、队列、独占状态 | JobStore 与执行所有权记录，不靠解析日志 |
| 当前 UI 类别 | 复用 `Bv.WhichGameUi`；当前类别只有 Unknown/Main/Talk/BigMap |
| 秘境/弹窗/低血量/运动状态等 | 独立识别字段，只在适用画面下返回 |
| 地图与位置 | 复用匹配能力，携带地图 ID、坐标系、观测时间和有效性 |
| 队伍 | 优先无副作用识别；缓存信息必须标明非实时事实 |
| 证据 | frameId 与可选截图 Artifact，按需取，不每轮发给模型 |

示例：

```json
{
  "instanceId": "bgi-instance-example",
  "snapshotId": "snapshot-example",
  "observedAt": "2026-09-07T10:00:00Z",
  "frameId": "frame-example",
  "runtime": {
    "captureReady": true,
    "windowActive": true,
    "activeJobId": null
  },
  "ui": {
    "value": "main",
    "status": "observed",
    "confidence": null,
    "source": "Bv.WhichGameUi"
  },
  "position": {
    "value": null,
    "status": "unknown",
    "reason": "Not localized on this frame"
  }
}
```

已有布尔识别器没有校准置信度时，`confidence` 返回 null，不凭空写 0.99。识别不到位置返回 unknown，不能返回 `(0,0)`。低血量布尔值不是完整生命值；队伍缓存也不是随时准确的当前角色状态。

单次快照尽量使用同一截图执行多种识别，并释放 Mat/ImageRegion。复用已有捕获链，不为每个 GET 创建高频捕获循环。采样频率配置化，先以空闲 1 Hz、任务期 2–5 Hz 为调优起点，实测截图和识别开销后调整，不承诺这些值适合所有机器。

当前 `CaptureToRectArea()` 会进入实际捕获链，不能假定它就是一个并发安全的只读缓存。实现时核查具体截图后端与现有任务的并发访问契约；无法证明线程安全时，使用已捕获帧的受控拷贝/缓存发布，或增加窄范围捕获同步接点。仅给 Remote 自己加锁不能同步旧执行路径。

被动观测不得改变游戏界面。例如 `RunnerContext.GetCombatScenes()` 会返回主界面后识别，不能直接放进被动 GET；可以参考其静默识别入口。必须打开地图/背包才能获得的信息，另建显式的 `ObserveWithInteraction` 能力，标记游戏写操作并进入互斥队列。[BvStatus.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/Common/BgiVision/BvStatus.cs)、[RunnerContext.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/RunnerContext.cs)

### 6.3 RemotePathingService / RemoteScriptService

`RemotePathingService.Run` 读取经过验证的路线资源，使用 `PathingTask.BuildFromJson` 和 `PathExecutor`，传入当前 Job Token。保留执行器报告、取消原因、起止状态和可用的路线进度。无法确认完整完成时返回 `unknown`，不把 `SuccessEnd` 包装成确定的完成证明。

`RemoteScriptService.Run` 按资源 ID 找到本地受信任脚本，验证目录和 `manifest.json`，调用现有 `ScriptProject.ExecuteAsync`。设置数据经清洗转换为宿主需要的形态，不调用设置 UI。配置组按文件读取并由新增服务编排，只有在确实需要兼容对应功能时实现优先组/重复规则，不悄悄略过原有语义。

直接复用的脚本可能继续调用旧宿主，旧宿主内部吞错仍然存在。因此“脚本执行无异常”也不能等同于“采集目标完成”。对需要强结果的业务，优先专门的薄适配或后置验证。任意新生成脚本的执行不是普通工具默认权限，需审核、资源登记和明确授权。

### 6.4 文件操作与配置操作分开

路线、脚本等本来按运行时读取的文件，可由本地外层管理，或经 Remote 受限资源服务操作；采用内容 Hash、版本、备份、同目录临时文件加原子替换。运行中的任务固定资源版本，不能边执行边替换成另一份内容。

`User/config.json` 不能简单外部覆盖后宣称即时生效。当前 `ConfigService` 缓存 `Config`，绑定自动保存回调；旧内存对象还可能把外部改动覆盖回去。新增 `RemoteConfigService` 对允许字段做定点更新，在原配置要求的线程上更新内存并调用原保存流程；或仅提供“空闲/重启后生效”的文件编辑模式。不要通过 `Read()` 替换全局引用后假定所有订阅已自动迁移。[ConfigService.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Service/ConfigService.cs)

## 七、执行控制：互斥、取消与现有全局状态

### 7.1 一个游戏实例同一时间只有一个写任务

Bridge Worker 对 `gameWrite/unknown` 串行执行，真正进入游戏执行前复用现有 `TaskControl.TaskSemaphore`。外层排队不等于本体已经加锁，两层都需要：外层保证计划顺序，本体保证所有客户端共享的执行边界。

为每个方法记录一种执行入口模式：

- `bridgeManaged`：直接调用底层能力，由 RemoteExecutionService 管锁、上下文和清理。
- `selfManaged`：原方法内部已有 TaskRunner/锁，Bridge 不能再套同一把锁；未经审核不作为推荐执行入口。
- `readOnly`：只有被确认不会写共享上下文/输入/配置的查询才可并发，结果使用快照 DTO。

不能先拿 `TaskSemaphore` 再调用 `TaskRunner.RunCurrentAsync()`：它会再次试图拿锁并直接返回。未知方法也不能靠名称判断是不是自带 Runner；发现目录需保留待审核状态，支持用户 owner 的受控原始调用，但垂类 Agent 正常路径优先选择可靠薄适配。

### 7.2 新执行服务的生命周期

`RemoteExecutionService` 不调用吞异常的 `RunCurrentAsync` 来判结果。可复用 `TaskRunner.Init/End` 的现有准备/收尾能力，并由新服务掌握异常、锁和持久化。结构如下：

```csharp
// 伪代码：细节由实际类型、线程规则和所有权校验补齐。
await AcquireExistingTaskSemaphoreAsync(job);
try
{
    await EnsureSessionReadyAsync(job);
    // 只有获得执行权后，才能初始化全局脚本取消上下文。
    CancellationContext.Instance.Set();
    var capturedCts = CancellationContext.Instance.Cts;
    using var registration = job.Token.Register(() => TryCancel(capturedCts));
    RunnerContext.Instance.Clear();

    try
    {
        await InitializeRunnerOnRequiredThreadAsync();
        var result = await InvokeTargetAndAwaitAsync(job);
        await PersistInvocationOutcomeAsync(job, result);
    }
    finally
    {
        await CleanupRunnerOnRequiredThreadAsync();
    }
}
finally
{
    // 按本 Job 实际取得的资源和所有权清理；清理异常不能覆盖主异常。
    ClearOwnedCancellationAndRunnerContext();
    ReleaseExistingTaskSemaphore();
}
```

真实实现必须在资源创建过程中逐项记账，保证 Init 失败也释放已持有资源；取消注册解除后才释放 CTS；日志/清理/持久化分别捕获错误，避免 End 抛错导致锁永不释放。全局上下文清理前校验 owner，不能清理另一条执行链的 CTS。

HTTP 线程不直接执行长任务，也不使用无人监管的 fire-and-forget。Worker 持有 Task 并统一观察异常。WPF/STA 操作只派发短生命周期操作，不把整个导航/战斗放在 UI Dispatcher 上跑。

### 7.3 必须承认的现有并发限制

当前 `TaskRunner.RunSoloTaskAsync()`、`ScriptService.RunMulti()`，以及一条龙 ViewModel 的某些入口，会在拿执行锁前调用全局 `CancellationContext.Set()`。因此，仅在 Remote 模块拿到 `TaskSemaphore`，并不能阻止用户同时点 UI 后覆盖全局 CTS。扫描目录或加一个外层队列都解决不了这个问题。[CancellationContext.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Core/Script/CancellationContext.cs)、[TaskRunner.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/GameTask/TaskRunner.cs)

按“不改 ViewModel、不重构原 Service”的约束，第一版明确运行模式：该 BGI 实例进入远程控制期间，不同时从 UI/热键启动另一组业务任务；已有全局停止可作为人工急停。服务端对可观测冲突返回 busy，检测到上下文被替换则停止接纳新写任务并报告执行所有权丢失。

这是一项运行约束，不是完全隔离保证。若要支持任意 UI/Agent 混用，必须增加共享执行所有权机制，在原初始化/取消入口发生作用；可以进一步研究集中改造 CancellationContext/TaskRunner 等基础设施，但不能在未验证前承诺“仅新增 Service 就能保证”。不擅自把修改 ViewModel 当实施前提，也不为了本方案扩散重构已有 Service。

多 BGI 实例可以分别拥有队列与 Job，但如果两个实例仍向同一个桌面使用前台 SendInput，它们依然可能争抢焦点。按 Windows Session/输入目标建立更高层互斥，或使用真正隔离的会话。不同端口不等于游戏输入已经隔离。[InstanceContext.cs](https://github.com/babalae/better-genshin-impact/blob/b3e46b3004b8e4a1065846243a3a2a518b9a214d/BetterGenshinImpact/Service/Instance/InstanceContext.cs)

### 7.4 取消和超时不等于终止

Job 接纳后生命周期不绑定 HTTP `RequestAborted`；连接断开默认继续，客户端用 Job ID 重连。`cancel` 只取消指定 Job 的 CTS；调用全局 CancellationContext 前必须确认它仍属于该 Job。不能把“取消 job A”实现为“无条件停止现在的任何任务”。

排队任务可以立即取消；执行任务进入 cancelling，等待底层协作退出和释放所有按键。超过宽限时间则进入 `stoppingUnconfirmed`，保留互斥并拒绝后续游戏写任务，不能用 `Task.WhenAny` 超时后丢下仍在按键的 Task 就释放锁。

同进程中无法安全强杀任意 .NET Task。对不响应取消的第三方脚本/原生调用，不承诺无损强制停止；最终恢复可能需要人工结束该 BGI 进程。进程退出、服务停机、任务失败时都应尽最大努力释放键鼠输入，但不能把“发出了松键”当作旧任务已经不能再次按键的证明。

## 八、外层 Capability：让 Agent 理解，但不把全目录塞进模型

### 8.1 两份目录，而不是一份混合物

本体提供 Raw Catalog，描述真实可调用的技术契约。外层保存 Capability Catalog，描述“这个能力有什么用、什么时候能用、执行后检查什么”。自动导入原始方法后，即使没有语义补充也能在 owner 的开发目录中找到；普通 Agent 工具集合只包含已授权、可解释的能力。

外层文件建议：`capabilities/*.json`、`recipes/*.json`、`policies/*.json`、`knowledge/`。配置热加载必须先校验完整快照，再原子替换；正在运行的计划固定版本，不能执行一半被热更新改变动作。

### 8.2 能力描述示例

```json
{
  "id": "game.teleport",
  "version": "1.0.0",
  "description": "传送到资源目录中已解析的地图位置",
  "binding": {
    "providerId": "script-host.genshin",
    "signature": "Tp(System.Double,System.Double,System.String,System.Boolean)"
  },
  "effect": "gameWrite",
  "concurrency": "gameExclusive",
  "parameters": {
    "targetId": {"type": "string", "source": "teleport-resource-index"}
  },
  "argumentMapping": {
    "x": "target.x",
    "y": "target.y",
    "mapName": "target.mapName",
    "force": false
  },
  "preconditions": ["capture.ready", "target.resolved"],
  "postconditions": ["ui.main", "position.nearResolvedDestination"],
  "retryPolicy": "observe-before-retry",
  "exposeToThirdParty": true
}
```

以上 binding 是外层解析规则，不是允许客户端提交任意反射字符串。加载时必须解析成 Raw Catalog 中唯一的 `methodId`；解析失败就禁用能力并给出诊断。

`argumentMapping` 只支持受限字段路径和常量，`preconditions/postconditions` 使用预注册的谓词 ID，不允许配置直接执行 C#、JavaScript、Shell 或任意表达式。谓词需要声明读取字段、最大状态年龄和验证预算；地图距离必须使用正确坐标系与单位。

### 8.3 工具按需检索

LLM 初始上下文只包含目标、规则、状态摘要和少量核心工具。核心入口可以保持为 `capability.search`、`capability.describe`、`capability.invoke`、`task.status`、`task.cancel`、`state.get`，再根据当前任务注入少量完整业务 schema。

目录全量存在磁盘/内存，不等于每次全部进入 prompt。搜索第一版使用名称、分类、标签、中文别名和关键词索引；中文可用显式分词或 n-gram 字段，别直接假定 SQLite 默认英文分词能处理所有中文。规模和召回率有必要时再加向量检索，不先上向量数据库。

模型看不到的目录由程序处理。工具返回给模型的是精简结果、验证状态与必要证据引用，原始日志、完整路线和大截图留在本地，需要时再读取。

### 8.4 资源索引与垂类知识

能力目录之外单独索引脚本、地图路线、配置组、传送点和用户命名的地点。记录资源 ID、来源、版本/内容 Hash、参数 schema、兼容要求、用途标签和信任状态。地图资源必须带坐标系/地图层/版本，不把不同数据源的坐标直接混用。

LLM 负责将“去某地采矿”解析成目标和约束，资源解析器负责查真实资源。找不到路线就返回能力缺口或询问用户，不编造路线名、传送点 ID 或声称能够临时可靠生成全图导航。

游戏知识和能力语义版本化，引用外部文本、脚本 README、OCR 内容时将其视为数据，不能让其中的提示词覆盖系统策略。

## 九、全套自研 Agent Runtime

### 9.1 模块与接口

| 模块 | 建议接口 | 实现责任 |
| --- | --- | --- |
| 模型适配 | `ILlmClient` | 请求、流式输出、工具调用块、用量、错误、取消 |
| 上下文 | `IContextBuilder` | 当前目标、约束、摘要、工具、证据与 Token 预算 |
| 能力 | `ICapabilityRegistry` | 检索、描述、绑定、版本与权限 |
| 资源 | `IResourceResolver` | 路线/脚本/地点实体消歧与版本固定 |
| 规划 | `IPlanner` | 输出任务计划或澄清请求，不直接执行 |
| 验证 | `IPlanValidator` | schema、依赖、预算、权限、预后置条件 |
| 执行 | `ITaskEngine` / `IStepExecutor` | 状态转换、提交 Job、等待事件、断线重连 |
| 观测 | `IStateReader` / `IOutcomeVerifier` | 快照、时间有效性、条件验证 |
| 恢复 | `IRecoveryPolicy` | 规则重试、观察、重规划、人工处理 |
| 审批 | `IApprovalService` | 对精确动作授权并绑定版本/参数 |
| 持久化 | `IAgentStore` | 对话、计划、步骤、事件和恢复索引 |

`ILlmClient` 不绑死某一模型的接口名称。适配器统一产生文本片段、完整工具调用、完成和用量事件；流式工具 JSON 未收齐前不得执行。模型层网络重试与业务动作重试分开计数，不因为模型 API 失败重发已经运行的游戏步骤。

### 9.2 自然语言到执行的流程

1. 接收用户消息，识别是新任务、修改目标、查询还是停止；明确停止命令优先走确定性控制，不必等待 LLM。
2. 加载当前会话目标、用户授权范围、已有任务摘要和新鲜状态。
3. 检索相关能力与资源，必要时让模型输出结构化的澄清问题。
4. LLM 输出受限计划；程序校验后固定计划版本、能力版本、资源版本。
5. 审批必要动作，状态机执行下一步，调用 Bridge 并保存 Job ID。
6. Worker 根据 Job 事件与状态观测推进，不逐步要求 LLM 解释。
7. 规则可处理的异常本地处理；涉及新决策时提交精简失败证据给 LLM 重规划。
8. 结束时依据真实步骤与验证结果生成总结，明确哪些完成、哪些未知或取消。

### 9.3 计划数据结构

第一版使用有序步骤加条件，内部保留 `dependsOn` 便于以后扩展。游戏写动作仍串行，不因为模型输出 DAG 就并行操控游戏。

```json
{
  "schemaVersion": "1",
  "taskId": "task-example",
  "revision": 1,
  "goal": "运行已确认的采矿路线，完成后传送到指定位置",
  "constraints": {
    "maxDurationSec": 1800,
    "maxReplans": 2,
    "allowResourceDeletion": false
  },
  "steps": [
    {
      "id": "s1",
      "capabilityId": "pathing.run",
      "arguments": {"resourceId": "route-selected-by-resolver"},
      "dependsOn": [],
      "onUnverified": "needsReview"
    },
    {
      "id": "s2",
      "capabilityId": "game.teleport",
      "arguments": {"targetId": "target-selected-by-resolver"},
      "dependsOn": ["s1"],
      "onUnverified": "observe"
    }
  ]
}
```

校验包含：能力是否存在、参数是否正确、资源是否真实、依赖是否无环、总时长/调用次数是否有界、当前身份是否允许、是否需要确认，以及未验证步骤能否继续。模型不能通过写 `allowResourceDeletion=true` 自己提升权限；有效权限只能来自用户/系统授权。

### 9.4 自己写状态机，而非无限工具循环

任务状态：`draft → validating → awaitingApproval → queued → running → verifying → succeeded`；异常可进入 `recovering/needsReview/failed/cancelled`。需要澄清时停在 `awaitingUser`，停止时进入 `cancelling`，不能把已发取消请求立即显示为已停止。

步骤保存 attempt、Bridge Job ID、幂等键、前置状态版本、执行结果和验证证据。状态转换通过 `expectedRevision` 做乐观并发控制，以事务写入新状态和事件，防止 SSE 重放/重复 HTTP 回应重复推进下一步。

核心循环伪代码：

```csharp
while (!task.IsTerminal)
{
    var step = store.GetNextRunnableStep(task.Id);
    var snapshot = await stateReader.ReadFreshAsync(step.RequiredFacts);
    await validator.ValidatePreconditionsAsync(step, snapshot);
    await approvals.EnsureAuthorizedAsync(step);

    // 必须先落库再发网络请求；重启后可复用同一个 key 查询/重发。
    var attempt = store.GetOrCreatePendingAttempt(step);
    var job = await bridge.SubmitOrRecoverAsync(step, attempt.IdempotencyKey);
    store.BindJob(attempt.Id, job.Id);

    var invocation = await jobs.WaitOrReconnectAsync(job.Id);
    var verdict = await verifier.VerifyAsync(step, invocation);
    await transition.ApplyAsync(task, step, verdict);
    // Recovering 才进入规则恢复或模型重规划；循环不是无条件调用模型。
}
```

`SubmitOrRecoverAsync` 处理发送后丢响应、重启后未知 Job ID 等情况；无论旧 Job 是正在运行还是 completed，都先恢复旧调用，不能立即创建新尝试。

### 9.5 恢复规则

| 场景 | 优先处理 | 是否立即调用 LLM |
| --- | --- | --- |
| HTTP 断线、SSE 断线 | 原键重试/查 Job/重连事件 | 否 |
| 状态过旧或暂不可识别 | 有界重新观测 | 否 |
| 游戏正忙 | 按预算等待或排队 | 否 |
| 参数/schema 不匹配 | 刷新目录，重新验证绑定 | 仍不兼容时才需要规划/人工 |
| 已定义可恢复画面 | 运行经过授权的恢复配方 | 否 |
| 原方法返回但结果不确定 | 后置观测；不盲目重复动作 | 通常先不需要 |
| 新弹窗/路线失效/资源缺失 | 收集证据并请求新决策 | 是，或直接询问用户 |
| 权限拒绝 | 停止对应动作 | 否，不让模型绕过 |
| 无响应取消 | 保持阻塞并提示人工处理 | 不靠 LLM 强行“解决” |

同一个任务有明确步数、重试、重规划、时间与 Token 上限。预算耗尽进入 needsReview/failed，不无限循环；预算数字由产品配置，不把示例值硬写进系统行为。

### 9.6 会话、记忆和日志

SQLite 建议表：`conversations/messages/tasks/task_revisions/steps/attempts/approvals/capability_bindings/resources/events`。Bridge 另有 `jobs/idempotency_records/job_events/artifacts`，不跨进程直接共享同一个数据库文件。

消息存原文，模型上下文使用摘要和当前事实；长期偏好单独存储，未经确认不把一次任务参数永久记成用户偏好。任务真相来自结构化记录，不来自聊天摘要。

日志至少串联 `conversationId/taskId/stepId/attemptId/requestId/jobId/traceId`，记录耗时、模型用量、重试原因与观测证据，避免写 Token、敏感配置、完整截图 OCR 到普通日志。

恢复时先与 Bridge 对账：仍在跑则继续等待，已完成则继续验证，找不到且实例已重启则判定 unknown。不会从“最后一条聊天说开始采矿”推断应该重新执行整个路线。

## 十、第三方 Agent 如何接入

### 10.1 默认接外层，不直接接 Raw Bridge

普通第三方 Agent 使用 Agent Gateway，由网关执行相同的语义约束、审批和任务队列。只有明确授权的本机开发工具/高级编排器才获得 owner/raw Bridge 权限。

外层提供两种调用模式，避免双重规划：

| 模式 | 第三方输入 | 本系统行为 |
| --- | --- | --- |
| 托管任务 | 自然语言目标 + 约束 | 本系统 LLM 规划并完整执行，第三方只跟踪 Task |
| 工具模式 | capability ID + 结构化参数 | 不再调用本系统规划器，直接校验/执行/验证 |

不能第三方 LLM 已经规划好一个动作，内层又把同一句话交给 LLM 重规划成另一套动作。工具模式保持语义稳定，托管模式才接管规划。

### 10.2 外层 HTTP 接口

| 端点 | 功能 |
| --- | --- |
| `GET /agent/v1/capabilities?q=...` | 搜索授权能力摘要 |
| `GET /agent/v1/capabilities/{id}` | 完整输入、结果、风险和验证约定 |
| `POST /agent/v1/actions` | 工具模式，创建不经 LLM 规划的受控动作任务 |
| `POST /agent/v1/tasks` | 托管模式，提交目标和约束 |
| `GET /agent/v1/tasks/{id}` | 计划摘要、步骤、结果与证据 |
| `POST /agent/v1/tasks/{id}/cancel` | 取消该调用者有权控制的任务 |
| `POST /agent/v1/tasks/{id}/approval` | 有权限的人/客户端批准精确计划版本 |
| `GET /agent/v1/events` | 状态事件，按用户/客户端隔离 |
| `GET /agent/v1/state` | 去敏后的可用游戏状态 |

实现一个 `bgi-agent` CLI 薄客户端：`capabilities search/describe`、`action invoke`、`task submit/status/cancel`、`state get`，输出 JSON。它只是调用 Gateway，不携带 Bridge 的高权限 Token，不另建队列。

API 路径本身不决定 Token 开销。关键是只检索需要的工具 schema、返回摘要、由程序等待事件，而不是把全部能力和日志塞给模型。

### 10.3 不把 MCP 作为前提

第一版直接交付 HTTP API、协议说明和 CLI 即可。未来某客户端只能通过 MCP 使用工具时，可以在最外层加一个薄协议适配器，复用同一套 `capability.search/describe/invoke` 和任务接口。本体无需改动；本方案不依赖也不要求现在实现 MCP。

跨客户端不共享可写会话状态或隐式授权。每个 client/principal 有能力范围、任务所有权和取消权限；授权列表/审批由服务端执行，不能信任第三方 Agent 自称“已获用户批准”。

## 十一、安全与可靠性最低要求

Bridge 实际具有桌面输入和本地资源访问能力，安全边界必须放在本体和 Gateway 的代码里，而不只是提示词里。

| 项目 | 实现要求 |
| --- | --- |
| 默认网络 | Remote 关闭；开启后仅绑定 loopback，不绑定 `0.0.0.0` |
| 身份 | 至少 256 位随机 Token，安全随机生成，Windows ACL 保护；支持轮换，不进日志 |
| 实例发现 | 文件记录地址、PID、启动时间、instanceId、协议版本；凭证单独受保护保存 |
| 浏览器跨站访问 | 不开放任意 CORS，校验 Origin/Host；所有敏感端点鉴权，不能因来自 localhost 就信任 |
| 第三方 | 单独 Token/主体、最小权限、配额、任务所有权；不得拿到 owner Token |
| 文件路径 | 根目录约束、规范化、扩展名/大小限制，防 `..`、绝对路径、UNC、重解析点/符号链接逃逸 |
| 文件竞态 | 将资源映射为受信 ID；执行前复核真实路径/Hash，敏感写入使用版本检查 |
| 代码执行 | 不提供默认任意 Shell/Eval；新脚本按可执行代码审核，不因放进脚本目录就可信 |
| 资源消耗 | 请求体、并发、队列、事件保留、Artifact 大小/TTL、截图速率均有限制 |
| 破坏性动作 | 删除、覆盖配置、资源消耗动作需要相应授权；审批绑定计划版本、参数 Hash 与有效期 |
| 输出 | 错误与截图去敏，禁发完整配置/凭证，目录按身份过滤 |
| 审计 | 记录实际方法、参数摘要、身份、批准信息、执行/验证结果和取消原因 |

审批后修改计划、资源内容、能力绑定或关键参数，原审批自动失效。模型与外部 README/OCR 都不能批准自己的权限升级。

## 十二、实施顺序与验收

正确顺序不是“先给每个能力手写 HTTP，之后再改 RPC”，也不是“先把全部 Catalog 灌进 LLM”。先完成真实自动发现和统一调用的最小闭环，再在稳定协议上写外部 Agent。

### 阶段 1：本体基础面

增加 ASP.NET Core 宿主、安全配置、实例 info、自动 Catalog、参数绑定与统一 invoke。先用无副作用的测试服务验证通用性，再在真实 BGI 读取目录。

验收：新增一个符合规则的测试 Service 和方法后，重启即出现在目录并能调用；不新增端点、不改原 Service 加特性；重载、默认值、null、异常、Task/ValueTask 都有测试；不支持的签名有理由；目录生成不启动 UI、不实例化整个容器。

### 阶段 2：可靠执行和游戏状态

实现 JobStore、队列、幂等、取消、事件；补 Session/State/Pathing/Script 必要适配。落实远程独占运行约束并测试全局 CTS 冲突诊断，不承诺未实现的 UI 混用能力。

验收：两次相同网络请求只产生一个 Job；断线后可恢复；后台故障不误报成功；取消真实停止或明确 stoppingUnconfirmed；初始化失败不泄锁；未知位置不输出假坐标；两个游戏写任务不同时执行。

### 阶段 3：外层能力与资源

实现 Raw Catalog 缓存、语义配置、资源索引、稳定别名、schema 校验、权限和后置验证。先不用 LLM，通过 CLI 跑通“真实资源 → 执行 → 观察 → 结果”。

验收：新增本体方法自动导入开发目录；只改外层描述不需要本体 PR；签名变化能使绑定显式失效；目录很大时仍能按需获取；普通第三方拿不到 raw 权限。

### 阶段 4：全套自研 Agent

实现 ILlmClient、会话、受限计划、计划校验、任务状态机、审批、恢复、上下文预算与持久化。普通步骤程序推进，只有理解/决策时调用模型。

验收：自然语言选择真实资源并执行；信息缺失会询问；普通执行不逐步请求 LLM；重复事件不重复执行；重启能恢复任务；达到预算会停；摘要能区分完成、未知和取消。

### 阶段 5：第三方接入和打包

提供 Gateway API、CLI、机器可读 schema、兼容说明和样例。验证两种模式不发生双重规划，并完成部署运行时/端口/Token/退出清理测试。

验收：第三方无需知道 BGI 内部类即可提交任务或调用稳定能力；更换模型不影响 Bridge；修改 UI 不影响 Agent 协议；新机器安装后可以启用/关闭/升级/卸载 Remote 功能。

### 必测用例

| 类别 | 用例 |
| --- | --- |
| 自动发现 | DI 工厂、Scoped、重复注册、特殊构造、新增方法、不可绑定嵌套类型 |
| 协议 | 重载歧义、错误类型、重复字段、未知参数、版本不一致、越权方法 |
| 执行 | 锁忙、Init 异常、End 异常、内部吞错、执行完成但目标未达成 |
| 取消 | 排队取消、运行取消、取消后新 Job、过期 Job 取消、不可取消调用 |
| 网络 | 请求送达后丢响应、SSE 重放、事件缺口、重复并发提交 |
| 崩溃 | 接纳后未入队、动作后未落库、BGI 重启、Agent 重启、DB 写失败 |
| 状态 | Unknown UI、位置缺失、旧缓存、窗口变化、截图器停止、主动观察副作用 |
| 文件 | 路径逃逸、重解析点、运行中替换资源、配置缓存覆盖、审批后内容变更 |
| 多端 | 两客户端争抢、跨主体取消、UI 初始化覆盖 CTS、跨实例前台输入冲突 |
| Agent | 幻觉资源、非法计划、工具 JSON 流未完成、无限重试、提示词注入 |

这些测试中，单元测试/Fake Executor 先跑；实际键鼠与游戏联调只能在明确授权的测试会话进行。本文只完成代码核对和实施设计，没有修改或运行 BGI，也没有验证实机效果。

## 十三、最终交付清单与不做项

本体交付一个 Remote 模块，包含自动发现、统一调用、技术目录、任务管理、安全与状态服务；外部交付一个独立 Agent 工程，包含语义目录、资源索引、规划器、状态机、恢复、存储、API 和 CLI；双方以版本化 HTTP/JSON 契约连接。

第一版不做：逐能力 HTTP Controller、MCP 前置依赖、复杂 Agent 框架、微服务拆分、全量工具灌 prompt、任意 CLR 对象远程代理、LLM 逐帧操作、通用任意代码执行、无依据的“任务百分之百成功”或“UI 与 Agent 任意混用绝不会冲突”承诺。

这套设计的核心不是选择“RPC 还是 HTTP”，而是把三个边界做清楚：本体自动提供真实可调用能力；外层独立维护可理解、可验证的业务语义；Agent 使用明确授权和状态机完成任务闭环。ASP.NET Core 已经承担 HTTP 上的 RPC 服务端，不再额外套一层。
