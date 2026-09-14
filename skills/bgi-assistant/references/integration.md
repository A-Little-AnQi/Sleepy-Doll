# Agent 接入与能力目录设计

提示词只能决定 Agent 愿不愿意查能力，不能弥补一个只有 C# 方法签名的检索库。要让 600 多个能力可用，能力生成与检索层必须提供用户语义。

## 三层职责

1. 系统提示词固定身份、主动调用规则、普通用户语言与安全边界。可直接采用 `assets/system-prompt.md`。
2. 本 skill 提供 BGI 领域模型、工作流和排障知识，按任务加载相关 reference。
3. 动态能力目录保存当前版本的真实能力与参数；只在搜索和 describe 后把少量 schema 放进上下文。

不要把 600 个接口说明塞进系统提示词或一次性工具定义。不要让 skill 复制一份会迅速过期的方法清单。

## 每项能力的最小语义元数据

```yaml
id: internal.stable.id
title: 面向用户的功能名
summary: 一句话说明能完成什么
category: 独立任务/实时任务/脚本/配置/状态/诊断
when_to_use: 用户在什么目标下应该使用
user_phrases: [用户可能说的话]
aliases: [BGI术语, 常见简称, 旧称]
keywords: [搜索词]
preconditions: [运行前提]
side_effects: [消耗、覆盖、暂停其他任务等]
risk: read|routine|confirm|dangerous
async: true
surface_role: task|repository|config|navigation|ui_command|diagnostic|internal
completes_goal: true
execution_scope: item|directory_recursive|script|config_group
parameters:
  - name: internalName
    title: 用户可理解的参数名
    meaning: 它会怎样影响结果
    required: true
    examples: [示例]
returns: 用户可以据此确认什么
errors: 常见失败及用户可执行的处理
visibility: user|developer|hidden
```

搜索索引至少覆盖 `title + summary + when_to_use + user_phrases + aliases + keywords + parameters.title`。原始 XML 注释、方法名和类型名可作为低权重补充，不应主导召回。

## 暴露分级

- `user`：状态查询、高层任务、配置、安全的 BGI 操作，以及打开页面和明确 UI 命令，允许普通会话检索。
- `developer`：原始诊断、服务级能力和低层方法，仅在用户明确开发/排障时检索。
- `hidden`：DI 容器、窗口内部事件、反射基础设施、任意对象访问、不可序列化对象、ViewModel 生命周期方法和无业务含义的绑定属性等，不进入普通模型能力目录。

同一目标存在高低层方法时，为高层能力设置更高召回权重，并在低层能力元数据中声明 `superseded_by`。避免 `click`、`keyPress`、文件读写等通用方法在“传送、刷本、领取奖励”查询中排到前面。

不要按类型名整体屏蔽 ViewModel。把能够打开启动页、一条龙、调度器、地图追踪、脚本列表、自动战斗或设置页的命令标为 `navigation`；把行为边界清楚的页面按钮标为 `ui_command`。同时用 `completes_goal=false` 标记纯导航能力，防止 Agent 把“页面已打开”误认为“收集任务已完成”。

## 仓库与脚本能力

为了实现“收集某物”闭环，普通能力目录应覆盖：读取中央仓库状态与更新时间、更新中央仓库、列出/搜索 `repo.json`、读取仓库文本文件、查询已订阅路径、订阅/导入内容、更新已订阅内容、读取与写入脚本设置、创建或编辑配置组、运行路线/JS/配置组。重置仓库、删除用户副本和任意文件写入应保持高风险或开发可见。

仓库内容也应建立结构化索引，而不是只让模型全文搜索文件名。目录节点必须保留父子关系、完整路径、是否可递归运行和执行范围。对于地图路线，目标目录及 `目标@作者` 作者包应作为一级候选建立索引，`execution_scope=directory_recursive`；叶子 JSON 只作为父节点的内容与条件证据，默认不参与普通“收集某物”的执行候选排序。路线元数据至少索引目录分类、`info.name/type/version/description/bgi_version/tags/authors`、README 摘要和文件名中的地区/数量；JavaScript 脚本至少索引 manifest 名称、标签、描述、BGI 版本、`saved_files`、settings 的用户标签和 README 摘要。源码不需要常驻向量库，但应能按候选路径读取，以便 Agent 在配置或安全边界不明确时核对。

搜索结果要区分 `installed/subscribed/central-repo` 来源，并返回版本、兼容性、最后更新时间、用户数据是否会在更新时保留等信息。这样 Agent 才能优先复用现有配置，并在必要时自动更新或订阅。

目录搜索还应返回 `node_kind=target_root|author_package|subgroup|leaf`、父节点和子路线摘要。目标目录直接含叶子路线时，返回目标目录作为可执行候选；目标目录含多个作者包时，只返回作者包作为可执行候选。Agent 不应靠路径字符串猜测目录层级。

## 搜索返回格式

`capability.search` 只返回少量候选：`id`、`title`、`summary`、匹配原因、风险、是否异步和置信度，不返回完整 schema。`capability.describe` 才返回一个能力的参数和约束。搜索应支持中文分词、英文标识、别名和同义词，并允许 Agent 用目标短语而不是方法名查询。

## 状态上下文

`state.get` 应返回稳定的用户领域状态，而不是对象转储：BGI 是否就绪、游戏窗口与截图是否可用、当前任务及用户可见名称、关键实时任务开关、活动配置组、当前版本、最近错误摘要。原始日志和 CLR 对象通过开发诊断能力另取。

`capability.invoke` 的结果应包含 `status`、用户可见摘要、结构化证据、变更前后值或 task id、可重试性和标准错误码。这样模型可以判断“已开始”与“已完成”，而不是把 HTTP 成功误当成游戏目标成功。

## BetterGI 知识检索

要覆盖全局设置、一条龙和全部使用问题，除了动态接口目录，还需要让 Agent 能检索并读取三个知识源：`bettergi-docs/src` 用户文档、BetterGI 当前版本源码、`bettergi-scripts-list` 仓库内容。至少提供按关键词搜索和按路径读取文本的能力；搜索结果只给标题、路径与小段命中，Agent 选择后再读取完整小节或文件。

普通使用问题优先检索用户文档；文档缺失或与当前版本不一致时，检索页面 XAML、对应 ViewModel、`Core/Config` 和 `GameTask` 参数类；具体路线/脚本问题再查脚本仓库。不要把三套仓库内容一次性注入上下文。

为官方文档建立页面标题、功能名、旧称、界面路径和章节标题索引。为本体源码建立“用户功能 → 页面/ViewModel → 配置类 → GameTask”的关联索引。这样用户问“脚本启动前自动更新在哪”“一条龙的周日秘境怎么配”时，Agent 能找到真实页面和设置，而不是只命中反射出来的属性。

## 推荐的系统/技能装配

常驻上下文只放系统提示词、六个元工具的短说明和 BGI skill 的名称/description。普通请求触发 skill 后加载 `SKILL.md`；根据任务再加载一个或两个 reference。动态接口 schema 只保留当前步骤需要的候选，任务结束后可压缩为用户目标、选择、参数、task id 与结果摘要。

接入后用 [evaluation-cases.md](evaluation-cases.md) 做行为回归，重点检查是否主动查能力、是否误用底层操作、是否把“已开始”说成“已完成”，以及普通回复中是否泄露内部术语。领域资料的版本与更新范围见 [sources.md](sources.md)。
