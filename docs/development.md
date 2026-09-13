# 开发环境

## 依赖

- Rust stable，edition 2024
- Node.js 22+
- Windows 10/11：WebView2 Runtime（通常随系统或 Edge 安装）
- 注入桥：.NET 8 SDK、Visual Studio 2022 C++ Build Tools（x64）
- Linux 桌面开发：Wry 所需的 GTK/WebKitGTK 开发包

## 构建与运行

```bash
npm ci
build-desktop.cmd      # 桥 + 界面 + release EXE，组装到 dist\Sleepy-Doll\
```

`build-desktop.cmd` 是唯一的发布构建入口，每次重建整个 `dist\`（含 `dist\Sleepy-Doll\user\`），
产物只落在 `dist\Sleepy-Doll\`。它内部依次执行
`bgi-bridge/build.cmd`、`npm run check`（含 `vite build`，release 二进制靠 `rust-embed`
把 `ui-dist/` 编进去）和 `cargo build --release`，最后把 EXE 与桥组件组装到一起。

`target\` 是 Cargo 的中间目录，`bgi-bridge\.build\` 是桥各 .NET 项目的中间目录，都不参与分发。

单独构建桥执行 `bgi-bridge/build.cmd`。桥组件被宿主加载时会锁定文件，重新构建前必须先退出
BetterGI；`bgi-bridge/dev/dev-rebuild.cmd` 已包含该步骤。

开发时 `cargo run` 的可执行文件在 `target/<profile>/` 下，找不到旁边的桥组件，会自动回退到
仓库的 `bgi-bridge/dist`。debug、release、mock 各自使用独立的 `user/` 目录，配置解析规则见
[配置与数据存放](./configuration.md)。

桌面 EXE 通过 Windows manifest 在启动时请求管理员权限，桥的开关不另外提权。
打包使用 `bridge.config.example.json`，不带开发凭据。

实际连接测试：先运行 `cargo test --no-default-features --test bridge_control --no-run`，
然后以管理员权限执行输出的测试 EXE，参数为 `--ignored --exact real_bridge_switch_round_trip`。
可用 `bgi-bridge/dev/test-desktop.ps1` 指定测试 EXE、BetterGI 成品路径和已有结果目录。
测试覆盖宿主识别、状态读取、错误 token、关闭后拒绝旧客户端、工具目录热更新和反复开关，
不执行游戏操作。默认测试套件会跳过这项实机测试。

## 测试

前端按职责组织：`product.css` 管理设计变量、基础样式和共享控件，页面样式与页面组件同目录，
自定义下拉框通过 CSS Modules 隔离。`session.ts` 持有会话事件订阅与增量游标；页面只订阅视图状态，
切换页面不取消后台任务。输入草稿与当前会话保存在浏览器本地存储。

`npm test` 运行前端交互回归测试（会话后台订阅、事件去重、下拉键盘操作）。
`npm run check` 执行类型检查及生产构建。响应超时可在模型设置中调整；IPC 普通请求、事件长轮询和
桥加载采用不同的请求期限。模型只在收到响应体之前重试临时故障，部分流式响应不会重放。

Mock 流式响应使用逐帧 flush 的 HTTP chunked 写入，避免 tiny_http 默认 chunk encoder
把 token-sized 数据积攒到大缓冲区。流式回归必须验证首个 delta 在结束前到达、至少有多次增量，
不能只检查最终文字。一次默认时序实测：修复前首块约 12.6 秒、共 2 块；修复后约 0.38 秒、
共 168 块（随机抖动下的样本，不是延迟保证）。

`Transcript.tsx` 按用户轮次组织助手消息，按调用 ID 关联工具返回，合并相邻工具记录。
默认显示紧凑摘要，参数和返回数据在二级详情中展开。历史 Markdown 文本独立 memo，
避免每次流式增量都重新解析整段历史。

```bash
cargo test --no-default-features --features mock   # 全部测试
cargo test --lib --no-default-features --features mock   # 仅单元测试
```

`--features mock` 是必需的：驱动 `AppController` 的测试套件都通过 Mock Backend 运行，而该
feature 不在默认集合里。CI 还会跑 `cargo fmt --all -- --check`、`cargo check` 和
`cargo check --no-default-features`。

## 离线 Mock Backend

### 桥契约回归

修改桥或宿主版本后，重新生成源码文档索引并运行契约测试：

    dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- E:/BetterGIProject/better-genshin-impact E:/BetterGIProject/Sleepy-Doll/bgi-bridge/managed/Catalog/host-documentation.json
    dotnet run --project bgi-bridge/dev/ContractTests.csproj

索引识别嵌套类型、RelayCommand 命名转换、源码注释、实际 XAML 绑定、快捷键及样式设置说明。
人工补充说明位于 SettingDocumentation.cs / CommandDocumentation.cs；新增接口没有业务说明时，
真实宿主审计必须失败，不能仅用字段名或占位文案通过。

忽略的 real_bridge_switch_round_trip 集成测试仅用于指定测试安装、管理员环境。
它逐页核对全部目录项及示例，读取所有配置项，并对日志详细程度开关做预览、提交、回退，
检查配置值整体恢复；不执行游戏命令。离线恢复测试使用临时假安装，要求真实 BetterGI 已退出。

Mock Backend 是独立 Rust 进程，模拟模型 API、BGI Bridge 和浏览器开发模式下的 IPC 网关。
它只监听 `127.0.0.1`，不访问外网，模型用量固定为 0。

它是**开发工具，不在发布版中**：由非默认的 `mock` feature 门控，默认构建与 release 二进制
既不编译 `src/mock.rs`，也不链接其 HTTP 依赖。它固定读取仓库根的
`sleepy-doll.mock.config.json`（编译期路径），不接受命令行参数，因此开发数据不会因为启动
位置不同而散落。

```bash
npm run mock   # 启动 mock 后端
npm run dev    # 启动 Vite 开发服务器
```

Mock 进程启动时会打印实际读取的配置路径与当前 `activeModel`。浏览器开发模式的 IPC 会把
模型请求转发给配置里的 `activeModel`，因此要完全离线，请让 `activeModel` 指向 Mock 模型。

浏览器开发模式会自动连接 `http://127.0.0.1:47124/ipc`；Wry 桌面模式仍使用原生 IPC，不受
影响。该端点绑定整个 `AppController`，所以它只接受 `http://localhost:5173`、
`http://127.0.0.1:5173`、`http://[::1]:5173` 和 `http://localhost:4173` 这几个开发来源
（精确匹配）；带其他 `Origin` 的浏览器请求会被 403 拒绝。

### 模拟的节奏

首个 Token 前约 220–510ms，流式输出按 1–5 个字符一块推进，块间延迟在约 50–110ms 内抖动；
句子标点处停顿更久，偶发 260–640ms 的上游停顿。可通过 `SLEEPY_DOLL_MOCK_MODEL_DELAY_MS`
和 `SLEEPY_DOLL_MOCK_CHUNK_DELAY_MS` 调整两个基准值，设为 `0` 可完全关闭对应延迟与抖动。

### 内置交流场景

- 包含“短回复”：返回单句，用于检查紧凑消息布局。
- 包含“长回复”：返回多段、编号和长行，用于检查换行、滚动、固定输入区与长会话标题。
- 包含“状态”或“连接”：模型调用 `bgi.state.get`，再根据 Mock Bridge 返回生成总结。
- 包含“路线”：模型调用 `bgi.capability.search`，再生成能力目录总结。
- Bridge 能力 `mock.success`、`mock.unknown`、`mock.failure`、`mock.busy`、`mock.denied`
  分别覆盖成功、结果未知、失败、忙碌和权限拒绝；Job 还支持运行中与取消。
