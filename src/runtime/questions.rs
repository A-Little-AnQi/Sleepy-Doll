use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 结构化问答的集中定义：schema 归一、边界校验与答复校验都在这里，
/// runtime 与 journal 只消费结果，不各自解析参数。
///
/// 形状对齐 Codex 的 request_user_input：题目带 id/header/question 与可选
/// options（label+description），答复按题目 id 回填字符串数组。
const MAX_QUESTIONS: usize = 3;
const MAX_ID: usize = 64;
const MAX_HEADER: usize = 120;
const MAX_QUESTION: usize = 2000;
const MAX_OPTIONS: usize = 8;
const MAX_LABEL: usize = 120;
const MAX_DESCRIPTION: usize = 500;
const MAX_ANSWER_ENTRIES: usize = 8;
const MAX_ANSWER: usize = 4000;

/// 旧版 question:string 归一后的题目 id；答复按它回填，兼容旧 answer 字段。
pub const LEGACY_QUESTION_ID: &str = "answer";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuestionOption {
    pub label: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Question {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub options: Vec<QuestionOption>,
}

/// 归一后的完整请求。requestId 使用真实的工具调用 id。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequest {
    pub request_id: String,
    pub questions: Vec<Question>,
}

fn bounded(value: &str, limit: usize, what: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::Tool(format!("{what}不能为空")));
    }
    // 按字符计数，与 schema 提示一致，中文与多字节字符不受字节长度影响。
    if trimmed.chars().count() > limit {
        return Err(Error::Tool(format!("{what}超过 {limit} 字符上限")));
    }
    Ok(trimmed.to_owned())
}

/// 把 user.ask 参数归一为结构化请求；旧 question:string 保持兼容。
/// 返回 (题目列表, 旧格式纯文本)，后者用于 question 事件的兼容字段。
pub fn normalize(arguments: &Value) -> Result<(Vec<Question>, String)> {
    if let Some(list) = arguments["questions"].as_array() {
        if !arguments["question"]
            .as_str()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(Error::Tool("questions 与 question 只能二选一".into()));
        }
        if list.is_empty() || list.len() > MAX_QUESTIONS {
            return Err(Error::Tool(format!(
                "questions 需要 1 到 {MAX_QUESTIONS} 个问题"
            )));
        }
        let mut questions = Vec::new();
        let mut ids = std::collections::HashSet::new();
        for (index, item) in list.iter().enumerate() {
            let id = bounded(
                item["id"].as_str().unwrap_or(""),
                MAX_ID,
                &format!("第 {} 题的 id", index + 1),
            )?;
            if !ids.insert(id.clone()) {
                return Err(Error::Tool(format!("问题 id「{id}」重复")));
            }
            if id == LEGACY_QUESTION_ID {
                return Err(Error::Tool("问题 id 不能使用保留标识 answer".into()));
            }
            let header = bounded(
                item["header"].as_str().unwrap_or(""),
                MAX_HEADER,
                &format!("问题「{id}」的标题"),
            )?;
            let question = bounded(
                item["question"].as_str().unwrap_or(""),
                MAX_QUESTION,
                &format!("问题「{id}」的内容"),
            )?;
            let options = if item["options"].is_null() {
                Vec::new()
            } else {
                let options = item["options"]
                    .as_array()
                    .ok_or_else(|| Error::Tool(format!("问题「{id}」的 options 必须是数组")))?;
                if options.len() > MAX_OPTIONS {
                    return Err(Error::Tool(format!(
                        "问题「{id}」的选项超过 {MAX_OPTIONS} 个"
                    )));
                }
                let mut labels = std::collections::HashSet::new();
                options
                    .iter()
                    .map(|option| {
                        if !option.is_object() {
                            return Err(Error::Tool(format!("问题「{id}」的选项必须是对象")));
                        }
                        let label = bounded(
                            option["label"].as_str().unwrap_or(""),
                            MAX_LABEL,
                            &format!("问题「{id}」的选项名"),
                        )?;
                        if !labels.insert(label.clone()) {
                            return Err(Error::Tool(format!("问题「{id}」的选项「{label}」重复")));
                        }
                        // description 可省略；给了就必须是字符串且不超长，不静默截断。
                        let description = if option["description"].is_null() {
                            String::new()
                        } else {
                            bounded(
                                option["description"].as_str().unwrap_or(""),
                                MAX_DESCRIPTION,
                                &format!("问题「{id}」的选项说明"),
                            )?
                        };
                        Ok(QuestionOption { description, label })
                    })
                    .collect::<Result<Vec<_>>>()?
            };
            questions.push(Question {
                id,
                header,
                question,
                options,
            });
        }
        let legacy = questions
            .iter()
            .map(|q| format!("**{}**\n\n{}", q.header, q.question))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok((questions, legacy));
    }
    // 旧格式：单个 Markdown 问题。
    let text = bounded(
        arguments["question"].as_str().unwrap_or(""),
        MAX_QUESTION,
        "question",
    )?;
    Ok((
        vec![Question {
            id: LEGACY_QUESTION_ID.into(),
            header: "需要补充信息".into(),
            question: text.clone(),
            options: Vec::new(),
        }],
        text,
    ))
}

pub fn request(request_id: &str, questions: &[Question]) -> Value {
    serde_json::to_value(QuestionRequest {
        request_id: request_id.to_owned(),
        questions: questions.to_vec(),
    })
    .expect("serializable question request")
}

/// question 事件数据：结构化 request 之外保留旧 question 文本字段，
/// 旧前端与事件重放仍然能渲染纯文本卡片。
pub fn event_data(request: &Value, legacy_text: &str) -> Value {
    json!({"question": legacy_text, "request": request})
}

/// 校验并归一答复：键必须恰好覆盖全部题目（每题必答），值必须是
/// 非空字符串数组。返回规范化的 answers 对象。
pub fn validate_answers(questions: &[Question], answers: &Value) -> Result<Value> {
    let map = answers
        .as_object()
        .ok_or_else(|| Error::Config("answers 必须是对象".into()))?;
    let mut normalized = serde_json::Map::new();
    for question in questions {
        let entry = map
            .get(&question.id)
            .ok_or_else(|| Error::Config(format!("问题「{}」还没有回答", question.id)))?;
        let entries = entry["answers"]
            .as_array()
            .ok_or_else(|| Error::Config(format!("问题「{}」的答复必须是数组", question.id)))?;
        if entries.is_empty() || entries.len() > MAX_ANSWER_ENTRIES {
            return Err(Error::Config(format!(
                "问题「{}」的答复需要 1 到 {MAX_ANSWER_ENTRIES} 条",
                question.id
            )));
        }
        let mut cleaned = Vec::new();
        for entry in entries {
            let text = bounded(
                entry.as_str().unwrap_or(""),
                MAX_ANSWER,
                &format!("问题「{}」的答复", question.id),
            )?;
            cleaned.push(text);
        }
        normalized.insert(question.id.clone(), json!({"answers": cleaned}));
    }
    let known: std::collections::HashSet<&str> = questions.iter().map(|q| q.id.as_str()).collect();
    for key in map.keys() {
        if !known.contains(key.as_str()) {
            return Err(Error::Config(format!("未知的问题 id「{key}」")));
        }
    }
    Ok(Value::Object(normalized))
}

/// 工具返回值：结构化 answers；旧格式额外带兼容 answer 字段。
pub fn answer_result(request: &Value, answers: &Value) -> Value {
    let questions: Vec<Question> =
        serde_json::from_value(request["questions"].clone()).expect("stored questions");
    let mut result = json!({
        "requestId": request["requestId"],
        "answers": answers,
    });
    if questions.len() == 1 && questions[0].id == LEGACY_QUESTION_ID {
        let text = answers[LEGACY_QUESTION_ID]["answers"][0]
            .as_str()
            .unwrap_or("")
            .to_owned();
        result["answer"] = json!(text);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structured() -> Value {
        json!({"questions":[
            {"id":"mode","header":"运行方式","question":"现在启动还是只做检查？",
             "options":[{"label":"启动","description":"立即运行"},{"label":"检查"}]},
            {"id":"name","header":"入口名称","question":"入口叫什么名字？"}
        ]})
    }

    #[test]
    fn normalizes_structured_and_legacy() {
        let (questions, legacy) = normalize(&structured()).unwrap();
        assert_eq!(questions.len(), 2);
        assert_eq!(questions[0].options.len(), 2);
        assert!(legacy.contains("运行方式"));
        let (questions, legacy) = normalize(&json!({"question":"要用哪个配置组？"})).unwrap();
        assert_eq!(questions[0].id, LEGACY_QUESTION_ID);
        assert_eq!(legacy, "要用哪个配置组？");
    }

    #[test]
    fn rejects_invalid_questions() {
        for invalid in [
            json!({}),
            json!({"questions":[]}),
            json!({"questions":[{"id":"","header":"h","question":"q"}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q"},{"id":"a","header":"h","question":"q"}]}),
            json!({"questions":[{"id":"answer","header":"h","question":"q"}]}),
            json!({"questions":[{"id":"a","header":" ","question":"q"}]}),
            json!({"questions":[{"id":"a","header":"h","question":" "}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q","options":[{"label":""}]}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q","options":[{"label":"x"},{"label":"x"}]}]}),
            json!({"question":"旧","questions":[{"id":"a","header":"h","question":"q"}]}),
        ] {
            assert!(normalize(&invalid).is_err(), "应拒绝: {invalid}");
        }
        let long = "x".repeat(MAX_ID + 1);
        assert!(
            normalize(&json!({"questions":[{"id":long,"header":"h","question":"q"}]})).is_err()
        );
        // 非法 options 类型：字符串、数字、非对象元素、非字符串说明。
        for bad_options in [
            json!({"questions":[{"id":"a","header":"h","question":"q","options":"启动"}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q","options":3}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q","options":["启动"]}]}),
            json!({"questions":[{"id":"a","header":"h","question":"q","options":[{"label":"x","description":7}]}]}),
        ] {
            assert!(normalize(&bad_options).is_err(), "应拒绝: {bad_options}");
        }
        // 中文字符按字计数：64 个中文 id 合法，不按字节误判。
        let chinese_id = "题".repeat(64);
        let (questions, _) =
            normalize(&json!({"questions":[{"id":chinese_id,"header":"标","question":"问"}]}))
                .unwrap();
        assert_eq!(questions[0].id.chars().count(), 64);
        // 字符数超长拒绝：question 与选项说明。
        let long_question = "问".repeat(MAX_QUESTION + 1);
        assert!(
            normalize(&json!({"questions":[{"id":"a","header":"h","question":long_question}]}))
                .is_err()
        );
        let long_description = "说".repeat(MAX_DESCRIPTION + 1);
        assert!(normalize(
            &json!({"questions":[{"id":"a","header":"h","question":"q","options":[{"label":"x","description":long_description}]}]})
        )
        .is_err());
    }

    #[test]
    fn validates_and_normalizes_answers() {
        let (questions, _) = normalize(&structured()).unwrap();
        let answers = validate_answers(
            &questions,
            &json!({"mode":{"answers":["启动"]},"name":{"answers":["日常入口","备用"]}}),
        )
        .unwrap();
        assert_eq!(answers["name"]["answers"][1], json!("备用"));
        // 缺题、未知题、空答复、非数组都拒绝。
        for invalid in [
            json!({"mode":{"answers":["启动"]}}),
            json!({"mode":{"answers":["启动"]},"name":{"answers":[]},"extra":{}}),
            json!({"mode":{"answers":["启动"]},"name":{"answers":[" "]},"unknown":{"answers":["x"]}}),
            json!({"mode":{"answers":["启动"]},"name":"日常入口"}),
        ] {
            assert!(
                validate_answers(&questions, &invalid).is_err(),
                "应拒绝: {invalid}"
            );
        }
    }

    #[test]
    fn answer_result_keeps_legacy_field() {
        let (questions, _) = normalize(&json!({"question":"用哪个？"})).unwrap();
        let legacy_request = request("call-1", &questions);
        let answers = validate_answers(&questions, &json!({"answer":{"answers":["甲"]}})).unwrap();
        let result = answer_result(&legacy_request, &answers);
        assert_eq!(result["answer"], json!("甲"));
        assert_eq!(result["requestId"], json!("call-1"));
        let (structured_questions, _) = normalize(&structured()).unwrap();
        let structured_request = request("call-2", &structured_questions);
        let structured_answers = validate_answers(
            &structured_questions,
            &json!({"mode":{"answers":["启动"]},"name":{"answers":["n"]}}),
        )
        .unwrap();
        let structured_result = answer_result(&structured_request, &structured_answers);
        assert!(structured_result.get("answer").is_none());
        assert_eq!(
            structured_result["answers"]["mode"]["answers"][0],
            json!("启动")
        );
    }
}
