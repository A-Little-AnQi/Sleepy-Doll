# 配置与数据存放

## 文件位置

用户配置和数据固定存放**在可执行文件旁边的 `user/`**。首次启动会把
`sleepy-doll.config.example.json` 作为模板写入 `user/config.json`，并创建
`user/skills`、`user/plugins`、`user/catalog`、`user/.sleepy-doll`。模型密钥、会话数据库、
运行事件都只写在这里。

解析顺序：

1. 命令行第一个参数（**相对路径相对于可执行文件目录，不是工作目录**）
2. 环境变量 `SLEEPY_DOLL_CONFIG`
3. `<可执行文件目录>/user/config.json`
4. 安装目录不可写时（例如系统级安装）退回 `%APPDATA%\Sleepy Doll\user\config.json`

工作目录**从不**参与解析：既不能通过参数把数据树引到当前目录，也没有工作目录回退路径
（找不到可写位置时直接报错并提示设置 `SLEEPY_DOLL_CONFIG`）。因此 `--help`、`--version`
以及任何不像路径的参数都会被明确拒绝，而不是被当成配置文件名。

注意 `cargo run` 的可执行文件在 `target/<profile>/` 下，所以开发时的 `user/` 也落在那里
（`target/` 已被忽略，`cargo clean` 会一并清掉）；debug、release 与 mock 各自使用独立的
`user/` 目录。

要用仓库里的本地副本作为配置，用绝对路径（相对路径是相对可执行文件目录解析的）：

```bash
SLEEPY_DOLL_CONFIG="$PWD/sleepy-doll.dev.config.json" cargo run --release
```

## 文件结构

| 字段 | 说明 |
|---|---|
| `version` | 配置版本，当前为 `2`。`1` 会在启动时自动迁移并留下 `config.v1.backup.json` |
| `activeModel` | 唯一生效的模型，必须且只能指向 `models` 里的一项 |
| `models` | 可选模型列表，见下节 |
| `agent` | 回合数上限、单轮工具调用上限、系统提示词、技能目录、按需加载技能数 |
| `bridge` | BetterGI 连接开关、地址、token、超时 |
| `plugins` | 插件目录与已启用插件的 ID 列表 |
| `storage.database` | SQLite 路径，默认 `./.sleepy-doll/sleepy-doll.db` |
| `runtime` | 执行预算、编排上限、权限模式与持续授权 |
| `hooks` | 事件钩子，目标仅允许回环地址 |

`agent.skillDirectories`、`plugins.directories`、`storage.database`、`runtime.catalogDirectory`
的相对路径都相对于配置文件所在目录解析。

## 模型配置

`models` 是可选模型列表，`activeModel` 是唯一入口。例如：

```json
{
  "activeModel": "primary",
  "models": [
    {
      "id": "primary",
      "name": "Main model",
      "protocol": "openai-responses",
      "model": "your-model-id",
      "baseUrl": "https://api.openai.com/v1",
      "apiKey": "${ENV:OPENAI_API_KEY}",
      "headers": {},
      "options": { "maxOutputTokens": 8192 }
    }
  ]
}
```

`protocol` 取 `openai-responses`、`openai-chat`、`anthropic-messages`、`gemini` 或
`ollama-chat`。OpenAI-compatible、代理网关和私有部署可通过 `baseUrl` 与 `headers` 配置。

`options.timeoutMs` 是空闲超时：连接建立、以及流式响应中相邻数据块之间的最长间隔。整轮总时长
由任务时限（`runtime.durationSec`）约束，因此长回复不会被请求级超时截断。

## 密钥

密钥字段支持完整的 `${ENV:VARIABLE_NAME}` 引用，启动时从进程环境展开。IPC 的 bootstrap
响应不会返回密钥或自定义请求头，界面上也不会把已保存的密钥显示回来。

需要注意：

- 界面里填写的密钥会**明文**写入 `user/config.json`。长期使用建议只保留 `${ENV:...}` 引用。
- 配置文件本身没有做权限加固（不会自动设成 `0600`）。便携目录或共享目录下的
  `user/config.json` 与 `user/.sleepy-doll/*.db` 会沿用所在目录的权限，请自行确认该目录
  不对其他账户开放。
- `${ENV:...}` 由进程环境提供。插件拉起的 MCP 子进程只继承 `PATH`、`SystemRoot`、
  `WINDIR`、`TEMP`、`TMP`，读不到这些密钥。
- 变量不存在时 `${ENV:X}` 会展开成空串，运行时才会以认证失败的形式暴露出来。
