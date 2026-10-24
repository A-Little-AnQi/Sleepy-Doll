//! 辅助决策通过隔离的 tool call 返回；不注册到业务工具表，也不执行其函数名。
use super::*;

fn tool(name: &str, description: &str, schema: Value) -> ToolDefinition {
    ToolDefinition {
        name: name.into(),
        label: description.into(),
        description: description.into(),
        input_schema: schema,
        output_schema: None,
        source: "core:decision".into(),
        provider_version: None,
        execution: ToolExecution {
            effect: ToolEffect::InternalState,
            ..ToolExecution::default()
        },
    }
}

pub(super) fn web_extract() -> ToolDefinition {
    tool(
        "runtime.web_extract",
        "提交与本次问题相关的网页事实及原文行号；不执行业务动作",
        json!({
            "type":"object","additionalProperties":false,"required":["summary","relevant","ranges"],
            "properties":{"summary":{"type":"string","maxLength":12000},"relevant":{"type":"boolean"},
            "ranges":{"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,
                "required":["startLine","endLine"],"properties":{"startLine":{"type":"integer","minimum":1},"endLine":{"type":"integer","minimum":1}}}}}
        }),
    )
}

pub(super) fn memory(
    tools: &std::collections::BTreeSet<String>,
    methods: &std::collections::BTreeSet<String>,
) -> ToolDefinition {
    let strings = |max| json!({"type":"array","maxItems":max,"items":{"type":"string"}});
    let method_ids = methods
        .iter()
        .map(|id| json!(id))
        .chain(std::iter::once(Value::Null))
        .collect::<Vec<_>>();
    let step = json!({"type":"object","additionalProperties":false,"required":["tool","action"],
        "properties":{"tool":{"type":"string","enum":tools},"methodId":{"type":["string","null"],"enum":method_ids},
        "action":{"type":"string","minLength":1},"inputRoles":{"type":"object","additionalProperties":{"type":"string"}}}});
    let experience = json!({"type":"object","additionalProperties":false,"required":["intent","summary","conditions","steps","verification"],
        "properties":{"intent":{"type":"string","minLength":1,"maxLength":160},"summary":{"type":"string","minLength":1,"maxLength":240},
        "keywords":strings(12),"conditions":strings(8),"verification":strings(8),"pitfalls":strings(8),"anchors":strings(6),
        "steps":{"type":"array","minItems":1,"maxItems":6,"items":step}}});
    tool(
        "runtime.memory_experiences",
        "提交与真实用户目标相关的经验；没有实质增量时提交空 experiences",
        json!({
            "type":"object","additionalProperties":false,"required":["goalRelevant","experiences"],
            "properties":{"goalRelevant":{"type":"boolean"},"experiences":{"type":"array","maxItems":2,"items":experience}}
        }),
    )
}

pub(super) fn validate(
    response: &crate::model::ModelResponse,
    definition: &ToolDefinition,
) -> Result<()> {
    if gateway::output_truncated(response.finish_reason.as_deref())
        || response.tool_calls.len() != 1
    {
        return Err(Error::ModelProtocol(
            "辅助决策必须返回一个完整的专用工具调用，未接受文字 JSON 或多个调用".into(),
        ));
    }
    let call = &response.tool_calls[0];
    if call.name != definition.name
        || !crate::extension::validate(&call.arguments, &definition.input_schema, "$").is_empty()
    {
        return Err(Error::ModelProtocol(
            "辅助决策工具名称或参数不符合契约，未执行后续操作".into(),
        ));
    }
    Ok(())
}
