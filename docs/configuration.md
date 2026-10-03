# 配置与数据存放

Sleepy Doll 的全部用户状态——配置、模型密钥、会话数据库、日志——都集中在可执行文件旁边的 `user/` 目录。这份文档说明配置文件的位置与解析顺序、每个字段的含义、模型与密钥的配置方式，以及 BetterGI 连接相关落盘内容。

**适用读者**：准备部署或迁移安装目录的用户、需要写自动化脚本的维护者。

## 快速要点

- 数据只在 `user/` 树内，安装、覆盖构建都不碰它，卸载默认保留。
- 配置解析顺序：命令行参数 → `SLEEPY_DOLL_CONFIG` 环境变量 → `<可执行文件目录>/user/config.json` → `%APPDATA%` 回退。
- 相对路径相对**可执行文件目录**解析，与工作目录无关。
- 密钥支持 `${ENV:NAME}` 引用；界面填写的密钥会明文落盘，长期使用建议只用环境变量引用。
- 配置版本当前为 `4`，旧版本启动时自动迁移。

## 目录

- [user 目录布局](#user-目录布局)
- [解析顺序](#解析顺序)
- [配置结构](#配置结构)
- [版本迁移](#版本迁移)
- [模型配置](#模型配置)
- [密钥](#密钥)
- [BetterGI 连接](#bettergi-连接)
- [接口目录与配置恢复](#接口目录与配置恢复)
- [数据库与事件](#数据库与事件)

## user 目录布局

| 路径 | 内容 |
| --- | --- |
| `user/config.json` | 主配置，首次启动从 `sleepy-doll.config.example.json` 模板生成 |
| `user/skills`、`user/plugins`、`user/catalog` | 用户自己的技能、插件与目录缓存 |
| `user/log` | 运行日志：`sleepy-doll.log` 超过 4 MB 轮转、磁盘最多两份；`bridge.log` 由桥在 BetterGI 进程内写 |
| `user/.sleepy-doll` | 会话数据库、WebView2 用户数据目录、桥的宿主配置改动记录 |

`user/` 整棵树由程序自己创建和维护：

- 安装与构建都不覆盖它。
- 卸载默认保留；只有用户在卸载界面显式勾选才删除，解析顺序第 4 条的回退位置（`%APPDATA%\Sleepy Doll\`）按同一勾选处理。

## 解析顺序

1. 命令行第一个参数——相对路径相对**可执行文件目录**解析，不是工作目录。
2. 环境变量 `SLEEPY_DOLL_CONFIG`。
3. `<可执行文件目录>/user/config.json`。
4. 安装目录不可写时（例如解压到受保护目录）退回 `%APPDATA%\Sleepy Doll\user\config.json`。

工作目录不参与解析：不能通过参数把数据树引到当前目录，也没有工作目录回退路径——找不到可写位置时直接报错并提示设置 `SLEEPY_DOLL_CONFIG`。`--help`、`--version` 以及任何不像路径的参数都会被明确拒绝，不会当成配置文件名。

> **开发注意**：`cargo run` 的可执行文件在 `target/<profile>/` 下，开发时的 `user/` 也落在那里（`target/` 已被忽略，`cargo clean` 会一并清掉）；debug 与 release 各自使用独立的 `user/` 目录。

用仓库里的本地副本作为配置时用绝对路径：

```bash
SLEEPY_DOLL_CONFIG="$PWD/sleepy-doll.dev.config.json" cargo run --release
```

## 配置结构

| 字段 | 说明 |
| --- | --- |
| `version` | 配置版本，当前为 `4`，见[版本迁移](#版本迁移) |
| `activeModel` | 当前默认模型。没有模型时为空；有模型时必须指向 `models` 里的一项 |
| `models` | 模型列表，首次启动为空，由用户在设置里添加 |
| `agent` | 回合数上限、单轮工具调用上限、用户自定义指令、用户技能目录、按需加载技能数 |
| `bridge` | BetterGI 连接开关、地址、token、超时 |
| `plugins` | 插件目录、已启用插件的 ID 列表，以及显式停用的列表（随产品提供的插件默认开启） |
| `storage.database` | SQLite 路径，默认 `./.sleepy-doll/sleepy-doll.db` |
| `runtime` | 执行预算、编排上限、权限模式与持续授权 |
| `hooks` | 事件钩子，目标仅允许回环地址 |
| `tray.enabled` | 是否显示托盘图标，默认 `true`。只在桌面壳里生效；隐藏后点关闭按钮直接退出程序 |

相对路径字段（`agent.skillDirectories`、`plugins.directories`、`storage.database`、`runtime.catalogDirectory`）都相对**配置文件所在目录**解析。

`agent.systemPrompt` 是用户自己的常驻指令，随每一次运行注入，默认留空。产品自带的底座规则（语气、证据纪律、内部实现的边界）编译在程序里，不写在这个字段；领域知识随插件分发，也不在这里。在设置 → 通用 → 配置文件里可以直接编辑。

## 版本迁移

迁移按文件里声明的版本执行，每次前进一步：

| 迁移 | 内容 |
| --- | --- |
| `1` → `2` | 补齐缺失的 `runtime` 段；迁移前留下 `config.v1.backup.json` |
| `2` → `3` | `permissionMode` 仍是旧默认值 `askEach` 时改为 `standard`。旧默认值是写回文件的产物，不算用户选择；显式选择逐项审批的用户改回 `askEach` 即可，之后不再被覆盖 |
| `3` → `4` | 去掉产品技能目录（`../skills`），给插件目录补上产品根 `../plugins`——领域说明随 `plugins/bgi` 分发，不再单独列技能目录。用户自己的 `./skills` 与 `./plugins` 不受影响 |

## 模型配置

`models` 是模型列表；`activeModel` 在没有模型时为空，添加第一项时自动成为默认。

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

`protocol` 取以下之一：

| 值 | 协议 |
| --- | --- |
| `openai-responses` | OpenAI Responses |
| `openai-chat` | OpenAI-compatible Chat |
| `anthropic-messages` | Anthropic Messages |
| `gemini` | Google Gemini |
| `ollama-chat` | Ollama |

OpenAI-compatible、代理网关和私有部署通过 `baseUrl` 与 `headers` 配置。

`options.timeoutMs` 是**空闲超时**：连接建立、以及流式响应中相邻数据块之间的最长间隔。整轮总时长由任务时限（`runtime.durationSec`）约束，长回复不会被请求级超时截断。

`options.contextWindow` 是模型的上下文容量，不是历轮累计用量的限制。未填写时以 256000 token 为起始值。添加模型时，获取模型列表会采用服务报告的容量；没有容量信息时，已知模型沿用其预设，未知模型使用 256k 起始值。已保存和手动填写的容量不会因刷新列表而改变，可在高级选项中点击“使用建议值”。建议值不能保证代理服务或自部署模型支持同样的窗口，应以实际服务限制为准。

上下文压缩开始和完成时在对话过程里显示记录，最终答案出现后随过程收进折叠层。模型旁的用量圆环只展示占用、容量与缓存命中，不显示压缩说明。历史对话加载时先读取压缩记录的完成状态，避免重放旧事件时先展开再折叠。

删除对话采用数据库真删除：先停止对应的 Agent 运行，再清理消息、工具调用、运行事件、检查点、授权与执行记录，并清空页面会话缓存。无法结束的执行仍占用锁时，删除事务保持原数据完整。独立保存的快捷任务和已生成的输出文件保留；删除对话仍需二次确认。

## 密钥

密钥字段支持完整的 `${ENV:VARIABLE_NAME}` 引用，启动时从进程环境展开。模型配置页编辑已有配置时会回显已保存的密钥（可切换显示或隐藏）；bootstrap 响应不返回自定义请求头。

- 界面里填写的密钥会**明文**写入 `user/config.json`；长期使用建议只保留 `${ENV:...}` 引用。
- 配置文件本身没有做权限加固（不会自动设成 `0600`）。便携目录或共享目录下的 `user/config.json` 与 `user/.sleepy-doll/*.db` 沿用所在目录的权限，请自行确认该目录不对其他账户开放。
- `${ENV:...}` 由进程环境提供。插件拉起的 MCP 与 Adapter 子进程只继承 `PATH`、`SystemRoot`、`WINDIR`、`TEMP`、`TMP`，读不到这些密钥；隔离契约见 [Skill 与 Plugin 格式](./extensions.md)。
- 变量不存在时 `${ENV:X}` 展开成空串，运行时以认证失败的形式暴露。

## BetterGI 连接

BetterGI 连接在界面「BetterGI」页面开关，改变后立即生效。开启时准备注入桥、自动生成或复用 token，确认桥服务就绪后再注册 BGI 工具。保存为开启的连接在桌面启动时尝试恢复；BetterGI 重启或连接失败后点击「重新连接」。

- **地址**：仅支持本机 HTTP `127.0.0.1` 或 `localhost`。默认端口 26101；被其他程序占用时自动改用其后第一个空位（最多顺延 64 个），并写回 `bridge.baseUrl` 与桥的 `listen`。占用方是本产品的桥时沿用原端口。多实例应先关闭多余实例。
- **桥配置**：`bridge\bridge.config.json` 与桥组件同目录（安装目录的 `bridge\` 子目录），保存监听地址、token、方法分组和禁用列表；开关保留分组设置。程序每次连接写入数据根 `userDirectory`（指向安装目录下的 `user\`），桥的日志与配置改动记录据此落点，不写该字段时退回 `bridge\user\`。
- **关闭后**：业务端点（`invoke`、`jobs`、`state`、`host`）返回 `DISABLED`，只保留 `info`、`control` 与目录接口供再次开启；已启动的 BetterGI 任务不会因此停止，注入的 DLL 随 BetterGI 退出卸载。宿主不可达时，界面说明停用未获确认。

## 接口目录与配置恢复

设置 → BetterGI → **接口目录**直接读取桥的当前接口目录；页面与 Agent 使用同一份契约。

- 第一层包含用途、调用时机、主要参数、执行方式、副作用、可用状态；详情提供完整 JSON Schema、示例、前置条件、返回值判定与回退边界。
- 分组关闭或宿主空实现的接口仍可查阅，但无法调用。目录只代表当前宿主发现的接口，不是源码中所有公开 C# 方法。

Agent 的工作方式：`bgi.api.search` 分页发现 → `bgi.api.describe` 阅读当前版本说明 → `bgi.api.read` / `bgi.api.invoke` 调用。修改配置采用「读取旧版本 → 预览差异 → 授权提交 → 回读核验」；事务记录中的 changeId 可用于字段级回退，目标字段若已被用户再次修改，回退拒绝覆盖。没有安全 JSON 契约的复合对象、只读属性及尚未适配联动处理器的属性只开放读取，并说明原因。

**恢复记录**：配置事务在 `user\.sleepy-doll\config-changes` 保存 `config-change-*.json`，包含原配置备份，使用当前 Windows 用户的专有 ACL；记录在 `user\` 下，重装之后仍可回滚。

> 恢复记录含有敏感信息，不应上传、提交到 Git 或粘贴给模型。公开的接口结果仅显示脱敏差异。宿主命令执行前也保存配置检查点，但不能撤销外部通知、脚本文件修改或游戏进度。

BetterGI 因配置问题无法启动时：完全退出所有 BetterGI 进程，在设置 → BetterGI → **恢复记录**中选择并确认备份。独立恢复组件核对安装位置、备份摘要和当前文件版本，先备份当前文件再恢复所选记录；不在宿主运行时覆盖配置。自定义配置目录不符合自动恢复范围时明确拒绝，需人工核对。恢复整个文件会恢复该时点的全部配置。

## 数据库与事件

会话与工具调用的建表由 `src/runtime/store/migrations.rs` 统一拥有，`Journal` 是数据库的唯一所有者。`runtime_events` 中的流式帧（`assistant.delta`）在启动时由 `Journal::prune` 退休——帧内容已随助手消息落库；其余事件按序列保留最近 200000 条。

## 相关文档

| 文档 | 内容 |
| --- | --- |
| [Skill 与 Plugin 格式](./extensions.md) | 技能、插件、MCP 的文件格式与执行策略 |
| [运行内核](./internals.md) | 进程模型、决策执行、存储与恢复 |
| [BGI 宿主契约](./bgi/host-contracts.md) | 宿主约束、Job 与错误协议、并发与取消 |
