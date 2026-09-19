# Skill 与 Plugin 格式

仓库里有两个可以直接照抄的实例：[`plugins/bgi/`](../plugins/bgi/)（随产品分发）和
[`plugins/example-weather/`](../plugins/example-weather/)（HTTP 工具示例，默认停用）。

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

`agent.skillDirectories` 下、配置文件旁的 `skills/` 是用户自己导入的技能，在界面上逐个开关。
插件目录里的技能归插件所有：它们随插件一起装载，插件停用后一并消失，不在界面上单独列出，
也没有自己的开关。产品自带的领域说明挂在 `plugins/bgi` 下。

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

`plugins.directories` 里，配置目录旁的 `plugins/` 是用户自己装的插件；产品自带的插件放在
安装目录的 `plugins/`，由配置里的 `../plugins` 引入。产品插件默认开启，关闭它需要把 ID 写进
`plugins.disabled`：仅列进 `plugins.enabled` 不会重新打开已被显式停用的插件。这类插件无法移除，
界面上没有对应的入口。`bgi` 是宿主提供方的保留 ID，它的工具由程序自己登记，目录里放的是它的
领域说明。

### 执行策略

未提供 `execution` 的工具按可能写入处理：串行、需要授权，取消结果保守判为未知。只有明确声明
`readOnly` 与 `concurrencySafe` 的工具进入最多四路的只读批次。这项声明按可信输入处理：声明只读
的工具不经过授权询问，自动放行的调用记录为 `plugin.readOnly` 事件。启用来源不明的插件，等于把
这条免授权路径交给它。

可选的 `outputSchema` 会在结果进入模型上下文前校验。`maxResultChars` 超出后完整结果保存在
运行附件中，模型只接收有界预览。

### MCP

使用 newline-delimited JSON-RPC stdio，完成 `initialize`、`notifications/initialized`、
分页 `tools/list` 与按请求 ID 分发的 `tools/call`。桥不向第三方 Server 提供采样、任意文件访问或
客户端资源能力，这类服务端发起的请求会收到 method-not-found。

插件拉起的子进程（MCP Server 与 Adapter）共用同一套隔离：清空继承的环境变量、套上带
`KILL_ON_JOB_CLOSE` 与 512 MiB 单进程内存上限的 Windows Job Object。
