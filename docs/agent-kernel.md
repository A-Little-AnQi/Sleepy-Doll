# Agent Kernel

Sleepy Doll 的运行内核不理解 BetterGI、JSON、脚本组或任何其他领域格式。
领域能力由 Plugin 提供，Core 只处理契约、状态和副作用事实。

## Adapter protocol

Plugin 可在 `.sleepy-doll-plugin/plugin.json` 中声明独立进程 Adapter：

```json
{
  "schemaVersion": 1,
  "id": "example-domain",
  "name": "Example Domain",
  "version": "1.0.0",
  "adapters": [
    {
      "id": "resources",
      "version": "1",
      "command": "example-adapter.exe",
      "args": [],
      "timeoutMs": 30000,
      "maxResponseBytes": 4194304,
      "resourceRoots": ["C:\\Example\\User"]
    }
  ]
}
```

Adapter 使用一行一帧的 JSON-RPC 2.0 stdio，协商版本
`sleepy-adapter/1`。Core 会调用：

- `initialize`
- `health`
- `capabilities/list`
- `resources/discover`
- `resources/readRequest`
- `resources/inspect`
- `mutations/plan`
- `mutations/verify`
- `diagnostics/analyze`
- `strategy/validate`
- `shutdown`

Adapter 子进程与 MCP 子进程使用同一套隔离，见
[Skill 与 Plugin 格式](./extensions.md)。它公开的 Tool 必须为只读；写入 Tool 会导致整个
Plugin 启用失败。Adapter 通过 `MutationPlan` 请求副作用，不能用 Tool 返回值冒充提交完成。

资源内容以 `ResourceSnapshot + contentBase64` 发送给 Adapter。Core 使用内容寻址
Artifact Store 保存原始与 staged bytes，领域结构保持不透明。`mutations/plan`
返回的 `replaceResource` 携带 `contentBase64`；Core 验证大小并转换为 Artifact ID
后才持久化计划，Adapter 不得自行指定或写入 Artifact Store。

## Operation transaction

`MutationPlan` 固定 Provider、资源版本、内容 Hash、staged Artifact、执行契约、
验证请求与补偿计划。用户或可信资源范围授权后，Core 执行：

1. 重新读取所有资源并验证版本与 Hash。
2. 为原内容生成不可变 Snapshot。
3. 获取持久化 Lease。
4. 按 CAS 逐项提交 staged bytes。
5. 重新读取并进行字节验证。
6. 需要领域验证时，把新 Snapshot 交还原 Adapter。
7. 验证失败时仅回退本操作确实写入且此后未再次变化的资源。

如果提交后的资源又被外部程序修改，Core 进入 `needsReview`，不会用旧快照覆盖
第三方的新修改。

## Execution contract

所有 Tool 和 Workflow Step 统一声明：effect、risk、concurrency、lease scope、
idempotency、cancellation、verification、compensation、timeout、result budget 和
unattended policy；插件清单里这些字段的写法见
[Skill 与 Plugin 格式](./extensions.md)。缺失元数据按最保守方式处理。

模型只能提议操作。权限、重试、提交、验证、回退以及运行是否成功均由程序状态
决定。已验证 Workflow 固定 Tool Contract、Provider Version 和 Resource Version，
手动再次执行不调用模型。

## Context and cost

每个 Run 持久化结构化 Checkpoint。上下文通过有优先级的 Attachment 组装，优先
保留 Checkpoint、已激活 Skill 和必要偏好；大结果进入 Artifact Store。已知资源
查询、Operation 状态、回退与 Workflow 执行不需要模型。

模型降级只发生在尚未输出任何流式内容、尚未形成 Tool Call 且没有外部 Attempt
的 Provider 连接失败阶段。
