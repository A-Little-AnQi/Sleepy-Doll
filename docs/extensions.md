# Skill 与 Plugin 格式

仓库里有两个可以直接照抄的实例：[`skills/bgi-operator/SKILL.md`](../skills/bgi-operator/SKILL.md)
和 [`plugins/example-weather/`](../plugins/example-weather/)。

## Skill

一个目录里放 `SKILL.md`，用 frontmatter 描述，正文是给模型的操作说明：

```markdown
---
name: my-skill
description: 何时应该使用这个 Skill
tags: bgi, route
---

# 完整操作说明
...
```

Skill 文件上限为 128 KiB，默认最多自动装入 4 个。目录中存在不代表全部内容会进入每次模型
上下文——匹配到的正文才会快照进本次运行。行尾符（LF 或 CRLF）不影响解析。

## Plugin

插件目录必须包含 `.sleepy-doll-plugin/plugin.json`：

```json
{
  "schemaVersion": 1,
  "id": "my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "skills": ["./skills"],
  "httpTools": [
    {
      "name": "lookup",
      "description": "查询本地数据",
      "inputSchema": { "type": "object" },
      "outputSchema": { "type": "object" },
      "method": "GET",
      "url": "http://127.0.0.1:9000/lookup",
      "execution": {
        "effect": "readOnly",
        "concurrencySafe": true,
        "maxResultChars": 12000,
        "deferred": true,
        "alwaysLoad": false,
        "searchHint": "查询本地索引"
      }
    }
  ],
  "mcpServers": [
    {
      "id": "local",
      "command": "my-mcp-server",
      "args": [],
      "toolExecution": {
        "lookup": {
          "effect": "readOnly",
          "concurrencySafe": true
        }
      }
    }
  ]
}
```

### 安装与启用是两件事

放入目录只是“已安装”，还需把插件 ID 加入 `plugins.enabled` 才会注册工具或启动子进程。
更新插件前必须先停用它；移除和替换会把旧文件留在插件目录的 `.retired/` 下。

### 执行策略

未提供 `execution` 的工具按可能写入处理：串行、需要授权，且取消结果保守判为未知。只有明确
声明 `readOnly` 与 `concurrencySafe` 的工具才能进入最多四路的只读批次——**这条声明被当作
可信输入**，它会跳过授权询问，自动放行的调用记录为 `plugin.readOnly` 事件。不要启用来源
不明的插件。

可选的 `outputSchema` 会在结果进入模型上下文前校验。`maxResultChars` 超出后完整结果保存在
运行附件中，模型只接收有界预览。

### MCP

使用 newline-delimited JSON-RPC stdio，完成 `initialize`、`notifications/initialized`、
分页 `tools/list` 与按请求 ID 分发的 `tools/call`。不向第三方 Server 提供采样、任意文件访问或
客户端资源能力，这类服务端发起的请求会收到 method-not-found。

插件拉起的子进程（MCP Server 与 Adapter）共用同一套隔离：清空继承的环境变量、套上带
`KILL_ON_JOB_CLOSE` 与 512 MiB 单进程内存上限的 Windows Job Object。
