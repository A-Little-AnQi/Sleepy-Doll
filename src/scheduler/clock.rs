use crate::{Error, Result};
use serde_json::{Value, json};
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset, format_description::well_known::Rfc3339};

fn local_timestamp(at: OffsetDateTime) -> Result<OffsetDateTime> {
    let local = super::system::to_local(at)?;
    let seconds = local.assume_utc().unix_timestamp() - at.unix_timestamp();
    let offset = UtcOffset::from_whole_seconds(
        i32::try_from(seconds).map_err(|_| Error::Config("本机时区偏移无效".into()))?,
    )
    .map_err(|_| Error::Config("本机时区偏移无效".into()))?;
    Ok(at.to_offset(offset))
}

/// 只读本机时钟与 Windows 日期转换，不接触注册表或系统任务。
pub fn read(local_datetime: Option<&str>) -> Result<Value> {
    let now = OffsetDateTime::now_utc();
    let local = local_timestamp(now)?;
    let mut value = json!({
        "now": now.format(&Rfc3339).expect("时钟日期"),
        "localNow": local.format(&Rfc3339).expect("本地日期"),
        "localDate": local.date().to_string(),
        "weekday": local.weekday().number_from_monday(),
        "timeZone": "system",
    });
    if let Some(text) = local_datetime {
        let format =
            time::format_description::parse_borrowed::<2>("[year]-[month]-[day]T[hour]:[minute]")
                .expect("固定日期格式");
        let requested = PrimitiveDateTime::parse(text, &format)
            .map_err(|_| Error::Config("请使用有效的本机日期时间 YYYY-MM-DDTHH:mm".into()))?;
        let at = super::system::from_local(requested)?
            .ok_or_else(|| Error::Config("该本机时间不存在，请选择另一个时刻".into()))?;
        value["resolved"] = json!({
            "at": local_timestamp(at)?.format(&Rfc3339).expect("执行日期"),
            "future": at > now,
        });
    }
    Ok(value)
}
