# 架构与实施边界

当前实现以 [Runtime v2](./runtime-v2.md) 为准：执行、存储与恢复语义在那里，本文只写进程模型与能力边界。

## 进程与入口

tao/wry 承担 Windows 窗口和 WebView，React 通过 IPC 调用 `AppController`。Windows 上的窗口不带
系统标题栏与边框：标题栏由 React 自绘（`web/src/components/TitleBar.tsx`），拖动、缩放边缘与
最大化时的客户区修正由 `src/window_chrome.rs` 子类化父窗口和 WebView2 子窗口处理；其它平台保留
系统标题栏。模型、Skills、Plugins、审批、事务内核和数据库都由 Rust 负责。本版没有独立第三方
HTTP Gateway 或 CLI，模型是纯文本单入口，不提供图片输入。

宿主侧只有一个接触面：注入桥在 BetterGI 进程内提供 `/bridge/v1`，接口清单与宿主约束见
[BGI 宿主契约](./bgi-host-contracts.md)。

## 决策与执行

模型负责理解请求、检索、澄清和修订计划。领域 Plugin 描述能力、资源和验证规则，Core 不理解领域
文件格式。程序校验真实能力与资源，处理授权、执行互斥、幂等提交、Job 等待和结果验证。有序计划的
普通步骤由程序连续推进，等待和重连不消耗模型决策轮数。完整模型输出验证通过前不得执行工具。

## 能力边界

当前产品装配为外部 BGI Agent，但 Agent Kernel 可替换领域 Plugin，见 [Agent Kernel](./agent-kernel.md)。
版本不匹配或缺少必要能力时拒绝调用。本地语义描述符固定 methodId、版本、资源 Hash 和观测谓词，
不能用模型输出绕过绑定、扩大授权或执行任意表达式。插件只能在启用后注册，失败时原子撤销本次注册；
模型可见的插件列表不包含凭据或启动参数。MCP 使用单一异步响应分发器，Skills 读取受启用状态和目录
边界约束；插件自述 `readOnly` 的信任假设与执行策略见 [Skill 与 Plugin 格式](./extensions.md)。

数据目录、配置解析与事件保留规则见 [配置与数据存放](./configuration.md)。
