//! 实机验证：用本机真实的模型配置与真实的 BetterGI 桥执行一次完整问答。
//!
//! 默认忽略（会调用真实模型 API、消耗真实额度），手动执行：
//!
//! ```text
//! cargo test --no-default-features --test live -- --ignored --nocapture
//! ```
//!
//! 用的是 `dist/Sleepy-Doll/user/config.json` 里的模型与桥配置，只把数据库换到
//! 临时目录 —— 运行中的应用占着原来那个库。

use serde_json::{Value, json};
use sleepy_doll::app::AppController;
use std::{fs, sync::Arc, thread, time::Duration};

const INSTALL: &str = "dist/Sleepy-Doll";

#[test]
#[ignore = "会调用真实模型 API"]
fn answers_a_question_about_the_users_own_configuration() {
    // 配置里的 `./skills`、`./plugins`、`./catalog` 是相对应用工作目录的，
    // 测试进程默认在仓库根，得先站到应用目录里去。
    let install = fs::canonicalize(INSTALL).expect("找不到 dist/Sleepy-Doll，先构建一次");
    let source = fs::read_to_string(install.join("user/config.json"))
        .expect("找不到真实配置，先运行一次应用");
    std::env::set_current_dir(&install).unwrap();

    let mut config: Value = serde_json::from_str(&source).unwrap();
    let directory = tempfile::tempdir().unwrap();
    // 相对路径以配置文件所在目录为基准，配置挪到临时目录后这些基准就变了，
    // 一律改成应用真实数据目录下的绝对路径。
    let user = install.join("user");
    // 产品自带的插件（含它的技能）在安装目录，用户自己的在 user\ 下。
    config["agent"]["skillDirectories"] = json!([user.join("skills")]);
    config["plugins"]["directories"] = json!([install.join("plugins"), user.join("plugins")]);
    config["runtime"]["catalogDirectory"] = json!(user.join("catalog"));
    // 运行中的应用占着原来那个库，换一个。
    config["storage"]["database"] = json!(directory.path().join("live.db"));
    config["runtime"]["durationSec"] = json!(300);
    let path = directory.path().join("config.json");
    fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();

    let controller = Arc::new(AppController::load(&path).unwrap());
    let call = |method: &str, params: Value| {
        controller
            .handle(method, params, Arc::new(|_, _| {}))
            .unwrap()
    };

    // 换提问用环境变量，不用改代码：LIVE_PROMPT="帮我刷霜仙花"
    let prompt = std::env::var("LIVE_PROMPT")
        .unwrap_or_else(|_| "帮我看下我有哪些调度器，我需要梳理一下".into());
    let run = call("task.submit", json!({ "prompt": prompt }));
    let stop_at_approval = std::env::var("LIVE_STOP_AT_APPROVAL").as_deref() == Ok("1");
    let mut completed = Value::Null;
    for _ in 0..1200 {
        completed = call("task.get", json!({"id":run["id"]}));
        if ["answered", "failed", "needsReview"]
            .contains(&completed["state"].as_str().unwrap_or(""))
            || stop_at_approval
                && matches!(
                    completed["state"].as_str(),
                    Some("awaitingApproval" | "awaitingUser")
                )
        {
            break;
        }
        thread::sleep(Duration::from_millis(250));
    }

    let history = call("conversation.get", json!({"id":run["conversationId"]}));
    let messages = history["messages"].as_array().cloned().unwrap_or_default();
    println!("\n================ 会话 ================");
    let mut calls = 0;
    for message in &messages {
        let role = message["role"].as_str().unwrap_or("");
        let content = message["content"].as_str().unwrap_or("");
        match role {
            "user" => println!("\n[用户] {content}"),
            "assistant" => {
                if !content.is_empty() {
                    println!("\n[助手] {content}");
                }
                for call in message["toolCalls"].as_array().into_iter().flatten() {
                    calls += 1;
                    println!(
                        "  → {}({})",
                        call["name"].as_str().unwrap_or(""),
                        call["arguments"]
                    );
                }
            }
            "tool" => println!("  ← {}", content.chars().take(220).collect::<String>()),
            _ => {}
        }
    }
    println!("\n================ 结果 ================");
    println!("状态: {}", completed["state"]);
    println!("工具调用次数: {calls}");
    let active_model = config["activeModel"].as_str().unwrap_or("(未知)");
    let active_name = config["models"]
        .as_array()
        .and_then(|models| {
            models
                .iter()
                .find(|model| model["id"].as_str() == Some(active_model))
        })
        .and_then(|model| model["name"].as_str())
        .unwrap_or(active_model);
    println!("模型: {active_name} ({active_model})");
    if let Some(error) = completed["error"].as_str() {
        println!("错误: {error}");
    }
    println!("结论: {}", completed["result"].as_str().unwrap_or("(无)"));

    controller.shutdown();
}
