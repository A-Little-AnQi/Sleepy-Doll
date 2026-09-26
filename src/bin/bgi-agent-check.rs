//! 隔离验收用 JSON 行驱动器，复用实际 AppController/Supervisor，不启动桌面窗口。
use serde_json::{Value, json};
use sleepy_doll::app::AppController;
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os().nth(1).ok_or("需要隔离配置路径")?;
    let controller = Arc::new(AppController::load(path)?);
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        if value["method"] == "shutdown" {
            controller.shutdown();
            break;
        }
        let result = controller.handle(
            value["method"].as_str().unwrap_or(""),
            value["params"].clone(),
            Arc::new(|_, _| {}),
        );
        let response = match result {
            Ok(result) => json!({"ok":true,"result":result}),
            Err(error) => json!({"ok":false,"error":error.user_message()}),
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    controller.shutdown();
    Ok(())
}
