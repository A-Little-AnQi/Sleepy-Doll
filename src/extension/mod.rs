//! 工具契约与扩展：工具注册表，以及技能、插件、MCP 三类扩展来源。

pub mod mcp;
pub mod plugins;
pub mod skills;

use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{Error, Result};

const fn default_result_limit() -> usize {
    12_000
}

/// Execution semantics are part of the tool contract. Unknown effects are
/// deliberately scheduled like writes until a trusted manifest says otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolEffect {
    ReadOnly,
    InternalState,
    LocalWrite,
    GameWrite,
    ExternalWrite,
    #[serde(alias = "write")]
    Write,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    Observe,
    Low,
    #[default]
    Standard,
    High,
    Irreversible,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConcurrencyMode {
    ReadParallel,
    ResourceExclusive,
    GameExclusive,
    #[default]
    Serial,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IdempotencyMode {
    SafeRetry,
    OriginalKeyOnly,
    #[default]
    NeverRetry,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancellationMode {
    Reliable,
    Cooperative,
    #[default]
    Unconfirmed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VerificationMode {
    None,
    State,
    Job,
    Plugin,
    Human,
    #[default]
    Required,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompensationMode {
    None,
    SnapshotRestore,
    Plugin,
    #[default]
    Required,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnattendedPolicy {
    Allowed,
    #[default]
    Forbidden,
}

/// 一次写入的实际影响从哪里取值。权限判定按影响而不是工具名，但引擎自己读不到
/// 领域语义，所以由契约声明。缺省 `unknown` 表示无法界定，引擎不猜。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScopeKind {
    #[default]
    Unknown,
    /// 逐个配置叶字段：比较目标资源的当前内容与新内容。
    Fields,
    /// 按对象计数：`scopeTarget` 指向的参数是对象 ID 列表。
    Objects,
    /// 删除用户内容。
    Delete,
    /// 整份替换或重置。
    Whole,
}

/// 工具是否可能调用语言模型。零 token 承诺只对 `none` 成立：会调用模型的 MCP
/// 工具、自由脚本和无法判断的工具都不能伪装成零 token。缺省是 `unknown`，
/// 声明为零 token 必须由可信契约给出，不由工具自述。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelUsage {
    None,
    Possible,
    #[default]
    Unknown,
}

impl ModelUsage {
    pub fn is_deterministic(self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ToolExecution {
    pub effect: ToolEffect,
    pub concurrency_safe: bool,
    pub risk: RiskLevel,
    pub concurrency: ConcurrencyMode,
    pub lease_scope: Option<String>,
    pub idempotency: IdempotencyMode,
    pub cancellation: CancellationMode,
    pub verification: VerificationMode,
    pub compensation: CompensationMode,
    pub timeout_ms: u64,
    pub unattended: UnattendedPolicy,
    pub max_result_chars: usize,
    pub deferred: bool,
    pub always_load: bool,
    pub search_hint: Option<String>,
    pub model_usage: ModelUsage,
    pub scope: ScopeKind,
    /// 受影响目标所在的参数名。
    #[serde(default)]
    pub scope_target: Option<String>,
    /// 读取目标当前内容所用的只读工具；与 `Fields` 搭配。
    #[serde(default)]
    pub scope_reader: Option<String>,
}

impl Default for ToolExecution {
    fn default() -> Self {
        Self {
            effect: ToolEffect::Unknown,
            concurrency_safe: false,
            risk: RiskLevel::Standard,
            concurrency: ConcurrencyMode::Serial,
            lease_scope: None,
            idempotency: IdempotencyMode::NeverRetry,
            cancellation: CancellationMode::Unconfirmed,
            verification: VerificationMode::Required,
            compensation: CompensationMode::Required,
            timeout_ms: 30_000,
            unattended: UnattendedPolicy::Forbidden,
            max_result_chars: default_result_limit(),
            deferred: true,
            always_load: false,
            search_hint: None,
            model_usage: ModelUsage::Unknown,
            scope: ScopeKind::Unknown,
            scope_target: None,
            scope_reader: None,
        }
    }
}

impl ToolExecution {
    pub fn read_only() -> Self {
        Self {
            effect: ToolEffect::ReadOnly,
            concurrency_safe: true,
            risk: RiskLevel::Observe,
            concurrency: ConcurrencyMode::ReadParallel,
            idempotency: IdempotencyMode::SafeRetry,
            cancellation: CancellationMode::Reliable,
            verification: VerificationMode::None,
            compensation: CompensationMode::None,
            unattended: UnattendedPolicy::Allowed,
            // 内置只读工具都在本机完成，不经过模型网关。
            model_usage: ModelUsage::None,
            ..Self::default()
        }
    }

    pub fn can_run_concurrently(&self) -> bool {
        self.effect == ToolEffect::ReadOnly
            && self.concurrency_safe
            && matches!(
                self.concurrency,
                ConcurrencyMode::ReadParallel | ConcurrencyMode::Serial
            )
    }

    pub fn validate(&self) -> Result<()> {
        if self.max_result_chars < 256 || self.max_result_chars > 1_000_000 {
            return Err(Error::Config(
                "工具结果上限必须在 256 到 1000000 字符之间".into(),
            ));
        }
        if self.concurrency_safe && self.effect != ToolEffect::ReadOnly {
            return Err(Error::Config("只有明确只读的工具可以声明并发安全".into()));
        }
        if self.concurrency == ConcurrencyMode::ReadParallel && self.effect != ToolEffect::ReadOnly
        {
            return Err(Error::Config("只有只读工具可以声明 readParallel".into()));
        }
        if self.timeout_ms == 0 || self.timeout_ms > 86_400_000 {
            return Err(Error::Config("工具超时必须在 1 毫秒到 24 小时之间".into()));
        }
        if self.risk == RiskLevel::Irreversible && self.unattended == UnattendedPolicy::Allowed {
            return Err(Error::Config("不可逆工具不能用于无人值守运行".into()));
        }
        if self.always_load && self.deferred {
            return Err(Error::Config(
                "工具不能同时声明 alwaysLoad 与 deferred".into(),
            ));
        }
        if self
            .search_hint
            .as_ref()
            .is_some_and(|hint| hint.trim().is_empty() || hint.len() > 160)
        {
            return Err(Error::Config("工具 searchHint 无效".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_version: Option<String>,
    #[serde(default)]
    pub execution: ToolExecution,
}

pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    fn call(&self, arguments: &Value) -> Result<Value>;
    fn call_async(
        self: Arc<Self>,
        arguments: Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send>>
    where
        Self: 'static,
    {
        Box::pin(async move {
            tokio::select! {
                _ = cancel.cancelled()=>Err(Error::Conflict("工具停止状态尚未确认".into())),
                result=tokio::task::spawn_blocking(move||self.call(&arguments))=>result.map_err(|_|Error::Tool("工具进程异常退出".into()))?,
            }
        })
    }
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub async fn call_async(
        &self,
        name: &str,
        args: &Value,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| Error::Tool("工具未注册".into()))?
            .clone();
        let definition = tool.definition();
        if !validate(args, &definition.input_schema, "$").is_empty() {
            return Err(Error::Tool("工具参数不符合契约".into()));
        }
        let value = tool.call_async(args.clone(), cancel).await?;
        validate_output(&value, definition.output_schema.as_ref())?;
        Ok(value)
    }
    pub fn register<T: Tool + 'static>(&mut self, tool: T) -> Result<()> {
        let definition = tool.definition();
        definition.execution.validate()?;
        validate_definition(&definition)?;
        let name = definition.name;
        if !valid_name(&name) {
            return Err(Error::Tool(format!("invalid tool name: {name}")));
        }
        if self.tools.contains_key(&name) {
            return Err(Error::Tool(format!("duplicate tool: {name}")));
        }
        self.tools.insert(name, Arc::new(tool));
        Ok(())
    }

    pub fn register_shared(&mut self, tool: Arc<dyn Tool>) -> Result<()> {
        let definition = tool.definition();
        definition.execution.validate()?;
        validate_definition(&definition)?;
        let name = definition.name;
        if !valid_name(&name) {
            return Err(Error::Tool(format!("invalid tool name: {name}")));
        }
        if self.tools.contains_key(&name) {
            return Err(Error::Tool(format!("duplicate tool: {name}")));
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let mut result = self
            .tools
            .values()
            .map(|tool| tool.definition())
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.name.cmp(&right.name));
        result
    }

    pub fn call(&self, name: &str, arguments: &Value) -> Value {
        let Some(tool) = self.tools.get(name) else {
            return json!({"ok":false,"error":{"code":"TOOL_NOT_FOUND","message":format!("Unknown tool: {name}")}});
        };
        let definition = tool.definition();
        let issues = validate(arguments, &definition.input_schema, "$");
        if !issues.is_empty() {
            return json!({"ok":false,"error":{"code":"INVALID_ARGUMENT","message":"Tool arguments failed validation","details":issues}});
        }
        match tool.call(arguments).and_then(|value| {
            validate_output(&value, definition.output_schema.as_ref())?;
            Ok(value)
        }) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(error) => {
                json!({"ok":false,"error":{"code":"TOOL_EXECUTION_FAILED","message":error.to_string()}})
            }
        }
    }
}

fn validate_definition(definition: &ToolDefinition) -> Result<()> {
    jsonschema::validator_for(&definition.input_schema)
        .map_err(|_| Error::Config("工具输入 Schema 无效".into()))?;
    if let Some(schema) = &definition.output_schema {
        jsonschema::validator_for(schema)
            .map_err(|_| Error::Config("工具输出 Schema 无效".into()))?;
    }
    Ok(())
}

fn validate_output(value: &Value, schema: Option<&Value>) -> Result<()> {
    if schema.is_some_and(|schema| !validate(value, schema, "$").is_empty()) {
        return Err(Error::Tool("工具返回结果不符合输出契约".into()));
    }
    Ok(())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.chars().enumerate().all(|(index, character)| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') && index > 0
        })
}

pub(crate) fn validate(value: &Value, schema: &Value, path: &str) -> Vec<Value> {
    match jsonschema::validator_for(schema) {
        Ok(validator) => validator
            .iter_errors(value)
            .take(32)
            .map(|e| json!({"path":format!("{path}{}",e.instance_path),"message":e.to_string()}))
            .collect(),
        Err(error) => vec![json!({"path":path,"message":format!("invalid tool schema: {error}")})],
    }
}

pub struct FunctionTool<F>
where
    F: Fn(&Value) -> Result<Value> + Send + Sync,
{
    definition: ToolDefinition,
    function: F,
}

impl<F> FunctionTool<F>
where
    F: Fn(&Value) -> Result<Value> + Send + Sync,
{
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        source: impl Into<String>,
        function: F,
    ) -> Self {
        Self {
            definition: ToolDefinition {
                name: name.into(),
                description: description.into(),
                input_schema,
                output_schema: None,
                source: source.into(),
                provider_version: None,
                execution: ToolExecution::default(),
            },
            function,
        }
    }

    pub fn with_execution(mut self, execution: ToolExecution) -> Self {
        self.definition.execution = execution;
        self
    }

    pub fn with_provider_version(mut self, version: Option<String>) -> Self {
        self.definition.provider_version = version;
        self
    }

    pub fn with_output_schema(mut self, output_schema: Option<Value>) -> Self {
        self.definition.output_schema = output_schema;
        self
    }
}

impl<F> Tool for FunctionTool<F>
where
    F: Fn(&Value) -> Result<Value> + Send + Sync,
{
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }
    fn call(&self, arguments: &Value) -> Result<Value> {
        (self.function)(arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_unknown_arguments() {
        let issues = validate(
            &json!({"extra":1}),
            &json!({"type":"object","properties":{},"additionalProperties":false}),
            "$",
        );
        assert_eq!(issues.len(), 1);
    }

    #[test]
    fn tool_execution_defaults_fail_closed() {
        let execution = ToolExecution::default();
        assert_eq!(execution.effect, ToolEffect::Unknown);
        assert!(!execution.can_run_concurrently());
    }

    #[test]
    fn only_explicit_read_tools_can_be_concurrent() {
        let execution = ToolExecution {
            concurrency_safe: true,
            ..ToolExecution::default()
        };
        assert!(execution.validate().is_err());
        assert!(ToolExecution::read_only().validate().is_ok());
        assert!(ToolExecution::read_only().can_run_concurrently());
    }

    #[test]
    fn output_schema_is_checked_before_a_result_is_returned() {
        let mut registry = ToolRegistry::default();
        registry
            .register(
                FunctionTool::new(
                    "test.output",
                    "test",
                    json!({"type":"object"}),
                    "test",
                    |_| Ok(json!({"value":"wrong"})),
                )
                .with_output_schema(Some(json!({
                    "type":"object",
                    "properties":{"value":{"type":"integer"}},
                    "required":["value"]
                }))),
            )
            .unwrap();
        assert_eq!(registry.call("test.output", &json!({}))["ok"], false);
    }
}
