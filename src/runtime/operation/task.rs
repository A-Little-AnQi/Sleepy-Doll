//! 快捷任务：可保存的确定性定义与不可变修订。
//!
//! 定义与修订分开：定义持有名称、来源与发布指针，修订是不可变快照。运行只读取
//! 已发布的修订，草稿失败不影响线上版本。控制流在此处做静态校验 —— 语法合法不
//! 等于可执行，但结构错误必须在发布前拦下，而不是运行到一半才失败。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::Result,
    extension::{ToolEffect, ToolExecution},
};

/// 修订结构的版本号。外部导入的定义按此重新校验，不继承自带的验证结论。
pub const SCHEMA_VERSION: u32 = 1;

pub const MAX_NODES: usize = 200;
pub const MAX_LOOP_EXPANSIONS: u32 = 100;
pub const MAX_TOOL_ATTEMPTS: u32 = 1000;
pub const MAX_FOREACH_ITEMS: usize = 100;
pub const MAX_FOREACH_ITEMS_LIMIT: usize = 1000;
pub const MAX_REPEAT_ITERATIONS: u32 = 100;
pub const MAX_WAIT_SECONDS: u64 = 86_400;
pub const MIN_WAIT_CHECK_SECONDS: u64 = 1;
pub const DEFAULT_WAIT_CHECK_SECONDS: u64 = 5;
pub const DEFAULT_RUN_SECONDS: i64 = 1800;
pub const MAX_RUN_SECONDS: i64 = 86_400;
pub const MAX_TEMPLATE_CHARS: usize = 16 * 1024;

/// 零 token 承诺的依据来自工具契约（见 `crate::extension::ModelUsage`）。
pub use crate::extension::ModelUsage;

/// 效果无法判定的工具不能被当作确定性工具。
pub fn resolve_usage(declared: ModelUsage, effect: ToolEffect) -> ModelUsage {
    match (declared, effect) {
        (ModelUsage::None, ToolEffect::Unknown) => ModelUsage::Unknown,
        (usage, _) => usage,
    }
}

/// 失败策略。默认 `stop`；无幂等保证的写入不能套通用重试。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FailurePolicy {
    #[default]
    Stop,
    ContinueIndependent,
    BoundedRetry {
        max_attempts: u32,
        backoff_ms: u64,
    },
    Compensate {
        #[serde(default)]
        tool: Option<String>,
        #[serde(default)]
        arguments: Value,
    },
}

/// 条件求值的三值结果。缺字段是 unknown，不当作 false。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    pub fn from_bool(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
    pub fn negate(self) -> Self {
        match self {
            Self::True => Self::False,
            Self::False => Self::True,
            Self::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareOp {
    Equals,
    NotEquals,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

impl CompareOp {
    fn apply(self, left: &Value, right: &Value) -> Truth {
        if self == Self::Equals || self == Self::NotEquals {
            let equal = left == right;
            return Truth::from_bool(if self == Self::Equals { equal } else { !equal });
        }
        let (Some(left), Some(right)) = (number(left), number(right)) else {
            return Truth::Unknown;
        };
        let ordering = left.partial_cmp(&right);
        match (self, ordering) {
            (_, None) => Truth::Unknown,
            (CompareOp::Less, Some(order)) => Truth::from_bool(order.is_lt()),
            (CompareOp::LessOrEqual, Some(order)) => Truth::from_bool(order.is_le()),
            (CompareOp::Greater, Some(order)) => Truth::from_bool(order.is_gt()),
            (CompareOp::GreaterOrEqual, Some(order)) => Truth::from_bool(order.is_ge()),
            _ => Truth::Unknown,
        }
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        // 数值比较允许字符串化的数字：宿主返回值常以文本返回。
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

/// 取值引用。只允许解析后的节点/字段路径，不接受任意表达式。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ValueRef {
    Literal {
        value: Value,
    },
    /// `nodes.<nodeId>.output.<field...>` 的解析形式。
    NodeOutput {
        node: String,
        #[serde(default)]
        path: Vec<String>,
    },
    /// 当前循环项；仅在 foreach 内部有定义。
    Item {
        #[serde(default)]
        path: Vec<String>,
    },
    /// 当前迭代序号，从 1 开始。
    Iteration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Condition {
    Always {
        value: bool,
    },
    Exists {
        value: ValueRef,
    },
    Compare {
        op: CompareOp,
        left: ValueRef,
        right: ValueRef,
    },
    All {
        conditions: Vec<Condition>,
    },
    Any {
        conditions: Vec<Condition>,
    },
    Not {
        condition: Box<Condition>,
    },
}

impl Condition {
    pub fn evaluate(&self, scope: &Scope) -> Truth {
        match self {
            Condition::Always { value } => Truth::from_bool(*value),
            Condition::Exists { value } => match value.resolve(scope) {
                Some(Value::Null) | None => Truth::False,
                Some(_) => Truth::True,
            },
            Condition::Compare { op, left, right } => {
                let (Some(left), Some(right)) = (left.resolve(scope), right.resolve(scope)) else {
                    return Truth::Unknown;
                };
                op.apply(&left, &right)
            }
            // 三值逻辑：任一为 unknown 时，all 只有出现 false 才确定；any 只有
            // 出现 true 才确定。
            Condition::All { conditions } => {
                let mut unknown = false;
                for condition in conditions {
                    match condition.evaluate(scope) {
                        Truth::False => return Truth::False,
                        Truth::Unknown => unknown = true,
                        Truth::True => {}
                    }
                }
                if unknown { Truth::Unknown } else { Truth::True }
            }
            Condition::Any { conditions } => {
                let mut unknown = false;
                for condition in conditions {
                    match condition.evaluate(scope) {
                        Truth::True => return Truth::True,
                        Truth::Unknown => unknown = true,
                        Truth::False => {}
                    }
                }
                if unknown {
                    Truth::Unknown
                } else {
                    Truth::False
                }
            }
            Condition::Not { condition } => condition.evaluate(scope).negate(),
        }
    }

    fn visit(&self, visit: &mut impl FnMut(&ValueRef)) {
        match self {
            Condition::Always { .. } => {}
            Condition::Exists { value } => visit(value),
            Condition::Compare { left, right, .. } => {
                visit(left);
                visit(right);
            }
            Condition::All { conditions } | Condition::Any { conditions } => {
                for condition in conditions {
                    condition.visit(visit);
                }
            }
            Condition::Not { condition } => condition.visit(visit),
        }
    }
}

/// 运行期取值作用域：已完成节点的输出、当前循环项与迭代序号。
#[derive(Debug, Default, Clone)]
pub struct Scope {
    outputs: HashMap<String, Value>,
    item: Option<Value>,
    iteration: Option<u32>,
}

impl Scope {
    pub fn with_item(&self, item: Value, iteration: u32) -> Self {
        Self {
            outputs: self.outputs.clone(),
            item: Some(item),
            iteration: Some(iteration),
        }
    }
    pub fn record(&mut self, node: &str, output: Value) {
        self.outputs.insert(node.to_owned(), output);
    }
    pub fn output(&self, node: &str) -> Option<&Value> {
        self.outputs.get(node)
    }
}

impl ValueRef {
    pub fn resolve(&self, scope: &Scope) -> Option<Value> {
        match self {
            ValueRef::Literal { value } => Some(value.clone()),
            ValueRef::NodeOutput { node, path } => {
                Some(descend(scope.output(node)?, path)?.clone())
            }
            ValueRef::Item { path } => Some(descend(scope.item.as_ref()?, path)?.clone()),
            ValueRef::Iteration => scope.iteration.map(Value::from),
        }
    }
}

/// 只有 `null` 与缺失被视为「取不到」；`false`、`0`、空串都是有效值。
fn descend<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for segment in path {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolNode {
    pub id: String,
    pub title: String,
    /// 显式缺省时由编译器按工具名补齐；外部导入的定义必须自带。
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub capability_id: Option<String>,
    pub arguments: Value,
    #[serde(default)]
    pub execution: Option<ToolExecution>,
    #[serde(default)]
    pub provider_version: Option<String>,
    #[serde(default)]
    pub resource_versions: Vec<String>,
    #[serde(default)]
    pub on_failure: FailurePolicy,
    /// 工具调用成功但业务核验未通过时使用的策略。
    #[serde(default)]
    pub on_unverified: FailurePolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SequenceNode {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub nodes: Vec<TaskNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConditionNode {
    pub id: String,
    pub title: String,
    pub condition: Condition,
    #[serde(default)]
    pub then: Vec<TaskNode>,
    #[serde(default)]
    pub otherwise: Vec<TaskNode>,
    /// 缺字段时走这里。必须有内容：unknown 不能被当成 false。
    pub unknown: Vec<TaskNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForEachNode {
    pub id: String,
    pub title: String,
    pub items: ValueRef,
    /// 稳定项键，用于重启后识别已完成的项。
    pub item_key: String,
    pub nodes: Vec<TaskNode>,
    #[serde(default = "default_foreach_limit")]
    pub max_items: usize,
}

fn default_foreach_limit() -> usize {
    MAX_FOREACH_ITEMS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepeatNode {
    pub id: String,
    pub title: String,
    /// 固定次数；与 `until` 至少有一个。
    #[serde(default)]
    pub count: Option<u32>,
    /// 退出条件，每轮结束后求值。
    #[serde(default)]
    pub until: Option<Condition>,
    pub nodes: Vec<TaskNode>,
    #[serde(default = "default_repeat_iterations")]
    pub max_iterations: u32,
}

fn default_repeat_iterations() -> u32 {
    MAX_REPEAT_ITERATIONS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WaitNode {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub seconds: Option<u64>,
    /// 契约支持的只读条件等待；与 `seconds` 至少有一个。
    #[serde(default)]
    pub until: Option<Condition>,
    /// 条件等待时每轮调用的只读检查步骤。条件写在 `until` 里，取值来自这一步。
    #[serde(default)]
    pub probe: Option<Box<ToolNode>>,
    #[serde(default = "default_wait_check")]
    pub check_seconds: u64,
    /// 绝对期限（秒）。无条件等待取 `seconds`，条件等待必须显式给出。
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

fn default_wait_check() -> u64 {
    DEFAULT_WAIT_CHECK_SECONDS
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultNode {
    pub id: String,
    pub title: String,
    /// 受限模板：只允许 `{{ nodes.<id>.output.<path> }}` 与 `{{ item }}`。
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub outputs: Vec<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskNode {
    Tool(ToolNode),
    Sequence(SequenceNode),
    Condition(ConditionNode),
    ForEach(ForEachNode),
    Repeat(RepeatNode),
    Wait(WaitNode),
    Result(ResultNode),
}

impl TaskNode {
    pub fn id(&self) -> &str {
        match self {
            TaskNode::Tool(node) => &node.id,
            TaskNode::Sequence(node) => &node.id,
            TaskNode::Condition(node) => &node.id,
            TaskNode::ForEach(node) => &node.id,
            TaskNode::Repeat(node) => &node.id,
            TaskNode::Wait(node) => &node.id,
            TaskNode::Result(node) => &node.id,
        }
    }
    pub fn title(&self) -> &str {
        match self {
            TaskNode::Tool(node) => &node.title,
            TaskNode::Sequence(node) => &node.title,
            TaskNode::Condition(node) => &node.title,
            TaskNode::ForEach(node) => &node.title,
            TaskNode::Repeat(node) => &node.title,
            TaskNode::Wait(node) => &node.title,
            TaskNode::Result(node) => &node.title,
        }
    }
    /// 该节点在用户界面上的中文动作名，用于「正在执行：步骤名」。
    pub fn kind_label(&self) -> &'static str {
        match self {
            TaskNode::Tool(_) => "调用工具",
            TaskNode::Sequence(_) => "顺序执行",
            TaskNode::Condition(_) => "判断条件",
            TaskNode::ForEach(_) => "逐项执行",
            TaskNode::Repeat(_) => "重复执行",
            TaskNode::Wait(_) => "等待",
            TaskNode::Result(_) => "生成结果",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskLimits {
    pub max_nodes: usize,
    pub max_loop_expansions: u32,
    pub max_tool_attempts: u32,
    pub max_seconds: i64,
}

impl Default for TaskLimits {
    fn default() -> Self {
        Self {
            max_nodes: MAX_NODES,
            max_loop_expansions: MAX_LOOP_EXPANSIONS,
            max_tool_attempts: MAX_TOOL_ATTEMPTS,
            max_seconds: MAX_RUN_SECONDS,
        }
    }
}

/// 静态验证结论。静态通过只说明结构合法，不代表实机可用。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskValidation {
    pub model_usage: ModelUsage,
    pub node_count: usize,
    pub max_expansion: u32,
    #[serde(default)]
    pub issues: Vec<TaskIssue>,
    #[serde(default)]
    pub missing_bindings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIssue {
    pub node_id: String,
    pub message: String,
}

impl TaskValidation {
    pub fn publishable(&self) -> bool {
        self.issues.is_empty() && self.missing_bindings.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowRevision {
    pub task_id: String,
    pub revision: u64,
    pub schema_version: u32,
    pub name: String,
    pub description: String,
    pub nodes: Vec<TaskNode>,
    #[serde(default)]
    pub limits: TaskLimits,
    pub model_usage: ModelUsage,
    #[serde(default)]
    pub validation: TaskValidation,
    pub created_at: String,
}

/// 定义状态机的可见形态。文案与主按钮直接对应产品表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefinitionState {
    Draft,
    Invalid,
    ReadyUnverified,
    ReadyVerified,
    Unavailable,
    Archived,
    Deleted,
}

impl DefinitionState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "正在制作",
            Self::Invalid => "暂时无法生成可运行任务",
            Self::ReadyUnverified => "可运行 · 尚未实机验证",
            Self::ReadyVerified => "可运行",
            Self::Unavailable => "需要连接工具",
            Self::Archived => "已归档",
            Self::Deleted => "已删除",
        }
    }
    pub fn action(self) -> &'static str {
        match self {
            Self::Draft => "补充信息",
            Self::Invalid => "查看原因",
            Self::ReadyUnverified | Self::ReadyVerified => "运行",
            Self::Unavailable => "连接工具",
            Self::Archived => "恢复",
            Self::Deleted => "",
        }
    }
    pub fn runnable(self) -> bool {
        matches!(self, Self::ReadyUnverified | Self::ReadyVerified)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub source_conversation_id: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
    /// 来源会话被删除后仍可读的标题快照。
    #[serde(default)]
    pub source_title_snapshot: String,
    #[serde(default)]
    pub published_revision: Option<u64>,
    #[serde(default)]
    pub draft_revision: Option<u64>,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub last_run_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 编译与校验用的工具目录视图。Core 不硬编码任何领域字段。
pub struct ToolCatalog {
    entries: HashMap<String, ToolContract>,
}

#[derive(Debug, Clone)]
pub struct ToolContract {
    pub execution: ToolExecution,
    pub provider_version: Option<String>,
    pub input_schema: Value,
}

impl ToolCatalog {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
    pub fn insert(&mut self, name: &str, contract: ToolContract) {
        self.entries.insert(name.to_owned(), contract);
    }
    pub fn get(&self, name: &str) -> Option<&ToolContract> {
        self.entries.get(name)
    }
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// 编译：结构/类型校验 → 工具与控制流校验 → 资源绑定 → 效果与权限计算。
/// 只做静态检查，不产生任何真实工具写入。
pub fn compile(
    task_id: &str,
    revision: u64,
    name: &str,
    description: &str,
    mut nodes: Vec<TaskNode>,
    limits: Option<TaskLimits>,
    catalog: &ToolCatalog,
) -> Result<WorkflowRevision> {
    let limits = limits.unwrap_or_default();
    // 先把契约快照写进修订，校验再读修订自己的字段：运行期与发布期看到的是
    // 同一份契约，不随工具目录变化而漂移。
    bind_contracts(&mut nodes, catalog);
    let mut validation = TaskValidation::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut available: HashSet<String> = HashSet::new();
    let mut expansions: u32 = 0;
    let mut attempts: u32 = 0;
    let mut model_usage = ModelUsage::None;
    walk(
        &nodes,
        &mut Walk {
            catalog,
            seen: &mut seen,
            available: &mut available,
            validation: &mut validation,
            expansions: &mut expansions,
            attempts: &mut attempts,
            model_usage: &mut model_usage,
            item_scoped: false,
            iteration_scoped: false,
        },
    );
    if seen.len() > limits.max_nodes {
        validation.issues.push(TaskIssue {
            node_id: String::new(),
            message: format!(
                "任务包含 {} 个节点，超过上限 {}",
                seen.len(),
                limits.max_nodes
            ),
        });
    }
    if expansions > limits.max_loop_expansions {
        validation.issues.push(TaskIssue {
            node_id: String::new(),
            message: format!(
                "循环最多展开 {expansions} 次，超过上限 {}",
                limits.max_loop_expansions
            ),
        });
    }
    if attempts > limits.max_tool_attempts {
        validation.issues.push(TaskIssue {
            node_id: String::new(),
            message: format!(
                "工具调用最多 {attempts} 次，超过上限 {}",
                limits.max_tool_attempts
            ),
        });
    }
    if limits.max_seconds > MAX_RUN_SECONDS {
        validation.issues.push(TaskIssue {
            node_id: String::new(),
            message: format!("单次运行上限不能超过 {} 小时", MAX_RUN_SECONDS / 3600),
        });
    }
    validation.node_count = seen.len();
    validation.max_expansion = expansions;
    validation.model_usage = model_usage;
    if name.trim().is_empty() || name.chars().count() > 120 {
        validation.issues.push(TaskIssue {
            node_id: String::new(),
            message: "任务名称应为 1 到 120 个字符".into(),
        });
    }
    Ok(WorkflowRevision {
        task_id: task_id.into(),
        revision,
        schema_version: SCHEMA_VERSION,
        name: name.trim().into(),
        description: description.trim().into(),
        nodes,
        limits,
        model_usage,
        validation,
        created_at: crate::runtime::types::now(),
    })
}

/// 按工具名补齐执行契约与提供方版本，让修订自带发布时的快照。
fn bind_contracts(nodes: &mut [TaskNode], catalog: &ToolCatalog) {
    for node in nodes {
        match node {
            TaskNode::Tool(tool) => {
                if let Some(contract) = tool.tool.as_deref().and_then(|name| catalog.get(name)) {
                    if tool.execution.is_none() {
                        tool.execution = Some(contract.execution.clone());
                    }
                    // 已绑定的版本是修订的一部分：重新编译不能让旧定义悄悄跟上
                    // 新契约，契约变了要报出来要求重新验证。
                    if tool.provider_version.is_none() {
                        tool.provider_version = contract.provider_version.clone();
                    }
                }
            }
            TaskNode::Sequence(node) => bind_contracts(&mut node.nodes, catalog),
            TaskNode::Condition(node) => {
                bind_contracts(&mut node.then, catalog);
                bind_contracts(&mut node.otherwise, catalog);
                bind_contracts(&mut node.unknown, catalog);
            }
            TaskNode::ForEach(node) => bind_contracts(&mut node.nodes, catalog),
            TaskNode::Repeat(node) => bind_contracts(&mut node.nodes, catalog),
            TaskNode::Wait(node) => {
                if let Some(probe) = node.probe.as_mut()
                    && let Some(contract) = probe.tool.as_deref().and_then(|name| catalog.get(name))
                {
                    if probe.execution.is_none() {
                        probe.execution = Some(contract.execution.clone());
                    }
                    if probe.provider_version.is_none() {
                        probe.provider_version = contract.provider_version.clone();
                    }
                }
            }
            TaskNode::Result(_) => {}
        }
    }
}

struct Walk<'a> {
    catalog: &'a ToolCatalog,
    seen: &'a mut HashSet<String>,
    available: &'a mut HashSet<String>,
    validation: &'a mut TaskValidation,
    expansions: &'a mut u32,
    attempts: &'a mut u32,
    model_usage: &'a mut ModelUsage,
    /// 当前是否在 foreach 体内（item 有定义）。
    item_scoped: bool,
    /// 当前是否在 repeat 体内（iteration 有定义）。
    iteration_scoped: bool,
}

fn walk(nodes: &[TaskNode], state: &mut Walk<'_>) {
    for node in nodes {
        let id = node.id().to_owned();
        if id.trim().is_empty() {
            state.validation.issues.push(TaskIssue {
                node_id: id,
                message: "节点缺少 ID".into(),
            });
            continue;
        }
        if !state.seen.insert(id.clone()) {
            state.validation.issues.push(TaskIssue {
                node_id: id,
                message: "节点 ID 重复".into(),
            });
            continue;
        }
        match node {
            TaskNode::Tool(tool) => walk_tool(tool, state),
            TaskNode::Sequence(sequence) => {
                walk(&sequence.nodes, state);
                state.available.insert(id);
            }
            TaskNode::Condition(condition) => {
                check_condition(&condition.condition, state, &id);
                if condition.unknown.is_empty() {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "条件缺少 unknown 分支：缺字段时不能当作 false".into(),
                    });
                }
                walk(&condition.then, state);
                walk(&condition.otherwise, state);
                walk(&condition.unknown, state);
                state.available.insert(id);
            }
            TaskNode::ForEach(each) => {
                check_ref(&each.items, state, &id);
                if each.item_key.trim().is_empty() {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "逐项执行需要稳定的项键".into(),
                    });
                }
                if each.max_items == 0 || each.max_items > MAX_FOREACH_ITEMS_LIMIT {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: format!("逐项上限应为 1 到 {MAX_FOREACH_ITEMS_LIMIT}"),
                    });
                }
                *state.expansions = state.expansions.saturating_add(each.max_items as u32);
                let outer = state.item_scoped;
                state.item_scoped = true;
                walk(&each.nodes, state);
                state.item_scoped = outer;
                state.available.insert(id);
            }
            TaskNode::Repeat(repeat) => {
                if repeat.count.is_none() && repeat.until.is_none() {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "重复执行需要明确次数或退出条件".into(),
                    });
                }
                if repeat.max_iterations == 0 || repeat.max_iterations > MAX_REPEAT_ITERATIONS {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: format!("重复次数上限应在 1 到 {MAX_REPEAT_ITERATIONS} 之间"),
                    });
                }
                if let Some(count) = repeat.count
                    && (count == 0 || count > repeat.max_iterations)
                {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "重复次数必须为正且不超过上限".into(),
                    });
                }
                if let Some(until) = &repeat.until {
                    check_condition(until, state, &id);
                }
                *state.expansions = state.expansions.saturating_add(repeat.max_iterations);
                let outer = state.iteration_scoped;
                state.iteration_scoped = true;
                walk(&repeat.nodes, state);
                state.iteration_scoped = outer;
                state.available.insert(id);
            }
            TaskNode::Wait(wait) => {
                if wait.seconds.is_none() && wait.until.is_none() {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "等待需要固定时长或只读条件".into(),
                    });
                }
                if let Some(seconds) = wait.seconds
                    && (seconds == 0 || seconds > MAX_WAIT_SECONDS)
                {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: format!("等待时长应在 1 到 {MAX_WAIT_SECONDS} 秒之间"),
                    });
                }
                if wait.check_seconds < MIN_WAIT_CHECK_SECONDS {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: format!("检查间隔不能小于 {MIN_WAIT_CHECK_SECONDS} 秒"),
                    });
                }
                if wait.until.is_some() && wait.timeout_seconds.is_none() {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "条件等待必须有绝对期限".into(),
                    });
                }
                if let Some(until) = &wait.until {
                    check_condition(until, state, &id);
                }
                if let Some(probe) = &wait.probe {
                    let probe_id = format!("{}·检查", probe.id);
                    let mut probe = probe.clone();
                    probe.id = probe_id.clone();
                    if !state.seen.insert(probe_id.clone()) {
                        state.validation.issues.push(TaskIssue {
                            node_id: id.clone(),
                            message: "检查步骤的 ID 与已有节点重复".into(),
                        });
                    }
                    walk_tool(&probe, state);
                    // 检查步骤必须是只读的，否则等待会变成反复写入。
                    if let Some(name) = probe.tool.as_deref()
                        && let Some(contract) = state.catalog.get(name)
                        && contract.execution.effect != ToolEffect::ReadOnly
                    {
                        state.validation.issues.push(TaskIssue {
                            node_id: id.clone(),
                            message: "等待中的检查步骤必须是只读的".into(),
                        });
                    }
                    state.available.insert(probe_id);
                }
                state.available.insert(id);
            }
            TaskNode::Result(result) => {
                for reference in &result.outputs {
                    check_ref(reference, state, &id);
                }
                if result.template.chars().count() > MAX_TEMPLATE_CHARS {
                    state.validation.issues.push(TaskIssue {
                        node_id: id.clone(),
                        message: "结果模板过长".into(),
                    });
                }
                for reference in template_refs(&result.template) {
                    check_ref(&reference, state, &id);
                }
                state.available.insert(id);
            }
        }
    }
}

fn walk_tool(tool: &ToolNode, state: &mut Walk<'_>) {
    let id = tool.id.clone();
    if !tool.arguments.is_object() {
        state.validation.issues.push(TaskIssue {
            node_id: id.clone(),
            message: "工具参数必须是对象".into(),
        });
    }
    let Some(name) = tool.tool.as_deref() else {
        state.validation.issues.push(TaskIssue {
            node_id: id.clone(),
            message: "工具节点没有绑定工具".into(),
        });
        return;
    };
    let Some(contract) = state.catalog.get(name) else {
        // 缺契约不阻止保存草稿，但阻止发布 —— 由调用方按 issues 判定。
        state.validation.issues.push(TaskIssue {
            node_id: id.clone(),
            message: format!("工具「{name}」当前不可用，无法验证参数"),
        });
        *state.model_usage = ModelUsage::Unknown;
        return;
    };
    if !crate::extension::validate(&tool.arguments, &contract.input_schema, "$").is_empty() {
        state.validation.issues.push(TaskIssue {
            node_id: id.clone(),
            message: "参数不符合工具契约".into(),
        });
    }
    check_refs_in_value(&tool.arguments, state, &id);
    if let (Some(bound), Some(current)) = (&tool.provider_version, &contract.provider_version)
        && bound != current
    {
        state.validation.issues.push(TaskIssue {
            node_id: id.clone(),
            message: "工具契约版本已变化，需要重新验证".into(),
        });
    }
    let usage = resolve_usage(contract.execution.model_usage, contract.execution.effect);
    if !usage.is_deterministic() {
        *state.model_usage = if usage == ModelUsage::Unknown {
            ModelUsage::Unknown
        } else if *state.model_usage != ModelUsage::Unknown {
            ModelUsage::Possible
        } else {
            *state.model_usage
        };
    }
    validate_retry(tool, state, &id);
    *state.attempts = state.attempts.saturating_add(match tool.on_failure {
        FailurePolicy::BoundedRetry { max_attempts, .. } => max_attempts.max(1),
        _ => 1,
    });
    state.available.insert(id);
}

/// 无幂等保证的写入不能套通用重试。
fn validate_retry(tool: &ToolNode, state: &mut Walk<'_>, id: &str) {
    for policy in [&tool.on_failure, &tool.on_unverified] {
        if let FailurePolicy::BoundedRetry { max_attempts, .. } = policy
            && *max_attempts > 1
            && let Some(contract) = tool
                .tool
                .as_deref()
                .and_then(|name| state.catalog.get(name))
            && contract.execution.idempotency == crate::extension::IdempotencyMode::NeverRetry
        {
            state.validation.issues.push(TaskIssue {
                node_id: id.to_owned(),
                message: "该工具没有幂等保证，不能配置自动重试".into(),
            });
        }
    }
}

fn check_condition(condition: &Condition, state: &mut Walk<'_>, id: &str) {
    condition.visit(&mut |reference| check_ref(reference, state, id));
}

fn check_ref(reference: &ValueRef, state: &mut Walk<'_>, id: &str) {
    match reference {
        ValueRef::Literal { .. } => {}
        ValueRef::NodeOutput { node, .. } => {
            if !state.available.contains(node) {
                state.validation.issues.push(TaskIssue {
                    node_id: id.to_owned(),
                    message: format!("引用了尚未执行或不在当前作用域的节点「{node}」"),
                });
            }
        }
        ValueRef::Item { .. } => {
            if !state.item_scoped {
                state.validation.issues.push(TaskIssue {
                    node_id: id.to_owned(),
                    message: "循环项只在逐项执行的步骤内有定义".into(),
                });
            }
        }
        ValueRef::Iteration => {
            if !state.iteration_scoped {
                state.validation.issues.push(TaskIssue {
                    node_id: id.to_owned(),
                    message: "迭代序号只在重复执行的步骤内有定义".into(),
                });
            }
        }
    }
}

/// 参数里的引用写成 `{"$ref":{"node":"a","path":["b"]}}`，编译时解析成显式路径。
fn check_refs_in_value(value: &Value, state: &mut Walk<'_>, id: &str) {
    match value {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref")
                && let Ok(parsed) = serde_json::from_value::<ValueRef>(reference.clone())
            {
                check_ref(&parsed, state, id);
                return;
            }
            for nested in map.values() {
                check_refs_in_value(nested, state, id);
            }
        }
        Value::Array(items) => {
            for nested in items {
                check_refs_in_value(nested, state, id);
            }
        }
        _ => {}
    }
}

/// 结果模板只允许 `{{ ... }}` 里的受限引用，其余按字面量转义输出。
pub fn template_refs(template: &str) -> Vec<ValueRef> {
    let mut refs = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let expression = after[..end].trim();
        if let Some(reference) = parse_expression(expression) {
            refs.push(reference);
        }
        rest = &after[end + 2..];
    }
    refs
}

fn parse_expression(expression: &str) -> Option<ValueRef> {
    if expression == "item" {
        return Some(ValueRef::Item { path: vec![] });
    }
    if expression == "iteration" {
        return Some(ValueRef::Iteration);
    }
    let path = expression.strip_prefix("nodes.")?;
    let mut segments = path.split('.').map(str::to_owned).collect::<Vec<_>>();
    if segments.is_empty() {
        return None;
    }
    let node = segments.remove(0);
    let path = segments
        .into_iter()
        .filter(|segment| segment != "output")
        .collect::<Vec<_>>();
    Some(ValueRef::NodeOutput { node, path })
}

/// 受限模板渲染。取不到的引用保持为空串并如实记账，不编造内容。
pub fn render_template(template: &str, scope: &Scope) -> (String, Vec<String>) {
    let mut missing = Vec::new();
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            rendered.push_str(&rest[start..]);
            return (rendered, missing);
        };
        let expression = after[..end].trim();
        match parse_expression(expression).and_then(|reference| reference.resolve(scope)) {
            Some(Value::String(text)) => rendered.push_str(&text),
            Some(value) => rendered.push_str(&value.to_string()),
            None => missing.push(expression.to_owned()),
        }
        rest = &after[end + 2..];
    }
    rendered.push_str(rest);
    (rendered, missing)
}

/// 运行前把参数里的 `$ref` 解析成真实值；取不到时返回缺失路径名。
pub fn resolve_arguments(arguments: &Value, scope: &Scope) -> std::result::Result<Value, String> {
    match arguments {
        Value::Object(map) => {
            if let Some(reference) = map.get("$ref")
                && map.len() == 1
            {
                let parsed: ValueRef = serde_json::from_value(reference.clone())
                    .map_err(|_| "引用格式无效".to_owned())?;
                return parsed
                    .resolve(scope)
                    .ok_or_else(|| format!("引用 {} 当前取不到值", describe_ref(&parsed)));
            }
            let mut resolved = serde_json::Map::new();
            for (key, value) in map {
                resolved.insert(key.clone(), resolve_arguments(value, scope)?);
            }
            Ok(Value::Object(resolved))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| resolve_arguments(item, scope))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Value::Array),
        other => Ok(other.clone()),
    }
}

fn describe_ref(reference: &ValueRef) -> String {
    match reference {
        ValueRef::Literal { .. } => "常量".into(),
        ValueRef::NodeOutput { node, path } => {
            if path.is_empty() {
                node.clone()
            } else {
                format!("{node}.{}", path.join("."))
            }
        }
        ValueRef::Item { .. } => "循环项".into(),
        ValueRef::Iteration => "迭代序号".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(id: &str) -> TaskNode {
        TaskNode::Tool(ToolNode {
            id: id.into(),
            title: id.into(),
            tool: Some("demo.read".into()),
            capability_id: None,
            arguments: json!({}),
            execution: None,
            provider_version: None,
            resource_versions: vec![],
            on_failure: FailurePolicy::Stop,
            on_unverified: FailurePolicy::Stop,
        })
    }

    fn catalog() -> ToolCatalog {
        let mut catalog = ToolCatalog::new();
        catalog.insert(
            "demo.read",
            ToolContract {
                execution: ToolExecution::read_only(),
                provider_version: None,
                input_schema: json!({"type":"object","additionalProperties":true}),
            },
        );
        catalog
    }

    #[test]
    fn unknown_is_not_false_in_conditions() {
        let scope = Scope::default();
        let condition = Condition::Compare {
            op: CompareOp::Equals,
            left: ValueRef::NodeOutput {
                node: "a".into(),
                path: vec!["status".into()],
            },
            right: ValueRef::Literal { value: json!(1) },
        };
        assert_eq!(condition.evaluate(&scope), Truth::Unknown);
        assert_eq!(
            Condition::Not {
                condition: Box::new(condition.clone())
            }
            .evaluate(&scope),
            Truth::Unknown
        );
        assert_eq!(
            Condition::All {
                conditions: vec![Condition::Always { value: false }, condition.clone()]
            }
            .evaluate(&scope),
            Truth::False
        );
        assert_eq!(
            Condition::Any {
                conditions: vec![Condition::Always { value: true }, condition]
            }
            .evaluate(&scope),
            Truth::True
        );
    }

    #[test]
    fn condition_without_unknown_branch_is_rejected() {
        let nodes = vec![TaskNode::Condition(ConditionNode {
            id: "c".into(),
            title: "c".into(),
            condition: Condition::Always { value: true },
            then: vec![tool("t1")],
            otherwise: vec![tool("t2")],
            unknown: vec![],
        })];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog()).unwrap();
        assert!(!revision.validation.publishable());
        assert!(
            revision
                .validation
                .issues
                .iter()
                .any(|issue| issue.node_id == "c")
        );
    }

    #[test]
    fn loop_scope_is_local_to_the_body() {
        let nodes = vec![
            TaskNode::ForEach(ForEachNode {
                id: "each".into(),
                title: "each".into(),
                items: ValueRef::Literal {
                    value: json!(["a", "b"]),
                },
                item_key: "id".into(),
                nodes: vec![TaskNode::Tool(ToolNode {
                    arguments: json!({"target":{"$ref":{"kind":"item"}}}),
                    ..match tool("inside") {
                        TaskNode::Tool(node) => node,
                        _ => unreachable!(),
                    }
                })],
                max_items: 4,
            }),
            TaskNode::Tool(ToolNode {
                arguments: json!({"target":{"$ref":{"kind":"item"}}}),
                ..match tool("outside") {
                    TaskNode::Tool(node) => node,
                    _ => unreachable!(),
                }
            }),
        ];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog()).unwrap();
        let outside = revision
            .validation
            .issues
            .iter()
            .find(|issue| issue.node_id == "outside")
            .expect("循环项在循环外必须编译失败");
        assert!(outside.message.contains("循环项"));
    }

    #[test]
    fn non_idempotent_writes_cannot_retry() {
        let mut catalog = ToolCatalog::new();
        catalog.insert(
            "demo.write",
            ToolContract {
                execution: ToolExecution::default(),
                provider_version: None,
                input_schema: json!({"type":"object"}),
            },
        );
        let nodes = vec![TaskNode::Tool(ToolNode {
            on_failure: FailurePolicy::BoundedRetry {
                max_attempts: 3,
                backoff_ms: 100,
            },
            tool: Some("demo.write".into()),
            ..match tool("w") {
                TaskNode::Tool(node) => node,
                _ => unreachable!(),
            }
        })];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog).unwrap();
        assert!(
            revision
                .validation
                .issues
                .iter()
                .any(|issue| issue.message.contains("幂等"))
        );
    }

    #[test]
    fn zero_token_requires_every_tool_to_declare_none() {
        let mut catalog = catalog();
        catalog.insert(
            "demo.llm",
            ToolContract {
                execution: ToolExecution {
                    model_usage: ModelUsage::Possible,
                    ..ToolExecution::read_only()
                },
                provider_version: None,
                input_schema: json!({"type":"object"}),
            },
        );
        let nodes = vec![TaskNode::Tool(ToolNode {
            tool: Some("demo.llm".into()),
            ..match tool("l") {
                TaskNode::Tool(node) => node,
                _ => unreachable!(),
            }
        })];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog).unwrap();
        assert!(!revision.model_usage.is_deterministic());
    }

    /// 未声明 modelUsage 的工具保持 unknown，不能被包装成零 token。
    #[test]
    fn undeclared_tool_usage_stays_unknown() {
        let mut catalog = ToolCatalog::new();
        catalog.insert(
            "demo.opaque",
            ToolContract {
                execution: ToolExecution::default(),
                provider_version: None,
                input_schema: json!({"type":"object"}),
            },
        );
        let nodes = vec![TaskNode::Tool(ToolNode {
            tool: Some("demo.opaque".into()),
            ..match tool("o") {
                TaskNode::Tool(node) => node,
                _ => unreachable!(),
            }
        })];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog).unwrap();
        assert_eq!(revision.model_usage, ModelUsage::Unknown);
        assert!(!revision.model_usage.is_deterministic());
    }

    /// 编译把契约写进修订，运行期不再依赖当时的工具目录。
    #[test]
    fn compile_snapshots_the_tool_contract() {
        let nodes = vec![
            tool("a"),
            TaskNode::Tool(ToolNode {
                provider_version: Some("1.0.0".into()),
                ..match tool("b") {
                    TaskNode::Tool(node) => node,
                    _ => unreachable!(),
                }
            }),
        ];
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog()).unwrap();
        let TaskNode::Tool(first) = &revision.nodes[0] else {
            panic!("应为工具节点");
        };
        assert!(first.execution.is_some());
        assert!(revision.validation.publishable());
    }

    /// 契约版本变化必须报出来，而不是让任务悄悄跑在新语义上。
    #[test]
    fn stale_provider_version_is_reported() {
        let nodes = vec![TaskNode::Tool(ToolNode {
            provider_version: Some("0.9.0".into()),
            ..match tool("a") {
                TaskNode::Tool(node) => node,
                _ => unreachable!(),
            }
        })];
        let mut catalog = catalog();
        catalog.insert(
            "demo.read",
            ToolContract {
                execution: ToolExecution::read_only(),
                provider_version: Some("1.0.0".into()),
                input_schema: json!({"type":"object"}),
            },
        );
        let revision = compile("task", 1, "任务", "", nodes, None, &catalog).unwrap();
        assert!(
            revision
                .validation
                .issues
                .iter()
                .any(|issue| issue.message.contains("版本"))
        );
    }

    #[test]
    fn template_renders_only_declared_paths_and_reports_missing() {
        let mut scope = Scope::default();
        scope.record("a", json!({"count": 3}));
        let (rendered, missing) =
            render_template("共 {{ nodes.a.count }} 项，{{ nodes.b.x }}", &scope);
        assert_eq!(rendered, "共 3 项，");
        assert_eq!(missing, vec!["nodes.b.x"]);
    }

    #[test]
    fn expressions_are_paths_not_code() {
        assert_eq!(parse_expression("1+1"), None);
        assert_eq!(parse_expression("globalThis.x"), None);
        assert_eq!(
            parse_expression("nodes.a.output.b"),
            Some(ValueRef::NodeOutput {
                node: "a".into(),
                path: vec!["b".into()]
            })
        );
    }
}
