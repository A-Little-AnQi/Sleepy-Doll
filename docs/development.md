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
把 `target/ui/` 编进去）和 `cargo build --release`，最后组装出交付目录：

```text
dist\Sleepy-Doll\
  sleepy-doll.exe        主程序
  bridge\                9 个 BgiBridge.* 文件与 bridge.config.json
  skills\                随产品分发的能力包
```

桥组件收在 `bridge\` 子目录里并保持整目录在一起，运行期数据不落在那里。

组装是**原地覆盖写入，不先清空产物目录**：桥的 DLL 被运行中的 BetterGI 加载时删不掉，
先删会留下半个不可用的安装目录。构建失败时逐个列出没替换成的文件，其余组件保持可用。
`dist\Sleepy-Doll\user\` 是用户自己的数据，构建不碰它。

中间产物只有一个落点：仓库的 `target\`。Cargo 输出在 `target\release\` 与 `target\debug\`，
桥组件在 `target\bridge\`，.NET 中间目录在 `target\dotnet\`，界面构建在 `target\ui\`。
除交付目录 `dist\Sleepy-Doll\` 外都不参与分发。

单独构建桥执行 `bgi-bridge/build.cmd`。桥组件被宿主加载时会锁定文件，重新构建前必须先退出
BetterGI；`bgi-bridge/dev/dev-rebuild.cmd` 已包含该步骤。

开发时 `cargo run` 的可执行文件在 `target/<profile>/` 下，同级没有交付布局的 `bridge\`，
会自动回退到仓库的 `target\bridge`。debug 与 release 各自使用独立的 `user/` 目录，配置解析
规则见 [配置与数据存放](./configuration.md)。

桌面 EXE 通过 Windows manifest 在启动时请求管理员权限，桥的开关不另外提权。
打包使用 `bridge.config.example.json`，不带开发凭据。

## 安装包与发布

安装程序与产品共用一套外壳（tao + wry + React，界面在 `web/src/setup/`），外观与主程序一致。
它由 `build-desktop.cmd` 一并产出，没有单独的命令：

```cmd
build-desktop.cmd
```

产出两个东西：`dist\Sleepy-Doll\`（便携目录）与 `dist\Sleepy-Doll-<版本>-setup.exe`。
安装程序把交付目录打成压缩载荷（`installer/pack-payload.ps1`，排除 `user\`）并嵌进
`sleepy-doll-setup.exe`。安装器是独立的 feature，普通 `cargo build --release` 不会编译到它。

安装行为：

- 默认装到 `D:\Sleepy Doll`（D 盘存在且为固定磁盘时），否则 `%LOCALAPPDATA%\Programs\Sleepy Doll`。
- 用户选择的目录若最后一段不是产品名，补上 `Sleepy Doll` 并把结果写回界面；静默安装同样追加。
- 系统目录被拒绝；目标目录非空且没有 `sleepy-doll.exe` 时先征求确认。
- 覆盖安装不覆盖 `bridge\bridge.config.json`（里面是本机 token，程序把同一个 token 也写进了
  `user\config.json`，两者必须相等）。旧的平铺布局升级时先把该文件迁进 `bridge\`。
- `user\` 在安装与覆盖安装时都不动；卸载默认保留，只有用户显式勾选才删。

发布由 `.github/workflows/release.yml` 执行，推 `v*` tag 或手动触发。它先以 `workflow_call`
调用 `check.yml` 并要求发布作业依赖它，然后构建、打包便携 zip，最后创建 GitHub Release。
tag 的版本号必须与 `Cargo.toml` 的 `package.version` 一致，安装程序按这个版本号判断新旧。

## 测试

前端按职责组织：`product.css` 管理设计变量、基础样式和共享控件，页面样式与页面组件同目录，
自定义下拉框通过 CSS Modules 隔离。`web/src/session/` 持有会话事件订阅与增量游标；页面只订阅视图状态，
切换页面不取消后台任务。输入草稿与当前会话保存在浏览器本地存储。

`npm test` 运行前端交互回归测试（会话后台订阅、事件去重、下拉键盘操作）。
`npm run check` 执行类型检查及生产构建。响应超时可在模型设置中调整；IPC 普通请求、事件长轮询和
桥加载采用不同的请求期限。模型只在收到响应体之前重试临时故障，部分流式响应不会重放。

`web/src/components/chat/Transcript.tsx` 按用户轮次组织助手消息，按调用 ID 关联工具返回，合并相邻工具记录。
默认显示紧凑摘要，参数和返回数据在二级详情中展开。历史 Markdown 文本独立 memo，
避免每次流式增量都重新解析整段历史。

```bash
cargo test --no-default-features   # 全部测试
cargo test --lib --no-default-features   # 仅单元测试
```

CI 还会执行 `cargo fmt --all -- --check`、`cargo check` 和
`cargo check --no-default-features`。

## 浏览器预览

桌面壳走原生 IPC。浏览器开发只需要本地 HTTP 网关，转发到同一个 `AppController`，
不造假模型、也不造假 BetterGI。首次写入的配置与发行模板相同：没有模型。

```bash
npm run backend   # 127.0.0.1:47124/ipc，占用则顺延，端口写入 .sleepy-doll/dev/ipc.port
npm run dev       # Vite http://127.0.0.1:5173/，占用则顺延；/ipc 按上述文件转发
```

开发网关优先绑定 47124，被占则向后找空位。Vite 每次转发 `/ipc` 时读取端口文件，因此两端不必同一次启动就锁死同一端口。Vite 自己的 5173 被占时也会顺延；网关按 Origin 是否为本机回环决定 CORS，不写死 5173。

## 桥契约回归

修改桥或宿主版本后，重新生成源码文档索引并运行契约测试：

```bash
dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- <BetterGI 源码根目录> <输出 JSON 路径>
dotnet run --project bgi-bridge/dev/ContractTests.csproj
```

第一个参数是 BetterGI 源码树的根目录，第二个参数是索引的输出路径；仓库内的索引固定在
`bgi-bridge/managed/generated/host-documentation.json`，重新生成时要写回该路径（它作为嵌入资源
编进桥）。

索引识别嵌套类型、RelayCommand 命名转换、源码注释、实际 XAML 绑定、快捷键及样式设置说明。
人工补充说明位于 SettingDocumentation.cs / CommandDocumentation.cs；新增接口没有业务说明时，
真实宿主审计必须失败，不能仅用字段名或占位文案通过。

实机回归（`tests/bridge_control.rs`，标了 `#[ignore]`，默认套件跳过）只用于指定测试安装、管理员
环境：先执行 `cargo test --no-default-features --test bridge_control --no-run`，再以管理员权限
运行输出的测试 EXE，参数为 `--ignored --exact real_bridge_switch_round_trip`；
`bgi-bridge/dev/test-desktop.ps1` 可指定测试 EXE、BetterGI 成品路径和已有结果目录。它覆盖宿主
识别、状态读取、错误 token、关闭后拒绝旧客户端、工具目录热更新与反复开关，逐页核对全部目录项
及示例，读取所有配置项，并对日志详细程度开关做预览、提交、回退，检查配置值整体恢复；不执行
游戏命令。离线恢复测试使用临时假安装，要求真实 BetterGI 已退出。
