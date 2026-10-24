# 开发与验证

## 环境与命令

Windows 构建需要 Rust stable（edition 2024）、Node.js 22.18+、.NET 8 SDK、Visual Studio 2022 C++ Build Tools（x64）。主程序运行需要 WebView2 Runtime；安装器使用 Slint 原生界面，启动不依赖 WebView2。Linux 桌面开发另需 GTK/WebKitGTK。

```powershell
npm install
npm run dist
```

| 命令 | 用途 |
| --- | --- |
| `npm run dev` | Vite 界面开发服务器 |
| `npm run frame` | Loom 组件与主题工作台，默认 `http://127.0.0.1:4321/` |
| `npm run frame:setup -- <框架tgz路径> <安卓插件tgz路径>` | 独立安装可选的本地预览工具，不写入产品依赖 |
| `npm run frame:typecheck` | 单独检查预览适配类型，需要先安装本地工具 |
| `npm run frame:build` | 构建静态组件工作台至 `target/frame/` |
| `npm run frame:serve` | 预览已构建的组件工作台 |
| `npm run test:frame` | 本地产品页面、Frame 组件及两端视觉一致性检查 |
| `npm run backend` | 本地开发 IPC 网关 |
| `npm run desktop` | 完整构建并启动组装后的桌面程序 |
| `npm run build` | 发布配置校验、前端类型检查与生产构建 |
| `npm run typecheck` | 只检查前端类型 |
| `npm run dist` | 桥、前端、桌面程序与安装包的完整构建 |
| `npm test` | 分发服务与发布脚本测试 |
| `npm run format:check` | 前端格式检查 |
| `npm run format` | 格式化前端 |
| `npm run clean:rust -- -WhatIf` | 预览可清理的 Rust 缓存 |
| `npm run clean:rust` | 清理上述缓存 |

完整构建由 `scripts/build.cmd` 执行，内部调用 `bgi-bridge/build.cmd` 和 `scripts/pack-payload.ps1`。单独修改桥时可执行 `bgi-bridge/build.cmd`。桥通过影拷贝加载；重建不强制结束 BetterGI，连接后核对实际加载版本。

安装器入口为 `src/bin/sleepy-doll-setup.rs`，原生界面为 `src/setup/ui.slint`，由 `setup` 特性构建并嵌入。Vite 只构建主程序和托盘界面。安装器固定使用深色样式，不读取主程序主题设置；要求管理员权限，窗口首次显示时居中。运行时检测、安装、更新和卸载通过 Rust 安装引擎执行。

`package-lock.json` 与 `Cargo.lock` 为本机文件，Git 忽略它们；CI 使用 `npm install`。构建脚本把 `TEMP`、`TMP` 指向 `target/tmp/`，npm 缓存在 `target/npm-cache/`。

## 输出与数据

| 路径 | 内容 |
| --- | --- |
| `target/debug/`、`target/release/` | Rust 标准构建结果与缓存 |
| `target/ui/` | 前端生产构建 |
| `target/bridge/` | 桥组件 |
| `target/dotnet/` | .NET 与 native 构建缓存 |
| `target/setup/staging/` | 清单组装的安装包暂存目录，结束后自动清理 |
| `target/setup/payload.bin`、`target/setup/payload.json` | 安装器内嵌载荷，结束后自动清理 |
| `target/dev/user/` | 开发 IPC 网关配置与用户数据 |
| `target/dev/desktop/` | 通过 `npm run desktop` 启动的组装版程序及用户数据 |
| `target/android/maven/` | Android 组件本地 Maven 仓库 |
| `target/reports/` | 可重新生成的检查报告 |
| `target/.tmp/` | 隔离测试、截图与工具临时数据 |
| `dist/Sleepy-Doll-<版本>-setup.exe` | 当前版本分发安装包，配套 `.sha256` 与 `release.json` |

`dist/` 按根目录 [AGENTS.md](../AGENTS.md) 的白名单验收。构建前发现未知交付项会停止；不会把已有运行数据当成暂存文件删除。目录不得用于启动程序，Android 本地 Maven 仓库也不再写入 `dist/android/`。

构建与安装覆盖程序文件，保留用户数据和凭据。安装器按文件清单打包并使用空凭据模板，排除 `user/`。安装与发布细节见 [发布文档](distribution.md)，数据解析规则见 [配置文档](configuration.md)。

Rust 缓存清理保留 EXE、用户数据、桥、前端、安装载荷和 `dist/`，只清理 debug/release 中的 deps、build、incremental、.fingerprint 与顶层调试符号、Rust 库文件。编译进行中或目录含链接时拒绝清理。工程清理入口为 `scripts/remove-build-output.ps1`，在维护工作站自动转交规定的本机删除通道；不要使用 `cargo clean` 或删除整个 `target/`。

## 验证

```powershell
npm run check:repo
npm run build
npm test
cargo fmt --all -- --check
cargo check
cargo check --no-default-features
cargo test --no-default-features --lib
cargo test --no-default-features --lib extension::mcp::tests
cargo clippy --all-targets --no-default-features -- -D warnings
python bgi-bridge/dev/feature-guide-builder.py --check
```

CI 执行前端、发布脚本、Rust 格式／编译／库测试及功能索引一致性检查；clippy 在本地执行。

MCP 协议测试使用 `tests/runtime/mcp-fixture.mjs` 作为独立 Node 测试端，覆盖 stdio、HTTP JSON/SSE、有会话与无会话服务、取消通知、输出契约、错误，以及 BGI 工具迁移前后的契约一致性。测试端只绑定随机回环端口，退出时结束本任务子进程；不连接真实游戏或模型。

`tests/ui/` 用 Playwright 验证实际 React 界面，`tests/desktop/` 验证窗口与托盘，`tests/runtime/` 验证运行时行为。`bgi-bridge/dev/` 保留桥专项 C# 检查、元数据生成器及宿主接口盘点工具。需要模型或真实宿主的验证必须单独说明环境与范围，不能算作游戏全流程通过。

## 浏览器预览

通用控件、弹层、图标、品牌资源、输入容器、扩展条目和任务卡片位于内部 workspace 包 `packages/ui/`，主程序和托盘通过 `@sleepy-doll/ui` 导入。包不依赖产品 IPC 或会话状态；`ProductUiProvider` 提供产品文案，任务卡片的业务适配留在 `web/src/components/tasks/TaskCard.tsx`。CSS 和资源只有组件包中的一份实现，页面继续负责布局、数据和操作。Slint 安装器独立编译，界面验证通过原生安装器进行。

组件库是仓库内的正式模块，正常 `npm install`、`npm run build` 和 GitHub 构建无需 Moduloom。Moduloom 是独立的本地可选工具，`frame/package.json` 不属于产品 workspace，也不记录未发布包或机器路径。需要预览时执行 `npm run frame:setup -- <loom-frame.tgz完整路径> <android-plugin.tgz完整路径>`：安装位置是被 Git 忽略的 `frame/node_modules/`，不会写入根依赖或根 lock 文件。普通 Web 预览可使用轻量安卓插件；需要自带模拟器时使用 Windows 完整包。工具安装不包含在产品或安装包里。

根 `tsconfig.json` 只检查产品和内部组件库；预览适配单独使用 `frame/tsconfig.json`，通过 `npm run frame:typecheck` 检查。`npm test` 会检查产品依赖与实际 TypeScript 模块图，防止本地预览工具重新进入默认构建链路。

组件和主题调整优先使用 `npm run frame`，无需运行开发后端。`frame/catalog.ts` 登记真实参数和基础依赖，`frame/renderer.tsx` 直接调用内部包，并将交互回写工作台。现有 57 个条目包括 28 个控件、组合、样式上下文和品牌条目，以及 29 个描边图标。`frame.config.ts` 从正式 CSS 读取默认配色；默认明暗、圆角和字号保持产品原值。强调色仅映射品牌金色，主按钮与用户消息保留各自语义。自定义设计变量使用正式 CSS 变量名；布局、字号、圆角、动效、边框与阴影只有使用对应变量的部位会响应。

工作台设置默认存在当前浏览器 IndexedDB，端口与 namespace 共同决定存储范围。主题调整保存在工作台中；复制的主题 JSON 不会自动改写产品 CSS。组件结构、样式或资源修改则直接来自 workspace 包，两端会热更新。产品页面仍用于检查应用布局、业务状态和原生宿主行为，Frame 用于日常组件检查。

`npm run test:frame` 使用演示 IPC 在独立临时端口运行真实产品和 Frame；不会访问模型或执行游戏操作。抽取前可用 `node tests/ui/frame-ui-check.mjs --baseline` 保存 56 个产品页面状态，之后检查图片差异；只容许少量边缘像素的 1 色阶栅格化差异。两端组件另行核对完整样式树、文字、逐项几何和非文字图像，避免把不同合成上下文中的文字抗锯齿当成设计变化。基准、截图及检查数据位于 `target/.tmp/frame-integration/`，完成核对后走删除脚本清理。Frame 启动脚本按命令隔离运行时目录 `target/tmp/frame/dev/`、`build/` 和 `serve/`，退出时清理各自目录；开发缓存位于 `node_modules/.cache/loom-frame/`。

分别执行 `npm run backend` 与 `npm run dev`。网关优先使用 47124，端口写入 `target/dev/ipc.port`，开发配置和状态在 `target/dev/user`；旧 `.sleepy-doll/dev` 首次启动时自动迁移，目录外的自定义路径保持原指向。Vite 优先使用 5173，并按端口文件转发 `/ipc`。两端占用时均可顺延。浏览器预览调用真实 AppController，不自带模型或 BGI 模拟实现。

## BGI 索引与报告

桥内嵌源码文档是构建输入，更新宿主版本时重新生成：

```powershell
dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- <BGI源码根目录> bgi-bridge/managed/generated/host-documentation.json
python bgi-bridge/dev/coverage-audit.py <BGI源码根目录>
```

覆盖报告与现场契约快照生成到 `target/reports/bgi/`。确认需要更新功能索引的基准时，对盘点命令加 `--update-fixture`，再执行 `python bgi-bridge/dev/feature-guide-builder.py`。基准位于 `bgi-bridge/dev/fixtures/host-features.json`，口语目标测试数据位于同目录的 `user-scenarios.json`；前者供索引生成，后者仅保存测试请求和行为约束，不参与生产检索，不能当作现场运行证据。

`interface-review.py <类型契约JSON>` 将接口审阅报告写到相同报告目录。源码索引、隔离验证、宿主任务记录与实际游戏结果分别判定；Job 完成、接口返回或编译成功均不能单独证明业务成功。


### 主模型的渐进式技能验证

`tests/runtime/progressive-skills.rs` 提供显式运行的真实模型回归：用 SLEEPY_DOLL_MODEL_CONFIG 指定已有配置，SLEEPY_DOLL_TEST_MODEL 选择其中的模型；执行 `cargo test --no-default-features --lib live_model_reads_plugin_references_and_corrects_existing_task_without_running -- --ignored --nocapture`。它走真实 Supervisor 和模型协议，读取隔离练习应用的技能正文、引用资料、配置和文件版本；两轮修改次数并验证不运行、保留其他设置，不连接 BetterGI 或游戏，不修改正式配置。临时目录由 SLEEPY_DOLL_TEST_ROOT 控制，遵守根 AGENTS 的登记和清理要求。此测试只证明通用主循环的行为，不能代替具体插件的现场验收。


### 基础工具的效率对比

`tests/runtime/skill-efficiency.rs` 的显式真实模型检查走生产 Supervisor、实际 BGI 工具登记及文件读写；仅测试目录位置由本地夹具提供。设置模型配置、模型名、阶段、报告路径及测试根目录对应的 SLEEPY_DOLL 环境变量后，运行 `cargo test --no-default-features --lib live_bgi_skill_efficiency -- --ignored --nocapture`。每个阶段包含只读查询、修改真实配置开关和源码符号关系，各两次。运行时不加载真实游戏，不把测试期望传给模型。报告记录成功率、请求、工具、实际用量及网络耗时，不能用静态目录命中代替模型任务通过。无效或不符合场景约束的样本须单列排除，不纳入有效效率比较。
