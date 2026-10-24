#[path = "../setup_app.rs"]
#[allow(dead_code)]
mod setup_app;
fn main() {
    #[cfg(debug_assertions)]
    setup_app::preview().expect("native installer preview");
    #[cfg(not(debug_assertions))]
    panic!("安装器预览仅用于开发构建");
}
