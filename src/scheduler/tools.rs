use super::{Scheduler, model::ScheduleInput};
use crate::{
    Error, Result,
    extension::{
        FunctionTool, ToolEffect, ToolExecution, ToolRegistry, mcp_server::RegistryServer,
    },
};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn server(service: Arc<Scheduler>) -> Result<RegistryServer> {
    let mut registry = ToolRegistry::default();
    for (name, label, description, schema, read_only) in definitions() {
        let service = service.clone();
        let method = name.to_owned();
        let execution = if read_only {
            ToolExecution::read_only()
        } else {
            ToolExecution {
                effect: ToolEffect::ExternalWrite,
                model_usage: crate::extension::ModelUsage::None,
                concurrency: crate::extension::ConcurrencyMode::ResourceExclusive,
                lease_scope: Some("scheduled-tasks".into()),
                cancellation: crate::extension::CancellationMode::Reliable,
                ..ToolExecution::default()
            }
        };
        registry.register(
            FunctionTool::new(
                name,
                description,
                schema,
                "plugin:windows-scheduler:local",
                move |args| {
                    dispatch(&service, &method, args)
                        .map_err(|error| Error::Tool(error.user_message()))
                },
            )
            .with_label(label)
            .with_execution(execution),
        )?;
    }
    Ok(RegistryServer::new(
        registry,
        "sleepy-doll-scheduler",
        "处理定时任务前读取 schedule-task 技能，按其中的对话流程和接口引用操作。用 scheduler.clock 读取本机时间，先查真实任务再预览；只追问缺少的用户意图。有序执行项保留重复，单项出错后继续。默认类别的关闭游戏和关闭工具软件自动关联前面最近一个快捷任务的软件。保存不立即运行，返回注册核验成功才能说已设置。只接受时间、已发布任务引用和固定内置动作，禁止任意命令、程序路径、PID、XML、账号和提权参数。",
    ))
}

pub fn dispatch(service: &Scheduler, method: &str, args: &Value) -> Result<Value> {
    let schema = definitions()
        .into_iter()
        .find(|definition| definition.0 == method)
        .ok_or_else(|| Error::Tool("未知定时任务接口".into()))?
        .3;
    let issues = crate::extension::validate(args, &schema, "$");
    if !issues.is_empty() {
        return Err(Error::Config(format!(
            "定时任务参数不符合契约：{}",
            json!(issues)
        )));
    }
    Ok(match method {
        "scheduler.clock" => super::clock::read(args["localDateTime"].as_str())?,
        "scheduler.actions.list" if args["category"] == "defaults" => {
            let query = args["query"].as_str().unwrap_or("").trim();
            let items = [super::model::DefaultAction::CloseGame, super::model::DefaultAction::CloseToolSoftware]
                .into_iter().filter(|action| query.split_whitespace().all(|word| action.title().contains(word)))
                .map(|action| json!({"id":format!("default:{action:?}"),"title":action.title(),"content":"","binding":action.binding(),"available":true}))
                .collect::<Vec<_>>();
            let total = items.len();
            let offset = args["offset"].as_u64().unwrap_or(0) as usize;
            let limit = args["limit"].as_u64().unwrap_or(20) as usize;
            let items = items
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect::<Vec<_>>();
            json!({"total":total,"items":items,"nextOffset":(offset.saturating_add(limit)<total).then_some(offset.saturating_add(limit))})
        }
        "scheduler.actions.list" => service.actions(
            args["query"].as_str().unwrap_or(""),
            args["offset"].as_u64().unwrap_or(0) as usize,
            args["limit"].as_u64().unwrap_or(20) as usize,
        )?,
        "scheduler.list" => {
            json!({"items":service.store.list()?.iter().map(|schedule|schedule.view()).collect::<Vec<_>>()})
        }
        "scheduler.get" => service.store.get(required(args, "id")?)?.view(),
        "scheduler.preview" => {
            service.preview(&serde_json::from_value::<ScheduleInput>(args.clone())?)?
        }
        "scheduler.create" => service
            .create(serde_json::from_value(args.clone())?)?
            .view(),
        "scheduler.update" => service
            .update(
                required(args, "id")?,
                revision(args)?,
                serde_json::from_value(args["schedule"].clone())?,
            )?
            .view(),
        "scheduler.set_enabled" => service
            .set_enabled(
                required(args, "id")?,
                revision(args)?,
                args["enabled"]
                    .as_bool()
                    .ok_or_else(|| Error::Config("enabled 必须为布尔值".into()))?,
            )?
            .view(),
        "scheduler.delete" => service.delete(required(args, "id")?, revision(args)?)?,
        "scheduler.run_now" => service
            .run_now(
                required(args, "id")?,
                revision(args)?,
                required(args, "requestKey")?,
            )?
            .view(),
        "scheduler.stop" => service.stop_occurrence(required(args, "occurrenceId")?)?,
        "scheduler.history" => {
            json!({"items":service.history(args["id"].as_str(),args["limit"].as_u64().unwrap_or(20) as usize)?.iter().map(|occurrence|occurrence.view()).collect::<Vec<_>>()})
        }
        _ => return Err(Error::Tool("未知定时任务接口".into())),
    })
}

fn required<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args[name]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::Config(format!("缺少 {name}")))
}
fn revision(args: &Value) -> Result<u64> {
    args["expectedRevision"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::Config("缺少有效修订".into()))
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub fn schedule_schema() -> Value {
    let times = json!({"type":"array","minItems":1,"maxItems":8,"uniqueItems":true,"items":{"type":"string","pattern":"^([01][0-9]|2[0-3]):[0-5][0-9]$"}});
    let days = |max: u8| json!({"type":"array","uniqueItems":true,"maxItems":max,"items":{"type":"integer","minimum":1,"maximum":max}});
    let shortcut = object(
        json!({"kind":{"const":"shortcut"},"shortcutId":{"type":"string"},"expectedPublishedRevision":{"type":"integer","minimum":1}}),
        &["kind", "shortcutId", "expectedPublishedRevision"],
    );
    let binding = json!({"oneOf":[shortcut,object(json!({"kind":{"const":"closeGame"}}),&["kind"]),object(json!({"kind":{"const":"closeToolSoftware"}}),&["kind"])]});
    let mut daily = object(
        json!({"kind":{"const":"daily"},"localTimes":times,"intervalDays":{"type":"integer","minimum":1,"maximum":365,"default":1,"description":"按日期每 N 天执行，1 表示每天。"},"startDate":{"type":"string","format":"date","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$","description":"循环开始日期 YYYY-MM-DD；intervalDays 大于1时必须提供，决定隔日相位，后续修改不自动重置。"}}),
        &["kind", "localTimes"],
    );
    daily["anyOf"] = json!([{"properties":{"intervalDays":{"const":1}}},{"required":["intervalDays","startDate"]}]);
    object(
        json!({
            "name":{"type":"string","minLength":1,"maxLength":80},
            "entries":{"type":"array","minItems":1,"maxItems":32,"items":object(json!({"entryId":{"type":"string","format":"uuid"},"binding":binding}),&["binding"])},
            "trigger":{"oneOf":[
                object(json!({"kind":{"const":"once"},"at":{"type":"string","format":"date-time"}}),&["kind","at"]),
                daily,
                object(json!({"kind":{"const":"weekly"},"weekdays":days(7),"localTimes":times}),&["kind","weekdays","localTimes"]),
                object(json!({"kind":{"const":"monthly"},"daysOfMonth":days(31),"lastDayOfMonth":{"type":"boolean"},"localTimes":times}),&["kind","daysOfMonth","localTimes"])
            ]}
        }),
        &["name", "entries", "trigger"],
    )
}

type Definition = (&'static str, &'static str, &'static str, Value, bool);
fn definitions() -> Vec<Definition> {
    let id = json!({"type":"string","format":"uuid"});
    let revision = json!({"type":"integer","minimum":1});
    vec![
        (
            "scheduler.clock",
            "读取定时任务时钟",
            "只读取得本机当前日期、时间和星期；可将 YYYY-MM-DDTHH:mm 本机时间换算为含该日期实际 UTC 偏移的单次执行时刻，不注册或执行任务。",
            object(
                json!({"localDateTime":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}$"}}),
                &[],
            ),
            true,
        ),
        (
            "scheduler.actions.list",
            "查找可绑定任务",
            "搜索快捷任务标题和已保存内容，取得真实 ID 与发布修订；允许同一目标绑定多次。",
            object(
                json!({"query":{"type":"string","maxLength":200},"category":{"type":"string","enum":["shortcuts","defaults"]},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":50}}),
                &[],
            ),
            true,
        ),
        (
            "scheduler.list",
            "查看定时任务",
            "读取本插件管理的定时任务。",
            object(json!({}), &[]),
            true,
        ),
        (
            "scheduler.get",
            "读取定时任务",
            "读取时间规则、有序列表和实际注册状态。",
            object(json!({"id":id}), &["id"]),
            true,
        ),
        (
            "scheduler.preview",
            "预览定时安排",
            "只读核对有序任务引用与执行时间，不注册或运行任务。",
            schedule_schema(),
            true,
        ),
        (
            "scheduler.create",
            "创建定时任务",
            "按明确时间注册管理员运行的定时任务；执行项数组保留重复目标和顺序，保存不立即运行。",
            schedule_schema(),
            false,
        ),
        (
            "scheduler.update",
            "修改定时任务",
            "更新这条定时任务的时间及有序列表，不修改原快捷任务。",
            object(
                json!({"id":id,"expectedRevision":revision,"schedule":schedule_schema()}),
                &["id", "expectedRevision", "schedule"],
            ),
            false,
        ),
        (
            "scheduler.set_enabled",
            "开关定时任务",
            "暂停或恢复后续定时触发。",
            object(
                json!({"id":id,"expectedRevision":revision,"enabled":{"type":"boolean"}}),
                &["id", "expectedRevision", "enabled"],
            ),
            false,
        ),
        (
            "scheduler.delete",
            "删除定时任务",
            "只删除本插件的定时安排，不删除原任务，保留运行历史。",
            object(
                json!({"id":id,"expectedRevision":revision}),
                &["id", "expectedRevision"],
            ),
            false,
        ),
        (
            "scheduler.run_now",
            "立即执行一次",
            "按保存顺序执行一遍，每项出错后继续；requestKey 是本次请求的 UUID，重试必须复用。",
            object(
                json!({"id":id,"expectedRevision":revision,"requestKey":id}),
                &["id", "expectedRevision", "requestKey"],
            ),
            false,
        ),
        (
            "scheduler.history",
            "查看定时执行记录",
            "读取逐项运行记录，失败或结果不明不等于完成。",
            object(
                json!({"id":id,"limit":{"type":"integer","minimum":1,"maximum":100}}),
                &[],
            ),
            true,
        ),
        (
            "scheduler.stop",
            "停止本轮定时执行",
            "只停止这一轮及尚未开始的执行项，不删除源任务。",
            object(json!({"occurrenceId":id}), &["occurrenceId"]),
            false,
        ),
    ]
}
