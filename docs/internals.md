# 运行内核

tao/wry 负责 Windows 窗口和 WebView，React 经 IPC 调 `AppController`。Windows 上去掉系统标题栏，
界面自绘标题栏（`web/src/components/shell/TitleBar.tsx`），拖动与缩放由 `src/window_chrome.rs`
处理；其它平台保留系统栏。模型、Skills、Plugins、审批、事务和数据库都在 Rust 侧实现。程序没有
独立的 HTTP Gateway 或 CLI，模型是纯文本的单入口。

程序与宿主之间的接触面只有注入桥，即 BetterGI 进程内的 `/bridge/v1`，约束见
[BGI 宿主契约](./bgi/host-contracts.md)。

## 决策与执行

模型负责理解、检索、澄清和改计划。领域 Plugin 描述能力与验证规则，Core 不理解领域文件格式。
程序校验真实能力，并处理授权、互斥、幂等提交、Job 等待和结果验证。有序计划由程序连续推进，
模型输出完整通过校验之后，程序才执行其中的工具调用。

Supervisor（`src/runtime/`）最多并行四个对话，每个对话一个决策环。提交会持久化，并接受客户端
提供的幂等键。取消请求不调用模型。`plan.update` 校验依赖与能力绑定之后，在 Rust 侧执行剩余步骤。

只读的小批次最多四路并发。缺少元数据时按最保守的方式处理：未知副作用串行执行、需要授权，
并占用与 BGI 写入相同的 lease。缺验证证据的完成记为 `partial`，不记为 `succeeded`。终态分为
`answered`、`succeeded`、`partial`、`failed`、`cancelled` 与 `needsReview`。一次运行的默认上限为
32 次模型决策、128 次工具调用、两次计划修订和 30 分钟时限。

## 存储与恢复

SQLite 保存 run、计划修订、attempt、审批、artifact、消息、事件和 game lease。状态变更使用 CAS
修订号，并在同一事务里追加事件。BGI attempt 在发送前落盘。重启时先对账已知 Job；Job 身份丢失或
桥实例变化则进入 `needsReview`。取消结果未知时不释放 game lease。

语义能力先登记在目录里，模型输出的原始桥方法名不参与绑定：计划步骤只写能力 ID 或工具名，桥方法
由目录解析得出。资源描述包含路径与内容 Hash，调度前复核。

## Adapter 与事务

领域 Plugin 可声明独立进程 Adapter（JSON-RPC 2.0 stdio，`sleepy-adapter/1`）。Adapter 的 Tool 在
装载时限定为只读，写入通过 `MutationPlan` 提交。Artifact、授权、lease、提交、验证、补偿和恢复
都由 Core 负责。提交后资源若又被外部改掉，运行进入 `needsReview`，旧快照不覆盖第三方的修改。
清单字段见 [Skill 与 Plugin 格式](./extensions.md)。

## 桌面 IPC

- `run.submit` / `run.get` / `run.cancel` / `run.input`
- `approval.respond`
- `events.read`（按 conversation、序号长轮询）
- `plugin.install` / `plugin.remove` / `extensions.reload`

已验证的确定性计划可抽成快捷任务，再次执行不调用模型。
