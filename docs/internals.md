# 运行内核

Sleepy Doll 的进程模型与执行语义：窗口与 WebView 的分工、模型驱动的决策与执行环、运行终态与预算、SQLite 存储与重启恢复、Adapter 事务，以及桌面 IPC 面。

**适用读者**：需要理解程序如何执行任务的开发者——排查运行行为、写插件或扩展内核前先读这份。

## 快速要点

- React 界面经 IPC 调 Rust 侧 `AppController`；模型、技能、插件、审批、事务、数据库全在 Rust 侧。
- 与宿主（BetterGI）的唯一接触面是注入桥 `/bridge/v1`。
- 模型负责理解与计划，程序负责校验、授权、互斥、幂等提交与结果验证。
- 一次运行默认上限：32 次模型决策、128 次工具调用、两次计划修订、30 分钟时限。
- 缺验证证据的完成记为 `partial`，不记为 `succeeded`。

## 目录

- [进程模型](#进程模型)
- [决策与执行](#决策与执行)
- [运行终态与预算](#运行终态与预算)
- [存储与恢复](#存储与恢复)
- [Adapter 与事务](#adapter-与事务)
- [桌面 IPC](#桌面-ipc)

## 进程模型

- tao/wry 负责 Windows 窗口和 WebView；React 经 IPC 调 `AppController`。
- Windows 上去掉系统标题栏，界面自绘标题栏（`web/src/components/shell/TitleBar.tsx`）；拖动与缩放由 `src/window_chrome.rs` 处理。其它平台保留系统栏。
- 程序没有独立的 HTTP Gateway 或 CLI，模型是纯文本的单入口。

程序与宿主之间的接触面只有注入桥，即 BetterGI 进程内的 `/bridge/v1`，约束见 [BGI 宿主契约](./bgi/host-contracts.md)。

## 决策与执行

分工边界：

| 角色 | 职责 |
| --- | --- |
| 模型 | 理解、检索、澄清、改计划 |
| 领域 Plugin | 描述能力与验证规则（Core 不理解领域文件格式） |
| Core（Rust） | 校验真实能力、授权、互斥、幂等提交、Job 等待、结果验证 |

有序计划由程序连续推进：模型输出完整通过校验之后，程序才执行其中的工具调用。`plan.update` 校验依赖与能力绑定之后，剩余步骤在 Rust 侧执行。

Supervisor（`src/runtime/`）最多并行四个对话，每个对话一个决策环。提交持久化并接受客户端提供的幂等键；取消请求不调用模型。只读的小批次最多四路并发。

缺少元数据时按最保守方式处理：未知副作用串行执行、需要授权，并占用与 BGI 写入相同的 lease。

## 运行终态与预算

终态取以下之一：

| 终态 | 含义 |
| --- | --- |
| `answered` | 纯问答完成，无工具执行 |
| `succeeded` | 完成且有验证证据 |
| `partial` | 完成但缺验证证据 |
| `failed` | 失败 |
| `cancelled` | 已取消 |
| `needsReview` | 需要人工复核（如 Job 身份丢失、资源被外部修改） |

一次运行的默认上限：32 次模型决策、128 次工具调用、两次计划修订、30 分钟时限（`runtime.durationSec`）。

## 存储与恢复

SQLite 保存 run、计划修订、attempt、审批、artifact、消息、事件和 game lease。

- 状态变更使用 CAS 修订号，并在同一事务里追加事件。
- BGI attempt 在发送前落盘。
- 重启时先对账已知 Job；Job 身份丢失或桥实例变化则进入 `needsReview`。
- 取消结果未知时不释放 game lease。

语义能力先登记在目录里，模型输出的原始桥方法名不参与绑定：计划步骤只写能力 ID 或工具名，桥方法由目录解析得出。资源描述包含路径与内容 Hash，调度前复核。

## Adapter 与事务

领域 Plugin 可声明独立进程 Adapter（JSON-RPC 2.0 stdio，`sleepy-adapter/1`）：

- Adapter 的 Tool 在装载时限定为只读，写入通过 `MutationPlan` 提交。
- Artifact、授权、lease、提交、验证、补偿和恢复都由 Core 负责。
- 提交后资源若又被外部改掉，运行进入 `needsReview`，旧快照不覆盖第三方的修改。

清单字段见 [Skill 与 Plugin 格式](./extensions.md)。

## 桌面 IPC

| 方法 | 用途 |
| --- | --- |
| `run.submit` / `run.get` / `run.cancel` / `run.input` | 运行提交、查询、取消、补充输入 |
| `approval.respond` | 审批响应 |
| `events.read` | 按 conversation、序号长轮询事件 |
| `plugin.install` / `plugin.remove` / `extensions.reload` | 插件与扩展管理 |

已验证的确定性计划可抽成**快捷任务**：再次执行不调用模型。

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [BGI 宿主契约](./bgi/host-contracts.md) | Job 生命周期、错误协议、并发与取消 |
| [Skill 与 Plugin 格式](./extensions.md) | 插件清单与 Adapter 声明 |
| [配置与数据存放](./configuration.md) | 运行预算与数据库路径配置 |
