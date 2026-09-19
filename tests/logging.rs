//! 日志是用户报问题时唯一的依据：发布版没有控制台，记录必须落到用户目录的文件里。
use serde_json::json;
use sleepy_doll::{app::AppController, config::AppConfig};
use std::sync::Arc;

/// 用户目录就是配置文件的所在目录；记录写在它下面的 `log\` 里。
#[test]
fn an_operation_leaves_a_record_in_the_user_directory() {
    let root = tempfile::tempdir().unwrap();
    let user = root.path().join("user");
    std::fs::create_dir_all(&user).unwrap();
    let config_path = user.join("config.json");
    let config: AppConfig = serde_json::from_value(json!({
        "version":2,
        "activeModel":"local",
        "models":[{"id":"local","name":"local","protocol":"ollama-chat","model":"test","baseUrl":"http://127.0.0.1:1"}],
        "agent":{"systemPrompt":"test"},
        "bridge":{"enabled":false,"baseUrl":"http://127.0.0.1:1"},
        "storage":{"database":user.join("test.db")}
    }))
    .unwrap();
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

    sleepy_doll::logging::init(&user).unwrap();
    let app = Arc::new(AppController::load(&config_path).unwrap());
    // 关闭一个本来就没开的连接：用户能触发、不依赖任何外部程序，但同样经过
    // 界面开关那条入口。
    app.handle(
        "bridge.setEnabled",
        json!({"enabled": false}),
        Arc::new(|_, _| {}),
    )
    .unwrap();
    app.shutdown();

    let log = std::fs::read_to_string(user.join("log/sleepy-doll.log")).unwrap();
    assert!(log.contains("BetterGI 连接已关闭"), "{log}");
}
