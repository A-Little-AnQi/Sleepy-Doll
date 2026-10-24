# 贡献指南

正式源码只有当前仓库一份。目录职责与产物位置见 [AGENTS.md](AGENTS.md)，开发命令见 [开发文档](docs/development.md)，文档入口见 [docs/README.md](docs/README.md)。

## 开始开发

Windows 使用 Rust stable、Node.js 22.18+、.NET 8 SDK 和 Visual Studio 2022 C++ Build Tools。在仓库根目录运行 `npm install`；`npm run build` 构建界面，`npm run dist` 构建 Windows 安装包。首次构建会下载依赖。Moduloom 是可选预览工具，不是构建前提。Android 另需对应 SDK 与工具链。

根目录的 Cargo/npm 锁文件目前按项目策略保留在本机，CI 使用 `npm install`。修改依赖时说明原因与验证结果，不提交机器专用路径。当前依赖策略不提供锁定版本的可重复构建保证。

## 修改边界

- 通用 Agent 与 Harness 放在 `src/runtime/`、`src/model/`、`src/extension/`；BGI 适配放在 `src/bridge/`、`bgi-bridge/` 和 `plugins/bgi/`。
- 共用 React 组件放在 `packages/ui/`，产品页面放在 `web/`；不复制组件到另一套页面。
- 运行数据、截图、测试数据库与工具临时文件只使用规定的 `target/` 子目录。`dist/` 只放最终安装包及对应校验文件和清单。
- 配置模板使用空凭据或环境变量引用，不提交真实配置、数据库、模型请求记录、私钥或用户资源。
- 生成的宿主声明、功能索引与测试基准是正式构建输入；通过生成器更新，并说明来源。测试样本不能写入生产提示词来制造命中。

## 提交与验证

一个提交围绕一个可解释的功能或修复；重构、行为修改和生成数据更新尽量分别说明。提交说明写清问题、最终行为和实际验证范围。

先运行 `npm run check:repo`。前端修改执行 `npm run build`，分发与发布脚本修改执行 `npm test`。Rust 修改执行 `cargo fmt --all -- --check`、`cargo check`、`cargo check --no-default-features`、`cargo clippy --all-targets --no-default-features -- -D warnings`，再选择相关测试；完整命令与人工验证要求见开发文档。BGI 索引修改另执行 `python bgi-bridge/dev/feature-guide-builder.py --check`。

真实模型、Windows 桌面与游戏验证需要明确环境并手动选择，不因提交自动执行。编译通过、模拟测试通过、真实宿主调用与游戏任务完成分别报告。修改用户数据、运行游戏和发布线上资产需要符合任务授权。

项目内清理入口为 `scripts/remove-build-output.ps1`，只接受规定的可再生输出；当前维护工作站自动转交本机强制删除通道，其他开发机与 CI 使用仓库内的边界检查。不要删除整个 `target/` 或把目录冲突转移到系统 Temp。

## 反馈与许可

普通问题通过仓库 Issues 提交：附版本、复现步骤、预期与实际行为，日志先脱敏；功能请求说明使用场景。疑似安全问题按 [安全说明](SECURITY.md) 私下报告。

项目代码使用 [MIT](LICENSE)。提交者应确保有权提交所含代码和资源；BetterGI、其他宿主、第三方依赖与外部资源分别遵守各自许可。
