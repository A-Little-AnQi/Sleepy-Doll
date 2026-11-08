use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime, PrimitiveDateTime, Time};

use crate::{Error, Result, runtime::operation::task::WorkflowRevision};

pub const PLUGIN_ID: &str = "windows-scheduler";
pub const MAX_ENTRIES: usize = 32;
pub const MAX_TIMES: usize = 8;
pub const MAX_INTERVAL_DAYS: u16 = 365;

fn daily_interval() -> u16 {
    1
}
fn every_day(value: &u16) -> bool {
    *value == 1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Trigger {
    Once {
        at: String,
    },
    Daily {
        local_times: Vec<String>,
        #[serde(default = "daily_interval", skip_serializing_if = "every_day")]
        interval_days: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_date: Option<String>,
    },
    Weekly {
        weekdays: Vec<u8>,
        local_times: Vec<String>,
    },
    Monthly {
        days_of_month: Vec<u8>,
        #[serde(default)]
        last_day_of_month: bool,
        local_times: Vec<String>,
    },
}

impl Trigger {
    pub fn validate(&self, now: OffsetDateTime) -> Result<()> {
        match self {
            Self::Once { at } => {
                if parse_at(at)? <= now {
                    return Err(Error::Config("指定日期和时间必须晚于现在".into()));
                }
                return Ok(());
            }
            Self::Weekly { weekdays, .. } => validate_days(weekdays, 7, false)?,
            Self::Monthly {
                days_of_month,
                last_day_of_month,
                ..
            } => validate_days(days_of_month, 31, *last_day_of_month)?,
            Self::Daily {
                interval_days,
                start_date,
                ..
            } => {
                if !(1..=MAX_INTERVAL_DAYS).contains(interval_days) {
                    return Err(Error::Config(format!(
                        "天数间隔须为 1–{MAX_INTERVAL_DAYS} 的整数"
                    )));
                }
                if let Some(value) = start_date {
                    parse_start_date(value)?;
                } else if *interval_days > 1 {
                    return Err(Error::Config("按天间隔执行需要开始日期".into()));
                }
            }
        }
        let times = self.times();
        if times.is_empty() || times.len() > MAX_TIMES {
            return Err(Error::Config("请设置 1–8 个执行时刻".into()));
        }
        let mut seen = std::collections::HashSet::new();
        for value in times {
            parse_time(value)?;
            if !seen.insert(value) {
                return Err(Error::Config("执行时刻不能重复".into()));
            }
        }
        Ok(())
    }

    pub fn times(&self) -> &[String] {
        match self {
            Self::Once { .. } => &[],
            Self::Daily { local_times, .. }
            | Self::Weekly { local_times, .. }
            | Self::Monthly { local_times, .. } => local_times,
        }
    }

    pub fn matches_date(&self, date: Date) -> bool {
        match self {
            Self::Once { .. } => false,
            Self::Daily {
                interval_days,
                start_date,
                ..
            } => {
                if !(1..=MAX_INTERVAL_DAYS).contains(interval_days) {
                    return false;
                }
                match start_date {
                    Some(value) => parse_start_date(value).is_ok_and(|start| {
                        date >= start
                            && (date - start).whole_days() % i64::from(*interval_days) == 0
                    }),
                    None => *interval_days == 1,
                }
            }
            Self::Weekly { weekdays, .. } => {
                weekdays.contains(&date.weekday().number_from_monday())
            }
            Self::Monthly {
                days_of_month,
                last_day_of_month,
                ..
            } => {
                days_of_month.contains(&date.day())
                    || (*last_day_of_month
                        && date
                            .next_day()
                            .is_some_and(|next| next.month() != date.month()))
            }
        }
    }

    pub fn next(&self, after: OffsetDateTime, count: usize) -> Result<Vec<OffsetDateTime>> {
        if let Self::Once { at } = self {
            let at = parse_at(at)?;
            return Ok(if at > after { vec![at] } else { vec![] });
        }
        let local = super::system::to_local(after)?;
        let mut date = local.date();
        let wanted = count.clamp(1, 16);
        let mut horizon = 1462;
        if let Self::Daily {
            interval_days,
            start_date,
            ..
        } = self
        {
            if !(1..=MAX_INTERVAL_DAYS).contains(interval_days) {
                return Err(Error::Config("天数间隔超出范围".into()));
            }
            if let Some(value) = start_date {
                date = date.max(parse_start_date(value)?);
            } else if *interval_days > 1 {
                return Err(Error::Config("按天间隔执行需要开始日期".into()));
            }
            horizon = horizon.max(usize::from(*interval_days) * (wanted + 1));
        }
        let mut found = Vec::new();
        let mut times = self
            .times()
            .iter()
            .map(|value| parse_time(value))
            .collect::<Result<Vec<_>>>()?;
        times.sort();
        for _ in 0..=horizon {
            if self.matches_date(date) {
                for time in &times {
                    if let Some(at) =
                        super::system::from_local(PrimitiveDateTime::new(date, *time))?
                        && at > after
                    {
                        found.push(at);
                        if found.len() >= wanted {
                            return Ok(found);
                        }
                    }
                }
            }
            date = date
                .next_day()
                .ok_or_else(|| Error::Config("日期超出支持范围".into()))?;
        }
        Ok(found)
    }
}

pub fn parse_start_date(value: &str) -> Result<Date> {
    let format = time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
        .expect("固定日期格式");
    let date = Date::parse(value, &format)
        .map_err(|_| Error::Config("开始日期须为有效的 YYYY-MM-DD 日期".into()))?;
    if value.len() != 10 || date.year() < 1601 || date.to_string() != value {
        return Err(Error::Config("开始日期须为 1601–9999 年的有效日期".into()));
    }
    Ok(date)
}

fn validate_days(days: &[u8], maximum: u8, allow_empty: bool) -> Result<()> {
    let unique = days.iter().collect::<std::collections::HashSet<_>>();
    if (!allow_empty && days.is_empty())
        || unique.len() != days.len()
        || days.iter().any(|day| !(1..=maximum).contains(day))
    {
        return Err(Error::Config("执行日期为空、重复或超出范围".into()));
    }
    Ok(())
}

pub fn parse_at(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).map_err(|_| {
        Error::Config("指定日期须包含年月日、时分和时区偏移，例如 2026-11-02T20:00:00+08:00".into())
    })
}

pub fn parse_time(value: &str) -> Result<Time> {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || byte.is_ascii_digit())
    {
        return Err(Error::Config("时间必须为 24 小时制 HH:mm".into()));
    }
    let hour = (bytes[0] - b'0') * 10 + bytes[1] - b'0';
    let minute = (bytes[3] - b'0') * 10 + bytes[4] - b'0';
    Time::from_hms(hour, minute, 0)
        .map_err(|_| Error::Config("小时须为 00–23，分钟须为 00–59".into()))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Binding {
    Shortcut {
        shortcut_id: String,
        expected_published_revision: u64,
    },
    CloseGame,
    CloseToolSoftware,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultAction {
    CloseGame,
    CloseToolSoftware,
}

impl DefaultAction {
    pub fn title(self) -> &'static str {
        match self {
            Self::CloseGame => "关闭游戏",
            Self::CloseToolSoftware => "关闭工具软件",
        }
    }
    pub fn binding(self) -> Binding {
        match self {
            Self::CloseGame => Binding::CloseGame,
            Self::CloseToolSoftware => Binding::CloseToolSoftware,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
    pub binding: Binding,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduleInput {
    pub name: String,
    pub trigger: Trigger,
    pub entries: Vec<EntryInput>,
}

impl ScheduleInput {
    pub fn validate(&self, now: OffsetDateTime) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err(Error::Config("名称须为 1–80 个字符".into()));
        }
        if let Some((index, value)) = self.name.chars().enumerate().find(|(_, value)| {
            !matches!(*value, '\u{9}' | '\u{a}' | '\u{d}' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
        }) {
            return Err(Error::Config(format!("名称第 {} 位含不可用字符 U+{:04X}，请删除后重试",index+1,value as u32)));
        }
        if self.entries.is_empty() || self.entries.len() > MAX_ENTRIES {
            return Err(Error::Config("执行列表须包含 1–32 项任务".into()));
        }
        self.trigger.validate(now)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_action: Option<DefaultAction>,
    pub entry_id: String,
    pub workflow_id: String,
    pub published_revision: u64,
    pub title: String,
    pub source_hash: String,
    pub contract_hash: String,
    pub workflow: WorkflowRevision,
}

impl BoundEntry {
    pub fn binding(&self) -> Binding {
        self.default_action
            .map(DefaultAction::binding)
            .unwrap_or_else(|| Binding::Shortcut {
                shortcut_id: self.workflow_id.clone(),
                expected_published_revision: self.published_revision,
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    pub id: String,
    pub revision: u64,
    pub name: String,
    pub trigger: Trigger,
    pub entries: Vec<BoundEntry>,
    pub enabled: bool,
    pub registration: String,
    pub system_hash: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryResult {
    pub entry_id: String,
    pub title: String,
    pub state: String,
    pub run_id: Option<String>,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_state: Option<String>,
}

impl Schedule {
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({"id":self.id,"revision":self.revision,"name":self.name,"trigger":self.trigger,"enabled":self.enabled,"registration":self.registration,"error":self.error,
            "entries":self.entries.iter().map(|entry|serde_json::json!({"entryId":entry.entry_id,"title":entry.title,"binding":entry.binding()})).collect::<Vec<_>>()})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Occurrence {
    pub id: String,
    pub key: String,
    pub schedule: Schedule,
    pub state: String,
    pub results: Vec<EntryResult>,
    pub created_at: String,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn uuid(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| Error::Config("任务标识无效".into()))
}

impl Occurrence {
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({"id":self.id,"schedule":self.schedule.view(),"state":self.state,"results":self.results,"createdAt":self.created_at,"error":self.error})
    }
}
