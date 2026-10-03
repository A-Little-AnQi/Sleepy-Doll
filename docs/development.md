# 开发环境

在本地构建、运行与发布 Sleepy Doll 所需的全部信息：工具链依赖、构建命令与产物布局、安装包行为、CI 检查、浏览器预览开发网关，以及桥源码文档索引的再生成。

**适用读者**：参与本仓库开发的贡献者。

## 快速要点

- `npm ci && npm run dist` 是唯一的发布构建入口，产物在 `dist\Sleepy-Doll\` 与 `dist\Sleepy-Doll-<版本>-setup.exe`。
- 工具链：Rust stable（edition 2024）、Node.js 22+、.NET 8 SDK、VS 2022 C++ Build Tools（x64）、WebView2 Runtime。
- 构建是**原地覆盖写入**，不清空产物目录——桥 DLL 被运行中的 BetterGI 加载时删不掉。
- 前端浏览器预览：`npm run backend` + `npm run dev`，走本地 HTTP 网关转发到同一个 `AppController`。

## 目录

- [依赖](#依赖)
- [构建与运行](#构建与运行)
- [产物布局](#产物布局)
- [安装包与发布](#安装包与发布)
- [编译检查](#编译检查)
- [浏览器预览](#浏览器预览)
- [桥源码索引](#桥源码索引)

## 依赖

| 依赖 | 用途 |
| --- | --- |
| Rust stable（edition 2024） | 桌面壳与 Agent 运行时 |
| Node.js 22+ | 前端与构建脚本 |
| Windows 10/11 + WebView2 Runtime | 桌面 WebView（通常随系统或 Edge 安装） |
| .NET 8 SDK、Visual Studio 2022 C++ Build Tools（x64） | 注入桥 |
| GTK/WebKitGTK 开发包 | Linux 桌面开发（Wry 依赖） |

## 构建与运行

```bash
npm ci
npm run dist           # 发布构建入口，与执行 build-desktop.cmd 等价
```

`npm run dist` 执行 `build-desktop.cmd`，内部依次：

1. `bgi-bridge/build.cmd` 构建桥组件；
2. `npm run build`（含 `tsc` 类型检查与 `vite build`，release 二进制靠 `rust-embed` 把 `target/ui/` 编进去）；
3. `cargo build --release`；
4. 组装交付目录。

单独构建桥执行 `bgi-bridge/build.cmd`；桥组件被宿主加载时会锁定文件，重新构建前必须先退出 BetterGI——`bgi-bridge/dev/dev-rebuild.cmd` 已包含该步骤。

开发时 `cargo run` 的可执行文件在 `target/<profile>/` 下，同级没有交付布局的 `bridge\`，会自动回退到仓库的 `target\bridge`。debug 与 release 各自使用独立的 `user/` 目录，配置解析规则见[配置与数据存放](./configuration.md)。

桌面 EXE 通过 Windows manifest 在启动时请求管理员权限，桥的开关不另外提权。打包使用 `bridge.config.example.json`，不带开发凭据。

## 产物布局

```text
dist\Sleepy-Doll\
  sleepy-doll.exe        主程序
  bridge\                9 个 BgiBridge.* 文件与 bridge.config.json
  plugins\bgi\           随产品分发的插件：BetterGI 的领域说明
```

- 桥组件收在 `bridge\` 子目录里并保持整目录在一起；运行期数据不落在那里。
- `dist\Sleepy-Doll\user\` 是用户自己的数据，构建不碰它。
- 中间产物只有一个落点：仓库的 `target\`——Cargo 输出在 `target\release\` 与 `target\debug\`，桥组件在 `target\bridge\`，.NET 中间目录在 `target\dotnet\`，界面构建在 `target\ui\`。除交付目录外都不参与分发。

> **为什么覆盖而不清空**：桥的 DLL 被运行中的 BetterGI 加载时删不掉，先删会留下半个不可用的安装目录。构建失败时逐个列出没替换成的文件，其余组件保持可用。

### Rust 缓存体积与清理

开发与测试构建默认关闭增量编译，调试信息使用 `debug = 1`，保留回溯符号，省去完整的类型与变量调试信息。修改源码后的重新编译可能更慢；Cargo 仍会复用未变化的依赖产物。不同 feature 组合和工具链版本仍可能留下旧依赖，体积过大时执行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File ./clean-build-cache.ps1 -WhatIf
npm run clean:rust
```

清理脚本只删除 `target/debug` 与 `target/release` 中的 `incremental`、`deps`、`build`、`.fingerprint`，以及这两层目录里的 `.pdb`、`.rlib`、`.rmeta` 文件。已有 EXE、开发用户数据、WebView2 数据、桥组件、前端构建、安装载荷和 `dist` 均保留。清理后首次 Rust 构建会重新编译依赖；编译进行中或缓存目录含链接/目录联接时，脚本拒绝清理。不要用 `cargo clean` 或删除整个 `target` 来代替，它们会连同开发数据与其他构建产物一起删除。

## 安装包与发布

安装程序与产品共用一套外壳（tao + wry + React，界面在 `web/src/setup/`），外观与主程序一致，由 `build-desktop.cmd` 一并产出，没有单独的命令：

```cmd
build-desktop.cmd
```

产出两个东西：`dist\Sleepy-Doll\`（便携目录）与 `dist\Sleepy-Doll-<版本>-setup.exe`。安装程序把交付目录打成压缩载荷（`installer/pack-payload.ps1`，排除 `user\`）并嵌进安装器；安装器是独立的 feature，普通 `cargo build --release` 不会编译到它。

### 安装行为

- 默认装到 `D:\Sleepy Doll`（D 盘存在且为固定磁盘时），否则 `%LOCALAPPDATA%\Programs\Sleepy Doll`。
- 检测到注册表中的已有安装时进入更新模式，显示当前版本与目标版本；直接使用原安装目录，不再追加产品名、不重新创建快捷方式，更新卸载入口与版本登记。
- 用户选择的目录若最后一段不是产品名，补上 `Sleepy Doll` 并把结果写回界面；静默安装同样追加。
- 系统目录被拒绝；目标目录非空、没有 `sleepy-doll.exe`、也没有 `user\` 时先征求确认。卸载保留数据后目录里只剩 `user\`，不触发该询问。
- 覆盖安装不覆盖 `bridge\bridge.config.json`（里面是本机 token，程序把同一个 token 也写进了 `user\config.json`，两者必须相等）；旧的平铺布局升级时先把该文件迁进 `bridge\`。
- `user\` 在安装与覆盖安装时都不动；卸载默认保留，只有用户显式勾选才删。
- 卸载删掉载荷清单里的每一项，`bridge\bridge.config.json` 也在其列——它是安装写下的程序文件；token 不靠它保存，程序每次连接都会把 `user\config.json` 里的那份写回去。

### 发布流程

发布由 `.github/workflows/release.yml` 执行，推 `v*` tag 或手动触发：先以 `workflow_call` 调用 `check.yml` 并要求发布作业依赖它，然后构建、打包便携 zip，最后创建 GitHub Release。

> tag 的版本号必须与 `Cargo.toml` 的 `package.version` 一致，安装程序按这个版本号判断新旧。

## 编译检查

- `npm run typecheck` 只做前端类型检查；`npm run build` 执行类型检查及生产构建。
- CI 还执行 `cargo fmt --all -- --check`、`cargo check` 与 `cargo check --no-default-features`。
- 本地提交前执行 `cargo clippy --all-targets --no-default-features -- -D warnings`（CI 不执行 clippy，本地必须执行）。

### 前端组织

- `product.css` 管理设计变量、基础样式和共享控件；页面样式与页面组件同目录；自定义下拉框通过 CSS Modules 隔离。
- `web/src/session/` 持有会话事件订阅与增量游标；页面只订阅视图状态，切换页面不取消后台任务。输入草稿与当前会话保存在浏览器本地存储。
- `web/src/components/chat/Transcript.tsx` 按用户轮次组织助手消息，按调用 ID 关联工具返回，合并相邻工具记录；默认显示紧凑摘要，参数和返回数据在二级详情中展开；历史 Markdown 文本独立 memo，流式增量不重新解析整段历史。
- 响应超时可在模型设置中调整；IPC 普通请求、事件长轮询和桥加载采用不同的请求期限。模型只在收到响应体之前重试临时故障，部分流式响应不会重放。

## 浏览器预览

桌面壳走原生 IPC；浏览器开发只需要一个本地 HTTP 网关，转发到同一个 `AppController`。开发环境里没有模型与 BetterGI 的模拟实现，首次写入的配置与发行模板相同：没有模型。

```bash
npm run backend   # 127.0.0.1:47124/ipc，占用则顺延，端口写入 .sleepy-doll/dev/ipc.port
npm run dev       # Vite http://127.0.0.1:5173/，占用则顺延；/ipc 按上述文件转发
```

开发网关优先绑定 47124，被占则向后找空位。Vite 每次转发 `/ipc` 时读取端口文件，两端不必同一次启动就锁死同一端口。Vite 自己的 5173 被占时也会顺延；网关按 Origin 是否为本机回环决定 CORS，不写死 5173。

## 桥源码索引

修改桥或宿主版本后，重新生成源码文档索引：

```bash
dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- <BetterGI 源码根目录> <输出 JSON 路径>
```

- 第一个参数是 BetterGI 源码树的根目录；第二个参数是索引输出路径。仓库内的索引固定在 `bgi-bridge/managed/generated/host-documentation.json`，重新生成时要写回该路径（它作为嵌入资源编进桥）。
- 索引识别嵌套类型、RelayCommand 命名转换、源码注释、实际 XAML 绑定、快捷键及样式设置说明。
- 人工补充说明位于 `SettingDocumentation.cs` / `CommandDocumentation.cs`；新增接口缺少业务说明时，真实宿主审计判为失败——字段名与占位文案不作为通过依据。

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [配置与数据存放](./configuration.md) | 开发时 `user/` 的落点与解析规则 |
| [BGI 宿主契约](./bgi/host-contracts.md) | 桥与宿主的接口约束 |
