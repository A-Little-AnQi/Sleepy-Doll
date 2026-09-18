# 运行内核

tao/wry 管 Windows 窗口和 WebView，React 经 IPC 调 `AppController`。Windows 去掉系统标题栏：
界面自绘标题栏（`web/src/components/shell/TitleBar.tsx`），拖动与缩放由 `src/window_chrome.rs`
处理；其它平台保留系统栏。模型、Skills、Plugins、审批、事务和数据库都在 Rust。没有独立 HTTP
Gateway 或 CLI，模型是纯文本单入口。

宿主接触面只有注入桥：BetterGI 进程内的 `/bridge/v1`，约束见 [BGI 宿主契约](./bgi/host-contracts.md)。

## 决策与执行

模型负责理解、检索、澄清和改计划。领域 Plugin 描述能力与验证规则，Core 不理解领域文件格式。
程序校验真实能力，处理授权、互斥、幂等提交、Job 等待和结果验证。有序计划由程序连续推进；
完整模型输出通过验证前不得执行工具。

Supervisor（`src/runtime/`）最多并行四个对话，每对话一个决策环。提交持久化，接受客户端
幂等键。取消不调用模型。`plan.update` 校验依赖与能力绑定后在 Rust 里执行剩余步骤。

只读小批次最多四路并发。缺元数据按最保守处理：未知副作用串行、要授权、占用与 BGI 写入相同的
lease。没有权威验证的完成不能当成游戏成功。终态区分 `answered`、`succeeded`、`partial`、
`failed`、`cancelled`、`needsReview`。默认上限 32 次模型决策、128 次工具调用、两次计划修订、
30 分钟。

## 存储与恢复

SQLite 存 run、计划修订、attempt、审批、artifact、消息、事件和 game lease。状态变更用 CAS
修订号，并在同一事务里追加事件。BGI attempt 在发送前落盘。重启先对账已知 Job；Job 身份丢失或
桥实例变化则进入 `needsReview`。未知取消不释放 game lease。

语义能力必须先登记。模型输出的原始桥方法名不能绕过绑定。资源描述含路径与内容 Hash，调度前复核。

## Adapter 与事务

领域 Plugin 可声明独立进程 Adapter（JSON-RPC 2.0 stdio，`sleepy-adapter/1`）。Adapter 的 Tool
必须只读；写入走 `MutationPlan`。Core 拥有 Artifact、授权、lease、提交、验证、补偿和恢复。
提交后若资源又被外部改掉，进入 `needsReview`，不用旧快照覆盖第三方的新修改。清单字段见
[Skill 与 Plugin 格式](./extensions.md)。

## 桌面 IPC

- `run.submit` / `run.get` / `run.cancel` / `run.input`
- `approval.respond`
- `events.read`（按 conversation、序号长轮询）
- `plugin.install` / `plugin.remove` / `extensions.reload`

已验证的确定性计划可抽成快捷任务，再执行不调模型。
