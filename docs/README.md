# 文档导航

文档按用途维护。当前实现以源码和工具契约为准；架构教程中的建议不表示产品已经实现。运行日志、截图和逐轮任务记录不进入文档目录。

| 目标 | 文档 |
| --- | --- |
| 安装、构建和项目介绍 | [项目首页](../README.md) |
| 开发环境、验证命令和生成文件 | [开发与验证](development.md) |
| 模型、联网、凭据和数据迁移 | [配置与数据](configuration.md) |
| 当前 Agent、上下文、工具和记忆链路 | [运行内核](internals.md) |
| 技能、插件和 MCP 格式 | [扩展开发](extensions.md) |
| 保存和执行快捷任务 | [快捷任务](quick-tasks.md) |
| 构建安装包、签名和正式发布 | [发布与更新](distribution.md) |
| 理解 Agent 与 Harness 的设计取舍 | [Agent 开发教程](guides/agent-development.md) |
| BGI 当前工具与技能的披露过程 | [渐进式披露](bgi/progressive-disclosure.md) |
| BGI 桥接入与组件布局 | [桥说明](../bgi-bridge/README.md) |
| 宿主调用、互斥与验证边界 | [宿主契约](bgi/host-contracts.md) |
| 用户设置的备份与恢复 | [配置恢复](bgi/config-recovery.md) |
| 提交代码、反馈问题和安全报告 | [贡献指南](../CONTRIBUTING.md)、[安全说明](../SECURITY.md) |

`bgi/` 保存领域实现文档，`guides/` 保存概念教程；顶层文档描述可维护的产品与工程契约。生成的宿主声明和功能索引分别留在桥与插件模块，不复制到这里。
