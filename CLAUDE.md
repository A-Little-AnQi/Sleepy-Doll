# Sleepy Doll

面向 BetterGI 的本地桌面 Agent。Rust（桌面壳与 Agent 运行时）+ React（界面），
`bgi-bridge/` 是注入 BetterGI 进程的 .NET / C++ 桥。

## 构建与验证

```bash
npm ci
build-desktop.cmd          # 唯一的发布构建入口，产物在 dist\Sleepy-Doll\
```

| 命令 | 内容 |
|---|---|
| `npm run check` | 前端类型检查 + 生产构建 |
| `npm test` | 前端交互回归 |
| `dotnet run --project bgi-bridge/dev/ContractTests.csproj` | 桥的契约测试 |
| `cargo test --no-default-features` | 全部 Rust 测试 |
| `cargo check` 与 `cargo check --no-default-features` | 两种特性组合都要过 |
| `cargo clippy --all-targets --no-default-features -- -D warnings` | **CI 不执行 clippy，本地必须执行** |
| `cargo fmt --all -- --check` | 格式 |

安装程序是自绘的：`src/bin/sleepy-doll-setup.rs` 用与应用同一套 tao + wry + React 外壳起窗口，
界面在 `web/src/setup/`，外观与主程序一致。`build-desktop.cmd` 会先把交付目录打成压缩载荷
（`installer/pack-payload.ps1`），再编译这个 bin，产物在 `dist\Sleepy-Doll-<版本>-setup.exe`。
推 `v*` tag 时 `.github/workflows/release.yml` 自动执行整串。

`tests/live.rs` 是实机测试：用本机真实模型配置与真实桥执行一次完整问答，会消耗真实额度。
它标了 `#[ignore]`，**不要让它进 CI**。手动执行：

```bash
cargo test --no-default-features --test live -- --ignored --nocapture
```

## 约定

- **`.cmd` 脚本必须保持纯 ASCII。** cmd.exe 按 OEM 代码页读取，非 ASCII 字符会打乱命令解析
  （`build-desktop.cmd` 头部写着这条，违反过一次：中文注释让 `(` `)` 解析崩掉）。
- **构建是覆盖写入，不先清空目录。** 桥的 DLL 被运行中的 BetterGI 加载时删不掉，先删会留下
  半个不可用的安装目录。构建失败时逐个列出没替换成的文件，其余保持可用。
- 产物只有两个落点，都不进版本库：中间产物统一在 `target/`（`target/bridge`、`target/dotnet`、
  `target/ui`、`target/release`），交付物在 `dist/Sleepy-Doll/`。
- **注释只说明代码做什么、为什么必须这样，不叙述它以前是什么样。** 改动的来龙去脉属于提交信息，
  不属于源码；一条注释写三五行的辩解同样是噪音。
- 文档与提交信息用标准技术文体：动词用「执行」不用「跑」、产品名不译（Build Tools、token）。

## 协作方式

这一节约束的是我，不是项目。

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

- 注释只说明代码做什么、为什么必须这样，**不叙述它以前是什么样**。改动来历属于提交信息，
  不属于源码；三五行的辩解同样是噪音。
- 匹配周围代码的风格与抽象层级。**不要为一个场景写特例** —— 那通常说明缺一个模型，
  先把它找出来（领域对象、协议要求、数据结构），而不是加分支。
- **先量再改。** 判断「哪里慢」「为什么失败」之前先取真实数据（日志、探针、实测数字），
  不靠推理下结论。临时探针用完删掉。
- 交付前确认工作区干净：不留调试输出、不留临时文件。

**改动收尾**

- 搬文件或改路径后，**全仓扫一遍引用**（`git grep`，覆盖 `.github/`）。CI 里的路径最容易漏，
  本地构建绿不代表 CI 绿。
- 改了产物落地的位置，**清掉旧位置已经写进去的东西**。用户是按「打开那个目录看到什么」判断的，
  旧残留会让他以为你根本没改。
- 验证跑真实命令：构建、测试、实机。其中实机测试（`tests/live.rs`）用真实模型与真实桥，
  是唯一能证明端到端可用的手段。

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
  dev/        开发期专用：契约测试、元数据生成器、本地脚本
installer/    安装器载荷打包脚本
assets/       程序图标、.rc 与 UAC manifest，由 build.rs 编进 sleepy-doll.exe
```

交付目录的形态：

```text
dist/Sleepy-Doll/
  sleepy-doll.exe
  bridge/      9 个 BgiBridge.* 文件与 bridge.config.json，必须整组同目录
  skills/      随产品分发的能力包
  user/        用户数据，构建和安装都不动它；卸载默认保留，只有显式勾选才删
```

桥的运行期数据（日志、配置改动记录）与主程序共用安装根下的 `user/`。组件目录与数据根是两件事，
`src/bridge/control.rs` 把数据根写进 `bridge.config.json` 的 `userDirectory`。

## 领域知识

BetterGI 的领域知识随能力包分发，放在 `skills/` 下，由 `build-desktop.cmd` 一起装进
`dist\Sleepy-Doll\skills\`：

- [`skills/bgi-assistant/SKILL.md`](skills/bgi-assistant/SKILL.md) —— 助手行为规范、领域概念、
  能力边界与参考路由。`references/` 下是按需读取的资料，通过 `skills.reference` 取用。
- [`skills/bgi-operator/SKILL.md`](skills/bgi-operator/SKILL.md) —— 具体工具调用手册：
  配置组与任务的字段、`User\` 目录布局、脚本目录（`README.md` / `manifest.json` /
  `settings.json`）、执行模型、界面命令「操作当前选中项」的语义，以及各条稳定接口的调用顺序。

两份都标了 `requiresProviders: bgi`，**只在桥连接时注入**。`CORE_AGENT_POLICY` 保持与领域无关，
接入其他软件时不会带上 BGI 的规则。

**不要在这里复述它们的内容，也不要另写一份功能清单** —— 功能面以桥的接口目录为准
（`setting.` 全部配置项、`cmd.` 全部界面命令，每条自带中文说明）。

## 容易踩的

- **BetterGI 不监听配置文件。** 改完要重启 BetterGI 才生效；运行中的 BetterGI 保存设置时会把
  内存里的旧配置写回文件、覆盖改动。
- **桥的 DLL 被加载时构建会失败**（`Access is denied`）。BetterGI 是提权进程，非提权会话杀不掉它，
  要用提权方式终止。
- **Anthropic 兼容端点要求把 `content[].thinking` 原样回传，包括 `signature`。** 丢掉这些块会让
  下一轮请求直接 400；各协议对推理载荷的要求不同，见 `src/model/protocol.rs`。
- **交付的是 `dist/Sleepy-Doll/`**，`target/bridge/` 只是桥的中间产物。
- 用户的配置与密钥在 `<安装目录>\user\` 下，构建不会碰它（但 `rmdir` 式的清空会）。
- **`[profile.release]` 用 `panic = "abort"` + `opt-level = "s"` 换体积**（exe 8.5 MB，两者合计省 7.6 MB）。
  代价是没有展开清理路径，且 Rust 侧代码生成偏体积；若发现界面卡顿，删掉 `opt-level` 一行即可退回
  11.4 MB。
