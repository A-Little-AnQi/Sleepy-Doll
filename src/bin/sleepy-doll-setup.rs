#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path = "../setup_app.rs"]
mod setup_app;
fn main() {
    if let Err(error) = setup_app::run() {
        setup_app::native_message(&format!("安装程序无法启动：{error}"));
        std::process::exit(1);
    }
}
