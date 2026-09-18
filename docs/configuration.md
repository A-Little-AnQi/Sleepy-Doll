# 配置与数据存放

## 文件位置

用户配置和数据固定存放**在可执行文件旁边的 `user/`**。首次启动会把
`sleepy-doll.config.example.json` 作为模板写入 `user/config.json`。模型密钥、会话数据库、
运行事件、日志都只写在这里。

| 目录 | 内容 |
|---|---|
| `user/config.json` | 主配置 |
| `user/skills`、`user/plugins`、`user/catalog` | 用户自己的扩展与目录缓存 |
| `user/log` | 运行日志。`sleepy-doll.log` 由主程序写，超过 4 MB 轮转一次、磁盘上最多留两份；`bridge.log` 由桥在 BetterGI 进程内写 |
| `user/.sleepy-doll` | 会话数据库、WebView2 的用户数据目录，以及桥的宿主配置改动记录 |

`user/` 整棵树都由程序自己创建和维护。安装与构建都不覆盖它；卸载默认保留，只有用户在卸载界面上
显式勾选才会删除。

解析顺序：

1. 命令行第一个参数（**相对路径相对于可执行文件目录，不是工作目录**）
2. 环境变量 `SLEEPY_DOLL_CONFIG`
3. `<可执行文件目录>/user/config.json`
4. 安装目录不可写时（例如解压到受保护目录）退回 `%APPDATA%\Sleepy Doll\user\config.json`

工作目录**从不**参与解析：既不能通过参数把数据树引到当前目录，也没有工作目录回退路径
（找不到可写位置时直接报错并提示设置 `SLEEPY_DOLL_CONFIG`）。因此 `--help`、`--version`
以及任何不像路径的参数都会被明确拒绝，而不是被当成配置文件名。

注意 `cargo run` 的可执行文件在 `target/<profile>/` 下，所以开发时的 `user/` 也落在那里
（`target/` 已被忽略，`cargo clean` 会一并清掉）；debug 与 release 各自使用独立的
`user/` 目录。

要用仓库里的本地副本作为配置，用绝对路径（相对路径是相对可执行文件目录解析的）：

```bash
SLEEPY_DOLL_CONFIG="$PWD/sleepy-doll.dev.config.json" cargo run --release
```

IPC 的 `bootstrap` 不返回密钥，只返回会话与设置界面需要的部分。

## 数据库与事件

会话与工具调用的建表由 `src/runtime/store/migrations.rs` 统一拥有，`Journal` 是数据库的唯一所有者。
`runtime_events` 中的流式帧（`assistant.delta`）在启动时由 `Journal::prune` 退休，帧内容已随助手
消息落库；其余事件按序列保留最近 200000 条。

## 文件结构

| 字段 | 说明 |
|---|---|
| `version` | 配置版本，当前为 `3`。旧版本在启动时自动迁移，见下 |
| `activeModel` | 当前默认模型。没有模型时为空；有模型时必须指向 `models` 里的一项 |
| `models` | 模型列表，首次启动为空，由用户在设置里添加 |
| `agent` | 回合数上限、单轮工具调用上限、系统提示词、技能目录、按需加载技能数 |
| `bridge` | BetterGI 连接开关、地址、token、超时 |
| `plugins` | 插件目录与已启用插件的 ID 列表 |
| `storage.database` | SQLite 路径，默认 `./.sleepy-doll/sleepy-doll.db` |
| `runtime` | 执行预算、编排上限、权限模式与持续授权 |
| `hooks` | 事件钩子，目标仅允许回环地址 |

`agent.skillDirectories`、`plugins.directories`、`storage.database`、`runtime.catalogDirectory`
的相对路径都相对于配置文件所在目录解析。

迁移按文件里声明的版本执行，每次前进一步。`1` → `2` 补齐缺失的 `runtime` 段，并在迁移前留下
`config.v1.backup.json`；`2` → `3` 在 `permissionMode` 仍是旧默认值 `askEach` 时改为 `standard`
（旧默认值是写回文件的产物，不算用户选择；显式选择逐项审批的用户改回 `askEach` 即可，之后不再被覆盖）。

## BetterGI 连接

BetterGI 连接可直接在 BetterGI 页面开关，改变后立即生效。开启时会准备注入桥，
自动生成或复用 token，确认桥服务就绪后再注册 BGI 工具。保存为开启的连接在桌面启动时
尝试恢复；BetterGI 重启或连接失败后可点击「重新连接」。注入地址仅支持本机 HTTP
`127.0.0.1` 或 `localhost`。多实例时应先关闭多余实例。

桥自己的 `bridge.config.json` 位于安装目录的 `bridge\` 子目录（与桥组件同目录），
其中保存监听地址、token、方法分组和禁用列表。开关保留分组设置。
程序每次连接还会写入数据根 `userDirectory`（指向安装目录下的 `user\`），桥的日志与配置改动
记录据此落点 —— 不写这个字段时会退回 `bridge\user\`，那是桥目录还是安装目录时期的旧布局。
关闭后业务端点返回 `DISABLED`（`invoke`、`jobs`、`state`、`host`），只保留 `info`、`control` 与目录接口
供再次开启；已启动的 BetterGI 任务不会因此停止，注入的 DLL 随 BetterGI 退出卸载。
如果宿主不可达，界面会说明停用未获确认。

## 模型配置

`models` 是模型列表，首次启动为空。`activeModel` 在没有模型时为空；添加第一项时自动成为默认。例如：

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
- `${ENV:...}` 由进程环境提供。插件拉起的 MCP 与 Adapter 子进程只继承 `PATH`、`SystemRoot`、
  `WINDIR`、`TEMP`、`TMP`，读不到这些密钥（隔离契约见 [Skill 与 Plugin 格式](./extensions.md)）。
- 变量不存在时 `${ENV:X}` 会展开成空串，运行时才会以认证失败的形式暴露出来。

## BetterGI 接口说明与配置恢复

设置 → BetterGI → **接口目录**，直接读取桥的当前接口目录；页面与 Agent 使用同一份契约。
第一层包含用途、调用时机、主要参数、执行方式、副作用、可用状态。详情提供完整 JSON Schema、
示例、前置条件、返回值判定与回退边界。分组关闭或宿主空实现的接口仍可查阅，但不允许执行。
目录只代表当前宿主发现的接口，不是源码中所有公开 C# 方法。

Agent 使用 bgi.api.search 分页发现，bgi.api.describe 阅读当前版本说明，再通过 bgi.api.read
或 bgi.api.invoke 调用。修改配置采用读取旧版本 → 预览差异 → 授权提交 → 回读核验；
事务记录中的 changeId 可用于字段级回退。目标字段若已被用户再次修改，回退会拒绝覆盖。
没有安全 JSON 契约的复合对象、只读属性及尚未适配联动处理器的属性只开放读取，并说明原因。

配置事务在 `user\.sleepy-doll\config-changes` 保存 config-change-*.json 恢复记录，包含原配置
备份，使用当前 Windows 用户的专有 ACL。放在 `user\` 下是为了重装之后仍可回滚；记录含有敏感信息，
不应上传、提交到 Git 或粘贴给模型。
公开的接口结果仅显示脱敏差异。宿主命令执行前也保存配置检查点，但不能撤销外部通知、
脚本文件修改或游戏进度。

如果 BetterGI 因配置问题无法启动：完全退出所有 BetterGI 进程，在设置 → BetterGI →
**恢复记录**中选择并确认备份。独立恢复组件核对安装位置、备份摘要和当前文件版本，
先备份当前文件，再恢复所选记录；不会在宿主运行时覆盖配置。自定义配置目录不符合
自动恢复范围时会明确拒绝，需要用户人工核对。恢复整个文件会恢复该时点的全部配置。
