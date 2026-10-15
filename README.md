# Sleepy Doll

用于游戏脚本工具的本地桌面助手，目前已适配 BetterGI（BGI）。用户用自然语言提出目标，助手负责检索资料、配置任务、调用工具和核对结果；游戏操作由接入的脚本工具执行。

支持五种模型协议、技能与插件、操作审批、执行记录、取消与恢复，以及无需再次调用模型的快捷任务。通用 Agent 运行时与 BGI 适配层分开。

## 构建与运行

Windows 构建需要 Rust stable、Node.js 22.13+、.NET 8 SDK 和 Visual Studio 2022 C++ Build Tools。桌面运行需要 WebView2 Runtime。

```powershell
npm install
npm run dist
```

完整构建生成 `dist/Sleepy-Doll/` 和 `dist/Sleepy-Doll-<版本>-setup.exe`。分发使用安装包；本机组装目录可能含用户数据，不能整目录打包。

安装后启动 Sleepy Doll，在设置中添加模型，并在 BetterGI 页面连接工具。程序启动时请求管理员权限。默认安装目录为固定 D 盘上的 `D:\Sleepy Doll`；没有固定 D 盘时使用 `%LOCALAPPDATA%\Programs\Sleepy Doll`。

配置、模型密钥、会话和日志位于安装目录的 `user/`。覆盖构建与安装保留该目录，卸载默认保留数据。

## 工程结构

| 路径 | 内容 |
| --- | --- |
| `src/` | Rust 桌面壳、Agent 运行时与安装引擎 |
| `web/` | React 主程序、安装器与托盘界面 |
| `plugins/` | 领域技能与工具接入清单 |
| `bgi-bridge/` | BGI 进程内桥、源码索引与桥专项验证 |
| `scripts/` | 构建、发布、更新验证与缓存清理 |
| `tests/` | 界面、桌面与通用运行时验证 |
| `cloudflare/` | 分发服务 |
| `video/` | 宣传片源码与制作入口 |
| `docs/` | 维护文档 |
| `target/` | 构建缓存、生成报告与验证数据，不进 Git |
| `dist/` | 桌面程序和安装包，不进 Git |

宣传片使用现有的 `make-video.bat`，成片保存在 `video/output/`。

## 文档

- [开发与验证](docs/development.md)
- [配置与数据](docs/configuration.md)
- [运行内核与调用链路](docs/internals.md)
- [技能与插件](docs/extensions.md)
- [快捷任务](docs/quick-tasks.md)
- [发布与更新](docs/distribution.md)
- [BGI 桥](bgi-bridge/README.md)、[宿主契约](docs/bgi/host-contracts.md)、[配置恢复](docs/bgi/config-recovery.md)

## 许可

[MIT](LICENSE)。BetterGI 是独立项目，其许可与资源使用条件分别遵守原项目要求。
