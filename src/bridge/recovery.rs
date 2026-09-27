//! User-confirmed recovery, independent of model runs.
use super::{BgiClient, control};
use crate::{
    config::BridgeConfig,
    error::{Error, Result},
};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value[name]
        .as_str()
        .ok_or_else(|| Error::Config(format!("缺少 {name}")))
}
pub fn preview(config: &BridgeConfig, params: &Value) -> Result<Value> {
    let id = required(params, "changeId")?;
    let version = required(params, "recordVersion")?;
    let mode = params["mode"].as_str().unwrap_or("fields");
    let paths = params.get("paths").cloned().unwrap_or(json!([]));
    let serialized = paths.to_string();
    let mut result = control::recovery("preview", &[id, version, mode, &serialized])?;
    if mode == "fields" && result["hostRunning"] == true {
        let client = BgiClient::new(config.clone());
        let online = (|| {
            let contract = client.describe("bgi.preview_setting_restore")?;
            if contract["callable"] != true || contract["effect"] != "readOnly" {
                return Err(Error::Tool("当前连接不支持逐项恢复".into()));
            }
            let response = client.invoke(
                "bgi.preview_setting_restore",
                &json!({"changeId":id,"recordVersion":version,"paths":paths}),
            )?;
            response
                .get("result")
                .cloned()
                .ok_or_else(|| Error::Tool("恢复预览没有返回结果".into()))
        })();
        match online {
            Ok(value) => result = value,
            Err(error) => {
                result["canApply"] = json!(false);
                let message = error.user_message();
                result["reason"] = json!(if config.enabled
                    && (message.contains("bgi.")
                        || message.contains("METHOD_NOT_FOUND")
                        || message.contains("当前连接不支持逐项恢复"))
                {
                    "当前连接还不支持在线恢复，请重启 BetterGI 后重新连接；或退出 BetterGI 后离线恢复。".to_owned()
                } else if config.enabled {
                    format!(
                        "在线恢复未就绪：{}。可重新连接 BetterGI，或退出它后恢复。",
                        message
                    )
                } else {
                    "请先连接 BetterGI 才能在线恢复；也可以退出 BetterGI 后恢复。".into()
                });
            }
        }
    }
    Ok(result)
}
pub fn restore(config: &BridgeConfig, params: &Value) -> Result<Value> {
    if params["online"] == true {
        let client = BgiClient::new(config.clone());
        let response = client.invoke(
            "bgi.commit_settings",
            &json!({"planId":required(params,"planId")?}),
        )?;
        let job_id = response["jobId"]
            .as_str()
            .ok_or_else(|| Error::Tool("恢复请求结果不完整".into()))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let job = client.job(job_id)?;
            match job["state"].as_str() {
                Some("completed") if job["verification"]["status"] == "succeeded" => {
                    return Ok(
                        json!({"restored":true,"online":true,"recoveryChangeId":job["result"]["changeId"]}),
                    );
                }
                Some("failed") => {
                    return Err(Error::Tool(
                        job["error"]
                            .as_str()
                            .unwrap_or("恢复失败，没有确认配置已恢复。请重新预览。")
                            .into(),
                    ));
                }
                Some("cancelled" | "interrupted" | "completed") => {
                    return Err(Error::Tool(
                        "恢复结果尚未确认，请刷新后核对，不要重复覆盖。".into(),
                    ));
                }
                _ if Instant::now() >= deadline => {
                    return Err(Error::Tool(
                        "恢复仍在处理，请刷新后核对；不要重复提交。".into(),
                    ));
                }
                _ => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }
    let mode = params["mode"].as_str().unwrap_or("full");
    let paths = params
        .get("paths")
        .cloned()
        .unwrap_or(json!([]))
        .to_string();
    control::recovery(
        "restore",
        &[
            required(params, "changeId")?,
            required(params, "recordVersion")?,
            required(params, "currentVersion")?,
            mode,
            &paths,
        ],
    )
}
