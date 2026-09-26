# Sleepy Doll

用于使用其他游戏脚本工具的通用本地桌面 Agent，目前已适配 BetterGI（BGI）。
Rust 实现桌面壳与通用 Agent 运行时，React 实现界面；`bgi-bridge/` 是当前 BGI 适配使用的
.NET / C++ 进程内桥。

## 产品定位与架构边界

用户描述目标，Agent 负责理解、检索、规划、配置、工具调用与结果核对；实际游戏操作由接入的
游戏脚本工具执行。BetterGI 是当前适配对象，产品定位和通用内核不绑定 BetterGI。

- **通用运行时**：模型协议、对话与流式展示、技能和工具装载、规划、权限、并发、持久化、
  取消、恢复和快捷任务。
- **领域适配**：游戏工具的术语、资源目录、脚本参数、宿主接口、操作流程、前置条件与验证规则。
  这些内容放在对应插件、Adapter 或宿主接入模块中。
- **当前 BGI 接入**：`plugins/bgi/` 提供领域技能，`src/bridge/` 提供 BGI 客户端和工具，
  `bgi-bridge/` 在 BetterGI 进程内提供接口。

新增游戏脚本工具时，复用通用运行时并增加对应适配。修复 BGI 问题时，优先修改 BGI 插件和
接入模块；不得把材料名称、配置组格式、BGI 检索路由或界面行为写成通用 Agent 的规则。
只有多种工具共同需要的机制才进入通用层。目前仅完成 BGI 适配，不将扩展机制描述成其他工具已经可用。

## 构建与验证

```bash
npm ci
npm run dist               # 唯一的发布构建入口（执行 build-desktop.cmd），产物在 dist\Sleepy-Doll\
```

| 命令 | 内容 |
|---|---|
| `npm run build` | 前端类型检查 + 生产构建（`npm run typecheck` 只做类型检查） |
| `cargo check` 与 `cargo check --no-default-features` | 两种特性组合都要过 |
| `cargo clippy --all-targets --no-default-features -- -D warnings` | **CI 不执行 clippy，本地必须执行** |
| `cargo fmt --all -- --check` | 格式 |

安装程序是自绘的：`src/bin/sleepy-doll-setup.rs` 用与应用同一套 tao + wry + React 外壳起窗口，
界面在 `web/src/setup/`，外观与主程序一致。`build-desktop.cmd` 会先把交付目录打成压缩载荷
（`installer/pack-payload.ps1`），再编译这个 bin，产物在 `dist\Sleepy-Doll-<版本>-setup.exe`。
推 `v*` tag 时 `.github/workflows/release.yml` 自动执行整串。

## 约定

- **`.cmd` 脚本必须保持纯 ASCII。** cmd.exe 按 OEM 代码页读取，非 ASCII 字符会打乱命令解析
  （`build-desktop.cmd` 头部写着这条，违反过一次：中文注释让 `(` `)` 解析崩掉）。
- **构建是覆盖写入，不先清空目录。** 保留用户数据。运行中的主程序可能锁定 EXE；
  BGI 注入使用影拷贝，交付目录中的桥文件与已加载副本分开。替换失败时说明具体文件和占用情况。
- 产物只有两个落点，都不进版本库：中间产物统一在 `target/`（`target/bridge`、`target/dotnet`、
  `target/ui`、`target/release`），交付物为 `dist/Sleepy-Doll/` 与 `dist/Sleepy-Doll-<版本>-setup.exe`。
- **注释只写这段代码做什么，一行说完。** 代码本身看不出来的外部事实要留：Windows 与 BetterGI 的
  行为、模型协议的要求、文件格式、单位、取值范围、必须维持的不变量、危险操作的安全提示。本仓库
  自己的设计论证一律删掉 —— 「否则」「不然」「免得」「以前」，以及替某个选择辩解的整段话。
  界面文案同理：说清一件事，不解释为什么这样设计。通用层遵循本项目对应模块的风格；
  适配层核对对应工具的一手源码或接口契约。
- 文档、提交信息、注释与界面文案统一用标准技术文体与简体中文：动词用「执行」不用「跑」、
  产品名与协议术语不译（Build Tools、token）、代码注释与文档里不出现自称「我们」。

## 协作方式

本节约束仓库开发协作，不作为产品 Agent 的系统提示词。产品运行时的领域指令由对应插件提供。

**说话**

- 结论先行。先给结果和判断，再给依据。不写「我先看看」「搞清楚了」「找到了」这类过程叙述，
  不用比喻和夸张（「一地散文件」「别踩的坑」），不用语气词。
- 报告要如实：失败就写失败并附输出，跳过了就说明跳过了，不要把「应该能过」当结论。
- **不要把能自己查到的事丢回给用户。** 进程是否在运行、某个目录里有什么、某个值是多少 ——
  先查，确实查不到再问。
- **不要用罗列选择题代替继续工作。** 只有两种情况值得问：目标不存在；同一句话有两种代价不可逆的
  理解。问就一次问完。写操作本身会走审批闸门，不需要在对话里再确认一遍。
- 长任务不中途汇报进展，做完一次性说清：改了什么、验证了什么、还剩什么。

**写代码**

- **注释只写代码做什么，一行说完；改动来历属于提交信息。** 外部约束（系统与宿主的行为、协议、
  文件格式、取值范围、不变量）留下，本仓库自己的设计论证删掉。核对行为时阅读当前模块和对应工具的实现。
- 匹配周围代码的风格与抽象层级。**不要为一个场景写特例** —— 那通常说明缺一个模型，
  先把它找出来（领域对象、协议要求、数据结构），而不是加分支。
- **先量再改。** 判断「哪里慢」「为什么失败」之前先取真实数据（日志、探针、实测数字），
  不靠推理下结论。临时探针用完删掉。
- 交付前清理本任务的调试输出和临时资源；保留用户及其他任务已有的修改，不为清理工作区撤销它们。

**改动收尾**

- 搬文件或改路径后，**全仓扫一遍引用**（`git grep`，覆盖 `.github/`）。CI 里的路径最容易漏，
  本地构建绿不代表 CI 绿。
- 改了产物落地的位置，**清掉旧位置已经写进去的东西**。用户是按「打开那个目录看到什么」判断的，
  旧残留会让他以为你根本没改。
- 验证使用构建、实际操作和视觉验收。

## 代码结构

```
src/
  app/        控制器与 IPC 方法
  model/      五个模型协议 —— mod.rs（共享类型与 Model 抽象）/ protocol.rs（编解码）
  bridge/     BGI 接触面 —— mod.rs（客户端与 bgi.* 工具）/ control.rs（桥进程生命周期）
  extension/  工具契约，以及技能、插件、MCP 三类扩展来源
  setup/      安装引擎：解包、目录规则、快捷方式、注册表、卸载
  runtime/    Agent 运行时
    store/      持久化：journal、migrations、artifacts
    operation/  事务操作
      operations / kernel / verifier / permissions   事务本身
      task / task_store / executor                   快捷任务：领域模型、持久化、确定性执行器
      workflow                                       旧版提取结构的读取与转换（新写入不走这里）
    host/       宿主与扩展接触面：bridge、adapter、hooks、process、installation、catalog
web/src/
  brand/       产品图标、字标 SVG（可直接打开），以及内部用的木偶图
  ipc/         桌面 IPC、类型、宿主插件开关
  session/     会话订阅、草稿、对话分组
  appearance/  主题与界面语言
  models/      模型选择与预设
  components/
    shell/     AppShell、TitleBar、侧栏、详情栏
    chat/      对话记录与输入
    tasks/     TaskCard
    overlay/   Dialog、Toast
    controls/  表单与切换控件
    bridge/    BetterGI 页面组件
    icons/     描边图标；品牌从图标桶再导出
  pages/
    chat/ tasks/ extensions/ bridge/
    settings/  通用、模型、使用说明（设置里的标签页）
  setup/       安装器界面
bgi-bridge/
  native/     C++ 注入器与引导 DLL
  managed/    C# 桥本体，运行在 BetterGI 进程内
  recovery/   离线恢复工具
  dev/        开发期专用：元数据生成器、本地脚本
installer/    安装器载荷打包脚本
assets/       程序与安装器图标、.rc 与 UAC manifest，由 build.rs 编进 sleepy-doll.exe 与
  sleepy-doll-setup.exe
```

交付目录的形态：

```text
dist/Sleepy-Doll/
  sleepy-doll.exe
  bridge/      9 个 BgiBridge.* 文件与 bridge.config.json，必须整组同目录
  plugins/bgi/ 随产品分发的插件：BetterGI 的领域说明
  user/        用户数据，构建和安装都不动它；卸载默认保留，只有显式勾选才删
```

桥的运行期数据（日志、配置改动记录）与主程序共用安装根下的 `user/`。组件目录与数据根是两件事，
`src/bridge/control.rs` 把数据根写进 `bridge.config.json` 的 `userDirectory`。

## 领域插件与工具接入

领域知识和具体工具的操作说明通过插件提供，按插件启用状态装载。插件可以接入 HTTP 工具、MCP
或独立进程 Adapter，格式见 [docs/extensions.md](docs/extensions.md)。工具契约和宿主状态是
执行时的事实来源；技能说明不能代替接口是否存在、资源是否可用和操作是否完成的验证。

当前 BetterGI 的领域技能放在 `plugins/bgi/`，由 `build-desktop.cmd` 装入
`dist\Sleepy-Doll\plugins\bgi\`；其工具由 BGI 提供方登记，桥实现位于 `bgi-bridge/`。

- [`plugins/bgi/skills/bgi-assistant/SKILL.md`](plugins/bgi/skills/bgi-assistant/SKILL.md) ——
  助手行为规范、领域概念、能力边界与参考路由。`references/` 下是按需读取的资料，通过
  `skills.reference` 取用。
- [`plugins/bgi/skills/bgi-operator/SKILL.md`](plugins/bgi/skills/bgi-operator/SKILL.md) ——
  具体工具调用手册：配置组与任务的字段、`User\` 目录布局、脚本目录（`README.md` /
  `manifest.json` / `settings.json`）、执行模型、界面命令「操作当前选中项」的语义，以及
  各条稳定接口的调用顺序。
- [`plugins/bgi/skills/bgi-javascript/SKILL.md`](plugins/bgi/skills/bgi-javascript/SKILL.md) ——
  BGI JavaScript 的参数、入口、模块引用与执行机制阅读规范。

**插件目录里的技能归插件所有**：插件停用，它们就不在注册表里，界面上也不单独出现、没有自己的
开关。用户自己导入的技能走 `agent.skillDirectories`，那类才逐项开关。`CORE_AGENT_POLICY`
保持与领域无关，接入其他软件时不会带上 BGI 的规则。

本文件维护通用工程边界，不重复具体插件的操作手册。BGI 能力以当前桥接口目录为准；其他工具
以其适配层实际提供的契约为准，不将 BGI 的接口名和目录布局套用到其他工具。

## 容易踩的

- **BGI 全局设置通过桥的设置事务修改。** 运行中的 BetterGI 使用内存配置，直接修改磁盘文件
  可能被覆盖。脚本与配置组资源按各自契约处理，不能一概要求重启宿主。
- **桥更新需要核对加载版本。** 注入使用影拷贝，重新构建交付 DLL 不代表运行中的宿主已加载新版；
  需要按连接流程更新或重启 BetterGI，不为打包直接终止用户进程。
- **Anthropic 兼容端点要求把 `content[].thinking` 原样回传，包括 `signature`。** 丢掉这些块会让
  下一轮请求直接 400；各协议对推理载荷的要求不同，见 `src/model/protocol.rs`。
- **分发使用 `dist/Sleepy-Doll-<版本>-setup.exe`**，`dist/Sleepy-Doll/` 是本机组装目录，
  其中可能已有用户数据；`target/bridge/` 只是桥的中间产物。
- 用户的配置与密钥在 `<安装目录>\user\` 下，构建不会碰它（但 `rmdir` 式的清空会）。
- **`[profile.release]` 用 `panic = "abort"` + `opt-level = "s"` 换体积**（exe 8.5 MB，两者合计省 7.6 MB）。
  代价是没有展开清理路径，且 Rust 侧代码生成偏体积；若发现界面卡顿，删掉 `opt-level` 一行即可退回
  11.4 MB。
