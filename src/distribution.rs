use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const SERVICE: &str = "https://sleepy-doll.restless-nh3.com";
const DOWNLOAD_HOST: &str = "sleepy-doll-download.restless-nh3.com";
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;
/// 统计出口注册前允许积压的事件数上限；超出丢最旧的。
const MAX_PENDING_EVENTS: usize = 32;

/// 统计事件出口：把白名单负载交给宿主（桌面壳转发给界面里的 Google tag）。
/// 只在锁外调用，实现里不得再拿 Distribution 的锁。
pub type EventSink = Arc<dyn Fn(&Value) + Send + Sync>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub channel: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub notes: String,
    pub published_at: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Preferences {
    client_id: String,
    analytics_enabled: bool,
    channel: String,
    pending_version: Option<String>,
}

pub struct Distribution {
    root: PathBuf,
    preferences: Mutex<Preferences>,
    release: Mutex<Option<Release>>,
    downloaded: Mutex<Option<PathBuf>>,
    operation: Mutex<()>,
    session_id: u64,
    delivery: Mutex<EventDelivery>,
}

/// 出口与积压队列共用一把锁：注册时的 drain 与新事件的入队/直发原子完成，
/// 事件不会在「取到 None 出口」与「入队」之间被注册流程漏掉。
struct EventDelivery {
    sink: Option<EventSink>,
    pending: VecDeque<Value>,
}

fn version_parts(version: &str) -> Result<semver::Version> {
    if version.len() > 128 {
        return Err(Error::Config("版本号超过长度限制".into()));
    }
    semver::Version::parse(version).map_err(|_| Error::Config("版本号必须符合 SemVer".into()))
}

fn validate_channel(version: &str, channel: &str) -> Result<()> {
    let version = version_parts(version)?;
    if !matches!(channel, "stable" | "test") || (channel == "stable" && !version.pre.is_empty()) {
        return Err(Error::Config("测试版本不能进入正式通道".into()));
    }
    Ok(())
}

fn build_channel() -> Result<String> {
    let channel = env!("SLEEPY_RELEASE_CHANNEL");
    validate_channel(env!("CARGO_PKG_VERSION"), channel)?;
    Ok(channel.into())
}

// 白名单事件字段：不含对话内容、模型 key、文件路径。
fn event_payload(
    enabled: bool,
    client_id: &str,
    name: &str,
    target_version: Option<&str>,
    channel: &str,
    session_id: u64,
) -> Option<Value> {
    if !enabled {
        return None;
    }
    Some(
        json!({"clientId":client_id,"event":name,"version":env!("CARGO_PKG_VERSION"),
        "targetVersion":target_version,"channel":channel,"platform":"windows","sessionId":session_id}),
    )
}

fn validate_release(release: &Release, channel: &str) -> Result<()> {
    validate_channel(&release.version, channel)?;
    let url =
        url::Url::parse(&release.url).map_err(|_| Error::Config("更新地址格式不正确".into()))?;
    if release.channel != channel
        || url.scheme() != "https"
        || url.host_str() != Some(DOWNLOAD_HOST)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path()
            != format!(
                "/releases/{}/Sleepy-Doll-{}-setup.exe",
                release.version, release.version
            )
        || release.size == 0
        || release.size > MAX_INSTALLER
        || release.sha256.len() != 64
        || !release.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::Config("更新清单未通过来源和文件校验".into()));
    }
    Ok(())
}

impl Distribution {
    pub fn load(user_directory: &Path) -> Result<Self> {
        let root = user_directory.join(".sleepy-doll").join("updates");
        fs::create_dir_all(&root)?;
        let path = root.join("preferences.json");
        let preferences = if path.exists() {
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            Preferences {
                client_id: uuid::Uuid::new_v4().to_string(),
                analytics_enabled: true,
                channel: build_channel()?,
                pending_version: None,
            }
        };
        let service = Self {
            root,
            preferences: Mutex::new(preferences),
            release: Mutex::new(None),
            downloaded: Mutex::new(None),
            operation: Mutex::new(()),
            delivery: Mutex::new(EventDelivery {
                sink: None,
                pending: VecDeque::new(),
            }),
            session_id: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        service.save()?;
        Ok(service)
    }

    fn save(&self) -> Result<()> {
        fs::write(
            self.root.join("preferences.json"),
            serde_json::to_vec_pretty(&*self.preferences.lock().unwrap())?,
        )?;
        Ok(())
    }

    pub fn state(&self) -> Value {
        let preferences = self.preferences.lock().unwrap();
        json!({"currentVersion":env!("CARGO_PKG_VERSION"), "currentChannel":build_channel().unwrap_or_else(|_| "test".into()), "channel":preferences.channel,
            "analyticsEnabled":preferences.analytics_enabled,"release":*self.release.lock().unwrap(),
            "downloaded":self.downloaded.lock().unwrap().is_some()})
    }

    pub fn configure(&self, enabled: bool, channel: &str) -> Result<Value> {
        if !matches!(channel, "test" | "stable") {
            return Err(Error::Config("更新通道必须是 test 或 stable".into()));
        }
        let _operation = self.operation.lock().unwrap();
        {
            let mut preferences = self.preferences.lock().unwrap();
            preferences.analytics_enabled = enabled;
            preferences.channel = channel.into();
        }
        *self.release.lock().unwrap() = None;
        *self.downloaded.lock().unwrap() = None;
        self.save()?;
        // 开关变化立即通知界面：关闭时前端可以马上停用 Google tag。
        self.emit(json!({"analyticsEnabled": enabled}));
        Ok(self.state())
    }

    /// 注册统计事件出口。只注册一次；在锁内原子完成注册与积压转交，锁外按序调用出口。
    pub fn set_sink(&self, sink: EventSink) {
        let queued = {
            let mut delivery = self.delivery.lock().unwrap();
            if delivery.sink.is_some() {
                return;
            }
            delivery.sink = Some(sink.clone());
            std::mem::take(&mut delivery.pending)
        };
        for payload in queued {
            sink(&payload);
        }
    }

    /// 事件交给出口；出口尚未注册时进有界启动队列。锁外调用出口。
    fn emit(&self, payload: Value) {
        let sink = {
            let mut delivery = self.delivery.lock().unwrap();
            match &delivery.sink {
                Some(sink) => sink.clone(),
                None => {
                    if delivery.pending.len() >= MAX_PENDING_EVENTS {
                        delivery.pending.pop_front();
                    }
                    delivery.pending.push_back(payload);
                    return;
                }
            }
        };
        sink(&payload);
    }

    fn client(&self, seconds: u64) -> Result<reqwest::blocking::Client> {
        Ok(reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(seconds))
            .redirect(reqwest::redirect::Policy::none())
            .build()?)
    }

    pub fn record(&self, name: &str, target_version: Option<&str>) {
        let (client_id, channel, enabled) = {
            let preferences = self.preferences.lock().unwrap();
            (
                preferences.client_id.clone(),
                preferences.channel.clone(),
                preferences.analytics_enabled,
            )
        };
        let Some(payload) = event_payload(
            enabled,
            &client_id,
            name,
            target_version,
            &channel,
            self.session_id,
        ) else {
            return;
        };
        self.emit(payload);
    }

    pub fn startup(&self) {
        self.record("app_start", None);
        let pending = self.preferences.lock().unwrap().pending_version.clone();
        if pending.as_deref() == Some(env!("CARGO_PKG_VERSION")) {
            self.record("update_success", pending.as_deref());
            self.preferences.lock().unwrap().pending_version = None;
            let _ = self.save();
        }
    }

    pub fn check(&self) -> Result<Value> {
        let _operation = self.operation.lock().unwrap();
        let channel = self.preferences.lock().unwrap().channel.clone();
        let response = self
            .client(15)?
            .get(format!("{SERVICE}/api/releases/{channel}"))
            .header("Cache-Control", "no-cache")
            .send()?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            *self.release.lock().unwrap() = None;
            *self.downloaded.lock().unwrap() = None;
            return Ok(self.state());
        }
        let mut bytes = Vec::new();
        response
            .error_for_status()?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 128 * 1024 {
            return Err(Error::Config("更新清单超过大小限制".into()));
        }
        let release: Release = serde_json::from_slice(&bytes)?;
        validate_release(&release, &channel)?;
        let newer = version_parts(&release.version)?
            .cmp_precedence(&version_parts(env!("CARGO_PKG_VERSION"))?)
            .is_gt();
        if self
            .release
            .lock()
            .unwrap()
            .as_ref()
            .map(|item| &item.version)
            != Some(&release.version)
        {
            *self.downloaded.lock().unwrap() = None;
        }
        *self.release.lock().unwrap() = newer.then_some(release);
        Ok(self.state())
    }

    pub fn download(&self) -> Result<Value> {
        let _operation = self.operation.lock().unwrap();
        let release = self
            .release
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Conflict("请先检查更新".into()))?;
        validate_release(&release, &self.preferences.lock().unwrap().channel)?;
        let path = self
            .root
            .join(format!("Sleepy-Doll-{}-setup.exe", release.version));
        let partial = path.with_extension("partial");
        let outcome = (|| -> Result<()> {
            let mut response = self
                .client(180)?
                .get(&release.url)
                .send()?
                .error_for_status()?;
            if response
                .content_length()
                .is_some_and(|length| length != release.size)
            {
                return Err(Error::Config("安装包大小与更新清单不一致".into()));
            }
            let mut file = fs::File::create(&partial)?;
            let mut hash = Sha256::new();
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let count = response.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total += count as u64;
                if total > release.size {
                    return Err(Error::Config("安装包超过声明大小".into()));
                }
                hash.update(&buffer[..count]);
                file.write_all(&buffer[..count])?;
            }
            file.sync_all()?;
            drop(file);
            if total != release.size
                || format!("{:x}", hash.finalize()) != release.sha256.to_lowercase()
            {
                return Err(Error::Config("安装包 SHA-256 校验失败，请重新下载".into()));
            }
            if path.exists() {
                fs::remove_file(&path)?;
            }
            fs::rename(&partial, &path)?;
            *self.downloaded.lock().unwrap() = Some(path);
            Ok(())
        })();
        if outcome.is_err() {
            let _ = fs::remove_file(partial);
        }
        outcome?;
        Ok(self.state())
    }

    pub fn installer(&self) -> Result<PathBuf> {
        let _operation = self.operation.lock().unwrap();
        let path = self
            .downloaded
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Conflict("请先下载并校验更新".into()))?;
        let release = self
            .release
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::Conflict("更新清单已失效".into()))?;
        let bytes = fs::read(&path)?;
        if bytes.len() as u64 != release.size
            || format!("{:x}", Sha256::digest(&bytes)) != release.sha256.to_lowercase()
        {
            return Err(Error::Config("已下载的安装包校验失败".into()));
        }
        self.preferences.lock().unwrap().pending_version = Some(release.version);
        self.save()?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn versions_compare_numerically() {
        assert!(version_parts("0.1.10").unwrap() > version_parts("0.1.9").unwrap());
        assert!(version_parts("0.0.1-test").is_ok());
        for version in ["0.01.0", "0.1.0-alpha.01", "v0.1.0", "0.1.0/evil"] {
            assert!(version_parts(version).is_err(), "{version}");
        }
    }

    #[test]
    fn release_prereleases_sort_before_the_final_version() {
        let versions = [
            "0.2.0-alpha.2",
            "0.2.0-alpha.10",
            "0.2.0-beta.1",
            "0.2.0-rc.1",
            "0.2.0",
        ];
        for pair in versions.windows(2) {
            assert!(
                version_parts(pair[0])
                    .unwrap()
                    .cmp_precedence(&version_parts(pair[1]).unwrap())
                    .is_lt()
            );
        }
        assert!(
            version_parts("0.2.0+one")
                .unwrap()
                .cmp_precedence(&version_parts("0.2.0+two").unwrap())
                .is_eq()
        );
    }

    #[test]
    fn release_channels_are_explicit_and_do_not_depend_on_major_version() {
        assert!(validate_channel("0.1.0", "test").is_ok());
        assert!(validate_channel("0.1.0", "stable").is_ok());
        assert!(validate_channel("1.0.0-alpha.1", "stable").is_err());
        assert!(validate_channel("1.0.0-alpha.1", "test").is_ok());
        assert!(validate_channel("1.0.0", "unknown").is_err());
        assert_eq!(build_channel().unwrap(), serde_json::from_str::<Value>(include_str!("../release-channel.json")).unwrap()["channel"].as_str().unwrap());
    }
    #[test]
    fn update_manifest_rejects_untrusted_hosts_and_channels() {
        let mut release = Release {
            version: "0.1.0".into(),
            channel: "stable".into(),
            url: format!("https://{DOWNLOAD_HOST}/releases/0.1.0/Sleepy-Doll-0.1.0-setup.exe"),
            size: 7_000_000,
            sha256: "a".repeat(64),
            notes: String::new(),
            published_at: String::new(),
        };
        assert!(validate_release(&release, "stable").is_ok());
        assert!(validate_release(&release, "test").is_err());
        release.url = "https://example.com/installer.exe".into();
        assert!(validate_release(&release, "stable").is_err());
    }

    #[test]
    fn release_manifest_accepts_test_versions_only_in_test_channel() {
        let mut release = Release {
            version: "0.2.0-alpha.10".into(),
            channel: "test".into(),
            url: format!(
                "https://{DOWNLOAD_HOST}/releases/0.2.0-alpha.10/Sleepy-Doll-0.2.0-alpha.10-setup.exe"
            ),
            size: 7_000_000,
            sha256: "a".repeat(64),
            notes: String::new(),
            published_at: String::new(),
        };
        assert!(validate_release(&release, "test").is_ok());
        release.channel = "stable".into();
        assert!(validate_release(&release, "stable").is_err());
        release.channel = "test".into();
        release.url.push_str("?anything=1");
        assert!(validate_release(&release, "test").is_err());
    }

    #[test]
    fn analytics_disabled_produces_no_payload() {
        assert!(event_payload(false, "client", "app_start", None, "stable", 1).is_none());
    }

    #[test]
    fn analytics_enabled_payload_contains_only_whitelisted_fields() {
        let payload = event_payload(true, "client", "app_start", None, "stable", 1).unwrap();
        let mut keys: Vec<&str> = payload
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "channel",
                "clientId",
                "event",
                "platform",
                "sessionId",
                "targetVersion",
                "version"
            ]
        );
    }

    /// 每个用例独立的用户目录，都在 target 下，不碰系统临时目录。
    /// 复位只靠覆写固定偏好内容，不删除目录（删除统一走 Remove-Directory.ps1，由 Root 执行）。
    fn test_service(tag: &str) -> Distribution {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(".tmp")
            .join("ga-direct")
            .join(tag);
        // 幂等复位：覆写固定偏好内容（默认关闭），不删除目录。
        let preferences = root.join(".sleepy-doll").join("updates");
        std::fs::create_dir_all(&preferences).unwrap();
        std::fs::write(
            preferences.join("preferences.json"),
            r#"{"clientId":"00000000-0000-0000-0000-000000000000","analyticsEnabled":false,"channel":"test","pendingVersion":null}"#,
        )
        .unwrap();
        Distribution::load(&root).unwrap()
    }

    /// 并发交接：多线程 emit 与 set_sink 注册交错时，除 32 上限淘汰外事件不丢、不重复。
    #[test]
    fn concurrent_emit_and_sink_registration_lose_no_events() {
        let service = Arc::new(test_service("concurrent-handover"));
        service.configure(true, "test").unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = events.clone();
        // 总数低于队列上限（1 条控制 + 20 条事件），任何一条都不允许丢或重复；
        // 注册与多线程 emit 交错进行，验证 drain 与入队原子交接。
        let writers: Vec<_> = (0..4_u64)
            .map(|writer| {
                let service = service.clone();
                std::thread::spawn(move || {
                    for index in 0..5_u64 {
                        service.emit(json!({"writer":writer, "index":index}));
                    }
                })
            })
            .collect();
        service.set_sink(Arc::new(move |payload| {
            recorder.lock().unwrap().push(payload.clone());
        }));
        for writer in writers {
            writer.join().unwrap();
        }
        let events = events.lock().unwrap();
        let mut seen: Vec<(u64, u64)> = events
            .iter()
            // configure(true) 的 analyticsEnabled 控制消息不是业务事件，先过滤再解构。
            .filter(|item| item.get("writer").is_some())
            .map(|item| {
                (
                    item["writer"].as_u64().unwrap(),
                    item["index"].as_u64().unwrap(),
                )
            })
            .collect();
        seen.sort_unstable();
        let expected: Vec<(u64, u64)> = (0..4_u64)
            .flat_map(|writer| (0..5_u64).map(move |index| (writer, index)))
            .collect();
        assert_eq!(seen, expected);
    }

    fn delivered(tag: &str) -> (Distribution, Arc<Mutex<Vec<Value>>>) {
        let service = test_service(tag);
        service.configure(true, "test").unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = events.clone();
        service.set_sink(Arc::new(move |payload| {
            recorder.lock().unwrap().push(payload.clone());
        }));
        (service, events)
    }

    #[test]
    fn configure_emits_enable_and_disable_controls() {
        let (service, events) = delivered("configure-controls");
        service.configure(false, "test").unwrap();
        let events = events.lock().unwrap();
        let last = events.last().unwrap();
        assert_eq!(last["analyticsEnabled"], json!(false));
        assert!(
            events
                .iter()
                .any(|item| item["analyticsEnabled"] == json!(true))
        );
    }

    #[test]
    fn startup_events_hand_over_to_sink_in_order() {
        let service = test_service("startup-handover");
        service.configure(true, "test").unwrap();
        service.record("app_start", None);
        service.record("update_success", Some("0.0.2"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = events.clone();
        service.set_sink(Arc::new(move |payload| {
            recorder.lock().unwrap().push(payload.clone());
        }));
        let events = events.lock().unwrap();
        assert!(
            events
                .iter()
                .any(|item| item["event"] == json!("app_start"))
        );
        // 控制消息先于两个启动事件，顺序保持。
        assert_eq!(events[0]["analyticsEnabled"], json!(true));
        assert_eq!(events[1]["event"], json!("app_start"));
        assert_eq!(events[2]["event"], json!("update_success"));
    }

    #[test]
    fn pending_queue_is_bounded_at_32() {
        let service = test_service("bounded-queue");
        service.configure(true, "test").unwrap();
        for index in 0..40 {
            service.record("app_start", None);
            assert_eq!(
                service.delivery.lock().unwrap().pending.len(),
                (index + 2).min(MAX_PENDING_EVENTS)
            );
        }
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorder = events.clone();
        service.set_sink(Arc::new(move |payload| {
            recorder.lock().unwrap().push(payload.clone());
        }));
        assert_eq!(events.lock().unwrap().len(), MAX_PENDING_EVENTS);
    }

    #[test]
    fn only_first_sink_registration_wins() {
        let (service, events) = delivered("first-sink");
        let second = Arc::new(Mutex::new(Vec::new()));
        let recorder = second.clone();
        service.set_sink(Arc::new(move |payload| {
            recorder.lock().unwrap().push(payload.clone());
        }));
        service.record("app_start", None);
        assert_eq!(second.lock().unwrap().len(), 0);
        assert_eq!(
            events.lock().unwrap().last().unwrap()["event"],
            json!("app_start")
        );
    }
}
