//! Windows 触发器 XML 只由强类型时间规则生成，不接受外部 XML。
use super::model::{Trigger, parse_at};
use crate::Result;
use time::{OffsetDateTime, PrimitiveDateTime};

pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn local_stamp(value: PrimitiveDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        value.year(),
        u8::from(value.month()),
        value.day(),
        value.hour(),
        value.minute(),
        value.second()
    )
}

pub fn triggers(trigger: &Trigger, created_at: OffsetDateTime) -> Result<String> {
    if let Trigger::Once { at } = trigger {
        let at = parse_at(at)?
            .format(&time::format_description::well_known::Rfc3339)
            .expect("有效日期");
        return Ok(format!(
            "<TimeTrigger><StartBoundary>{at}</StartBoundary><Enabled>true</Enabled></TimeTrigger>"
        ));
    }
    let date = match trigger {
        Trigger::Daily {
            start_date: Some(value),
            ..
        } => super::model::parse_start_date(value)?,
        _ => super::system::to_local(created_at)?.date(),
    };
    let mut xml = String::new();
    for time in trigger.times() {
        let boundary = format!("{date}T{time}:00");
        let calendar = match trigger {
            Trigger::Daily { interval_days, .. } => {
                format!(
                    "<ScheduleByDay><DaysInterval>{interval_days}</DaysInterval></ScheduleByDay>"
                )
            }
            Trigger::Weekly { weekdays, .. } => {
                let names = [
                    "Monday",
                    "Tuesday",
                    "Wednesday",
                    "Thursday",
                    "Friday",
                    "Saturday",
                    "Sunday",
                ];
                let days = weekdays
                    .iter()
                    .map(|day| format!("<{} />", names[*day as usize - 1]))
                    .collect::<String>();
                format!(
                    "<ScheduleByWeek><WeeksInterval>1</WeeksInterval><DaysOfWeek>{days}</DaysOfWeek></ScheduleByWeek>"
                )
            }
            Trigger::Monthly {
                days_of_month,
                last_day_of_month,
                ..
            } => {
                let mut days = days_of_month
                    .iter()
                    .map(|day| format!("<Day>{day}</Day>"))
                    .collect::<String>();
                if *last_day_of_month {
                    days.push_str("<Day>Last</Day>");
                }
                let months = [
                    "January",
                    "February",
                    "March",
                    "April",
                    "May",
                    "June",
                    "July",
                    "August",
                    "September",
                    "October",
                    "November",
                    "December",
                ]
                .iter()
                .map(|name| format!("<{name} />"))
                .collect::<String>();
                format!(
                    "<ScheduleByMonth><DaysOfMonth>{days}</DaysOfMonth><Months>{months}</Months></ScheduleByMonth>"
                )
            }
            Trigger::Once { .. } => unreachable!(),
        };
        xml.push_str(&format!("<CalendarTrigger><StartBoundary>{boundary}</StartBoundary><Enabled>true</Enabled>{calendar}</CalendarTrigger>"));
    }
    Ok(xml)
}
