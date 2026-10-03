use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

const SERVICE: &str = "https://sleepy-doll.restless-nh3.com";
const DOWNLOAD_HOST: &str = "download.sleepy-doll.restless-nh3.com";
const MAX_INSTALLER: u64 = 256 * 1024 * 1024;

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
}

fn version_parts(version: &str) -> Result<[u64; 3]> {
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| Error::Config("版本号格式不正确".into()))?;
    parts
        .try_into()
        .map_err(|_| Error::Config("版本号必须包含主版本、次版本和修订号".into()))
}

fn validate_release(release: &Release, channel: &str) -> Result<()> {
    version_parts(&release.version)?;
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
                analytics_enabled: false,
                channel: if version_parts(env!("CARGO_PKG_VERSION"))? < [0, 1, 0] {
                    "test"
                } else {
                    "stable"
                }
                .into(),
                pending_version: None,
            }
        };
        let service = Self {
            root,
            preferences: Mutex::new(preferences),
            release: Mutex::new(None),
            downloaded: Mutex::new(None),
            operation: Mutex::new(()),
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
        json!({"currentVersion":env!("CARGO_PKG_VERSION"), "channel":preferences.channel,
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
        Ok(self.state())
    }

    fn client(&self, seconds: u64) -> Result<reqwest::blocking::Client> {
        Ok(reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(seconds))
            .redirect(reqwest::redirect::Policy::none())
            .build()?)
    }

    pub fn record(&self, name: &str, target_version: Option<&str>) {
        let (client_id, channel) = {
            let preferences = self.preferences.lock().unwrap();
            if !preferences.analytics_enabled {
                return;
            }
            (preferences.client_id.clone(), preferences.channel.clone())
        };
        let payload = json!({"clientId":client_id,"event":name,"version":env!("CARGO_PKG_VERSION"),
            "targetVersion":target_version,"channel":channel,"platform":"windows","sessionId":self.session_id});
        if let Ok(client) = self.client(5) {
            let _ = client
                .post(format!("{SERVICE}/api/events"))
                .json(&payload)
                .send();
        }
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
        let newer = version_parts(&release.version)? > version_parts(env!("CARGO_PKG_VERSION"))?;
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
        assert!(version_parts("0.0.1-test").is_err());
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
}
