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

`build-desktop.cmd` 是唯一的发布构建入口，产物落在 `dist\Sleepy-Doll\`。它内部依次执行
`bgi-bridge/build.cmd`、`npm run check`（含 `vite build`，release 二进制靠 `rust-embed`
把 `ui-dist/` 编进去）和 `cargo build --release`，最后把 EXE 与桥组件组装到一起。

组装是**原地覆盖写入，不先清空产物目录**：桥的 DLL 被运行中的 BetterGI 加载时删不掉，
先删会留下半个不可用的安装目录。构建失败时逐个列出没替换成的文件，其余组件保持可用。
`dist\Sleepy-Doll\user\` 是用户自己的数据，构建不碰它；只有早先误落在 `dist\` 根目录的散落
文件会被清掉。

`target\` 是 Cargo 的中间目录，`bgi-bridge\.build\` 是桥各 .NET 项目的中间目录，都不参与分发。

单独构建桥执行 `bgi-bridge/build.cmd`。桥组件被宿主加载时会锁定文件，重新构建前必须先退出
BetterGI；`bgi-bridge/dev/dev-rebuild.cmd` 已包含该步骤。

开发时 `cargo run` 的可执行文件在 `target/<profile>/` 下，找不到旁边的桥组件，会自动回退到
仓库的 `bgi-bridge/dist`。debug 与 release 各自使用独立的 `user/` 目录，配置解析规则见
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

`Transcript.tsx` 按用户轮次组织助手消息，按调用 ID 关联工具返回，合并相邻工具记录。
默认显示紧凑摘要，参数和返回数据在二级详情中展开。历史 Markdown 文本独立 memo，
避免每次流式增量都重新解析整段历史。

```bash
cargo test --no-default-features   # 全部测试
cargo test --lib --no-default-features   # 仅单元测试
```

CI 还会跑 `cargo fmt --all -- --check`、`cargo check` 和
`cargo check --no-default-features`。

## 浏览器预览

桌面壳走原生 IPC。浏览器开发只需要本地 HTTP 网关，转发到同一个 `AppController`，
不造假模型、也不造假 BetterGI。首次写入的配置与发行模板相同：没有模型。

```bash
npm run backend   # 127.0.0.1:47124/ipc
npm run dev       # Vite http://127.0.0.1:5173/
```

## 桥契约回归

修改桥或宿主版本后，重新生成源码文档索引并运行契约测试：

    dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- E:/BetterGIProject/better-genshin-impact E:/BetterGIProject/Sleepy-Doll/bgi-bridge/managed/Catalog/host-documentation.json
    dotnet run --project bgi-bridge/dev/ContractTests.csproj

索引识别嵌套类型、RelayCommand 命名转换、源码注释、实际 XAML 绑定、快捷键及样式设置说明。
人工补充说明位于 SettingDocumentation.cs / CommandDocumentation.cs；新增接口没有业务说明时，
真实宿主审计必须失败，不能仅用字段名或占位文案通过。

忽略的 real_bridge_switch_round_trip 集成测试仅用于指定测试安装、管理员环境。
它逐页核对全部目录项及示例，读取所有配置项，并对日志详细程度开关做预览、提交、回退，
检查配置值整体恢复；不执行游戏命令。离线恢复测试使用临时假安装，要求真实 BetterGI 已退出。
