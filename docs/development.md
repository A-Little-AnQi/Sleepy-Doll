# 开发与验证

## 环境与命令

Windows 构建需要 Rust stable（edition 2024）、Node.js 22.13+、.NET 8 SDK、Visual Studio 2022 C++ Build Tools（x64）。桌面运行需要 WebView2 Runtime；Linux 桌面开发另需 GTK/WebKitGTK。

```powershell
npm install
npm run dist
```

| 命令 | 用途 |
| --- | --- |
| `npm run dev` | Vite 界面开发服务器 |
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

`package-lock.json` 与 `Cargo.lock` 为本机文件，Git 忽略它们；CI 使用 `npm install`。构建脚本把 `TEMP`、`TMP` 指向 `target/tmp/`，npm 缓存在 `target/npm-cache/`。

## 输出与数据

| 路径 | 内容 |
| --- | --- |
| `target/debug/`、`target/release/` | Rust 构建及各自的开发用户数据 |
| `target/ui/` | 前端生产构建 |
| `target/bridge/` | 桥组件 |
| `target/dotnet/` | .NET 与 native 构建缓存 |
| `target/setup/` | 安装器内嵌载荷 |
| `target/reports/` | 可重新生成的检查报告 |
| `target/.tmp/` | 隔离测试、截图与工具临时数据 |
| `dist/Sleepy-Doll/` | 本机组装目录：主程序、bridge、plugins 与 user |
| `dist/Sleepy-Doll-<版本>-setup.exe` | 分发安装包 |

构建与安装覆盖程序文件，保留用户数据和凭据。安装器按文件清单打包并使用空凭据模板，排除 `user/`。安装与发布细节见 [发布文档](distribution.md)，数据解析规则见 [配置文档](configuration.md)。

Rust 缓存清理保留 EXE、用户数据、桥、前端、安装载荷和 `dist/`，只清理 debug/release 中的 deps、build、incremental、.fingerprint 与顶层调试符号、Rust 库文件。编译进行中或目录含链接时拒绝清理。删除通道为 `E:\tools\Remove-Directory.ps1`；不要使用 `cargo clean` 或删除整个 `target/`。

## 验证

```powershell
npm run build
npm test
cargo fmt --all -- --check
cargo check
cargo check --no-default-features
cargo test --no-default-features --lib
cargo clippy --all-targets --no-default-features -- -D warnings
python bgi-bridge/dev/feature-guide-builder.py --check
```

CI 执行前端、发布脚本、Rust 格式／编译／库测试及功能索引一致性检查；clippy 在本地执行。

`tests/ui/` 用 Playwright 验证实际 React 界面，`tests/desktop/` 验证窗口与托盘，`tests/runtime/` 验证运行时行为。`bgi-bridge/dev/` 保留桥专项 C# 检查、元数据生成器及宿主接口盘点工具。需要模型或真实宿主的验证必须单独说明环境与范围，不能算作游戏全流程通过。

## 浏览器预览

分别执行 `npm run backend` 与 `npm run dev`。网关优先使用 47124，端口写入 `target/dev/ipc.port`，开发配置和状态在 `target/dev/user`；旧 `.sleepy-doll/dev` 首次启动时自动迁移，目录外的自定义路径保持原指向。Vite 优先使用 5173，并按端口文件转发 `/ipc`。两端占用时均可顺延。浏览器预览调用真实 AppController，不自带模型或 BGI 模拟实现。

## BGI 索引与报告

桥内嵌源码文档是构建输入，更新宿主版本时重新生成：

```powershell
dotnet run --project bgi-bridge/dev/MetadataBuilder.csproj -- <BGI源码根目录> bgi-bridge/managed/generated/host-documentation.json
python bgi-bridge/dev/coverage-audit.py <BGI源码根目录>
```

覆盖报告与现场契约快照生成到 `target/reports/bgi/`。确认需要更新功能索引的基准时，对盘点命令加 `--update-fixture`，再执行 `python bgi-bridge/dev/feature-guide-builder.py`。基准位于 `bgi-bridge/dev/fixtures/host-features.json`，口语目标测试数据位于同目录的 `user-scenarios.json`；它们供索引生成和 Rust 测试读取，不能当作现场运行证据。

`interface-review.py <类型契约JSON>` 将接口审阅报告写到相同报告目录。源码索引、隔离验证、宿主任务记录与实际游戏结果分别判定；Job 完成、接口返回或编译成功均不能单独证明业务成功。
