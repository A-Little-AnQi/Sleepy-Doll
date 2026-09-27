//! 通用快捷任务的公开定义协议，供模型按需读取；不包含领域接口或业务步骤。
use serde_json::{Value, json};

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/$defs/{name}")})
}
fn array(name: &str) -> Value {
    json!({"type":"array","items":reference(name)})
}
fn tagged(kind: &str, fields: Value, required: &[&str]) -> Value {
    let mut fields = fields.as_object().unwrap().clone();
    fields.insert("kind".into(), json!({"const":kind}));
    let mut required = required.to_vec();
    required.push("kind");
    object(Value::Object(fields), &required)
}
fn node(kind: &str, extra: Value, required: &[&str]) -> Value {
    let mut fields = extra.as_object().unwrap().clone();
    fields.insert("id".into(), json!({"type":"string","minLength":1}));
    fields.insert("title".into(), json!({"type":"string","minLength":1}));
    let mut required = required.to_vec();
    required.extend(["id", "title"]);
    tagged(kind, Value::Object(fields), &required)
}

pub fn save_schema() -> Value {
    let tool = json!({
        "id":{"type":"string","minLength":1},"title":{"type":"string","minLength":1},
        "tool":{"type":"string","description":"已发现的真实工具名；插件内部 methodId 不能当作工具名。"},
        "capabilityId":{"type":["string","null"]},"arguments":{"type":"object"},
        "execution":{"type":["object","null"]},"providerVersion":{"type":["string","null"]},
        "resourceVersions":{"type":"array","items":{"type":"string"}},
        "onFailure":reference("failure"),"onUnverified":reference("failure")
    });
    let refs = json!({"oneOf":[
        tagged("literal",json!({"value":{}}), &["value"]),
        tagged("nodeOutput",json!({"node":{"type":"string"},"path":{"type":"array","items":{"type":"string"}}}), &["node"]),
        tagged("item",json!({"path":{"type":"array","items":{"type":"string"}}}), &[]),
        tagged("iteration",json!({}), &[])
    ]});
    let conditions = json!({"oneOf":[
        tagged("always",json!({"value":{"type":"boolean"}}), &["value"]),
        tagged("exists",json!({"value":reference("valueRef")}), &["value"]),
        tagged("compare",json!({"op":{"enum":["equals","notEquals","less","lessOrEqual","greater","greaterOrEqual"]},"left":reference("valueRef"),"right":reference("valueRef")}), &["op","left","right"]),
        tagged("all",json!({"conditions":array("condition")}), &["conditions"]),
        tagged("any",json!({"conditions":array("condition")}), &["conditions"]),
        tagged("not",json!({"condition":reference("condition")}), &["condition"])
    ]});
    let policies = json!({"oneOf":[
        tagged("stop",json!({}), &[]), tagged("continueIndependent",json!({}), &[]),
        tagged("boundedRetry",json!({"maxAttempts":{"type":"integer","minimum":1},"backoffMs":{"type":"integer","minimum":0}}), &["maxAttempts","backoffMs"]),
        tagged("compensate",json!({"tool":{"type":["string","null"]},"arguments":{}}), &[])
    ]});
    let nodes = json!({"oneOf":[
        node("tool",tool.clone(), &["arguments"]),
        node("sequence",json!({"nodes":array("node")}), &[]),
        node("condition",json!({"condition":reference("condition"),"then":array("node"),"otherwise":array("node"),"unknown":array("node")}), &["condition","unknown"]),
        node("forEach",json!({"items":reference("valueRef"),"itemKey":{"type":"string"},"nodes":array("node"),"maxItems":{"type":"integer","minimum":1}}), &["items","itemKey","nodes"]),
        node("repeat",json!({"count":{"type":["integer","null"],"minimum":1},"until":{"anyOf":[reference("condition"),{"type":"null"}]},"nodes":array("node"),"maxIterations":{"type":"integer","minimum":1}}), &["nodes"]),
        node("wait",json!({"seconds":{"type":["integer","null"],"minimum":1},"until":{"anyOf":[reference("condition"),{"type":"null"}]},"probe":{"anyOf":[reference("probe"),{"type":"null"}]},"checkSeconds":{"type":"integer","minimum":1},"timeoutSeconds":{"type":["integer","null"],"minimum":1}}), &[]),
        node("result",json!({"template":{"type":"string"},"outputs":array("valueRef")}), &[])
    ]});
    let mut schema = object(
        json!({
            "id":{"type":"string"},"name":{"type":"string","minLength":1,"maxLength":120},
            "description":{"type":"string"},"nodes":array("node"),"publish":{"type":"boolean"},
            "limits":object(json!({"maxNodes":{"type":"integer","minimum":1},"maxLoopExpansions":{"type":"integer","minimum":1},"maxToolAttempts":{"type":"integer","minimum":1},"maxSeconds":{"type":"integer","minimum":1}}), &["maxNodes","maxLoopExpansions","maxToolAttempts","maxSeconds"])
        }),
        &["name", "description", "nodes"],
    );
    schema["$defs"] = json!({"node":nodes,"probe":object(tool,&["id","title","arguments"]),"condition":conditions,"valueRef":refs,"failure":policies});
    schema
}

pub fn describe(kind: Option<&str>) -> Value {
    let schema = save_schema();
    let selected = kind.and_then(|kind| {
        schema["$defs"]["node"]["oneOf"]
            .as_array()?
            .iter()
            .find(|node| node["properties"]["kind"]["const"] == kind)
            .cloned()
    });
    let mut selected = selected.unwrap_or(schema.clone());
    selected["$defs"] = schema["$defs"].clone();
    json!({"schemaVersion":super::task::SCHEMA_VERSION,"schema": selected,
        "rules":["所有流程节点用 kind，不能用 type。wait.probe 是嵌入的工具对象，没有 kind。","until/condition 必须是条件表达式；读取一个值不等于条件判断。","tool 必须是已登记工具；领域内部接口 ID 放入对应工具的 arguments。","保存仅校验和落库，不执行目标；运行只使用已发布的修订，不调用模型。","参数可用 {\"$ref\":{\"kind\":\"nodeOutput\",\"node\":\"上游ID\",\"path\":[\"字段\"]}} 引用本次输出。"],
        "example": {"name":"读取文件并整理结果","description":"每次读取最新文件，生成结构化结果","nodes":[{"kind":"tool","id":"read","title":"读取本次文件","tool":"workspace.read","arguments":{"path":"example.json"}},{"kind":"result","id":"result","title":"输出本次结果","template":"{{ nodes.read.output.text }}"}]}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension::ToolExecution;
    use crate::runtime::operation::task::*;
    fn catalog() -> ToolCatalog {
        let mut catalog = ToolCatalog::new();
        for name in ["sample.state", "sample.consume"] {
            catalog.insert(name,ToolContract{execution:ToolExecution::read_only(),provider_version:None,input_schema:if name=="sample.consume" { json!({"type":"object","properties":{"value":{"type":"string"},"fixed":{"type":"integer"}},"required":["value","fixed"],"additionalProperties":false}) } else {json!({"type":"object"})}});
        }
        catalog
    }
    #[test]
    fn wait_protocol_and_probe_reference_match_the_executor() {
        let input = json!({"name":"通用等待","description":"不依赖宿主","nodes":[{"kind":"wait","id":"wait","title":"等到就绪","timeoutSeconds":30,"probe":{"id":"probe","title":"检查","tool":"sample.state","arguments":{}},"until":{"kind":"compare","op":"equals","left":{"kind":"nodeOutput","node":"probe","path":["ready"]},"right":{"kind":"literal","value":true}}}]});
        assert!(crate::extension::validate(&input, &save_schema(), "$").is_empty());
        let nodes: Vec<TaskNode> = serde_json::from_value(input["nodes"].clone()).unwrap();
        let revision = compile("task", 1, "通用等待", "", nodes.clone(), None, &catalog()).unwrap();
        assert!(
            revision.validation.publishable(),
            "{:?}",
            revision.validation.issues
        );
        assert_eq!(
            crate::runtime::operation::task_store::tool_names(&nodes),
            vec!["sample.state"]
        );
        let mut wrong = input;
        wrong["nodes"][0]["probe"]["kind"] = json!("tool");
        assert!(!crate::extension::validate(&wrong, &save_schema(), "$").is_empty());
    }
    #[test]
    fn invalid_kind_and_value_reference_as_condition_are_rejected_early() {
        let mut input = json!({"name":"流程","description":"","nodes":[{"type":"wait","id":"wait","title":"等待","until":{"kind":"nodeOutput","node":"probe","path":["ready"]}}]});
        assert!(!crate::extension::validate(&input, &save_schema(), "$").is_empty());
        input["nodes"][0].as_object_mut().unwrap().remove("type");
        input["nodes"][0]["kind"] = json!("wait");
        assert!(!crate::extension::validate(&input, &save_schema(), "$").is_empty());
    }
    #[test]
    fn dynamic_values_are_checked_as_references_not_final_literals() {
        let mut nodes:Vec<TaskNode>=serde_json::from_value(json!([
            {"kind":"tool","id":"read","title":"读取","tool":"sample.state","arguments":{}},
            {"kind":"tool","id":"consume","title":"使用本次值","tool":"sample.consume","arguments":{"value":{"$ref":{"kind":"nodeOutput","node":"read","path":["content"]}},"fixed":1}}
        ])).unwrap();
        assert!(
            compile("task", 1, "动态流程", "", nodes.clone(), None, &catalog())
                .unwrap()
                .validation
                .publishable()
        );
        let TaskNode::Tool(node) = &mut nodes[1] else {
            unreachable!()
        };
        node.arguments["fixed"] = json!("错误类型");
        assert!(
            !compile("task", 1, "动态流程", "", nodes, None, &catalog())
                .unwrap()
                .validation
                .publishable()
        );
    }
    #[test]
    fn repeated_body_outputs_and_iteration_are_available_to_until() {
        let nodes:Vec<TaskNode>=serde_json::from_value(json!([{ "kind":"repeat","id":"repeat","title":"重复读取","maxIterations":3,"nodes":[{"kind":"tool","id":"body","title":"检查本轮","tool":"sample.state","arguments":{}}],"until":{"kind":"compare","op":"equals","left":{"kind":"nodeOutput","node":"body","path":["ready"]},"right":{"kind":"literal","value":true}}}])).unwrap();
        assert!(
            compile("task", 1, "重复流程", "", nodes, None, &catalog())
                .unwrap()
                .validation
                .publishable()
        );
        let mut outer = Scope::default();
        let mut inner = outer.with_item(json!("局部"), 2);
        inner.record("body", json!({"ready":true}));
        outer.retain_outputs_from(&inner);
        assert_eq!(outer.output("body"), Some(&json!({"ready":true})));
        assert_eq!(ValueRef::Item { path: vec![] }.resolve(&outer), None);
        assert_eq!(ValueRef::Iteration.resolve(&outer), None);
    }
}
