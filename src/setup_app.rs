//! 原生安装窗口与工作线程。页面状态不依赖 WebView2。
use flate2::read::DeflateDecoder;
use sleepy_doll::{setup, webview_runtime as runtime};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::time::Duration;
#[path = "setup_window.rs"]
mod setup_window;
use std::{
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
};
slint::include_modules!();

const MANIFEST: &[u8] = include_bytes!("../target/setup/payload.json");
const PAYLOAD: &[u8] = include_bytes!("../target/setup/payload.bin");
#[derive(Clone)]
struct Context {
    directory: PathBuf,
    version: String,
    update: bool,
    uninstall: bool,
    repair_runtime: bool,
    shortcut: bool,
    close_blockers: bool,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let _cleanup = Cleanup;
    let temporary = setup::scratch_root().join("tmp");
    std::fs::create_dir_all(&temporary)?;
    // 此时尚未创建窗口或工作线程，统一重定向后续依赖及子进程临时文件。
    unsafe {
        for key in ["TEMP", "TMP", "TMPDIR"] {
            std::env::set_var(key, &temporary);
        }
    }
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        return apply_update(args.get(2).ok_or("缺少待退出进程号")?.parse()?);
    }
    let repair_runtime = args.iter().any(|s| s == "--repair-runtime");
    let uninstall = setup::is_uninstall_mode() && !repair_runtime;
    let installed = setup::installed();
    let ctx = Arc::new(Mutex::new(Context {
        directory: if repair_runtime {
            std::env::current_exe()?
                .parent()
                .ok_or("安装目录不可用")?
                .to_path_buf()
        } else if uninstall {
            setup::uninstall_directory().ok_or("未找到安装位置")?
        } else {
            installed
                .as_ref()
                .map(|i| i.directory.clone())
                .unwrap_or_else(|| PathBuf::from(setup::default_directory()))
        },
        version: installed
            .as_ref()
            .and_then(|i| i.version.clone())
            .unwrap_or_default(),
        update: installed.is_some(),
        uninstall,
        repair_runtime,
        shortcut: true,
        close_blockers: false,
    }));
    let ui = create_window()?;
    show_form(&ui, &ctx.lock().unwrap());
    if repair_runtime {
        show_runtime(&ui);
    }
    bind(&ui, ctx);
    center(&ui);
    ui.show()?;
    ui.run()?;
    Ok(())
}

fn create_window() -> Result<SetupWindow, Box<dyn std::error::Error>> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .with_winit_window_attributes_hook(|attributes| {
            attributes
                .with_transparent(false)
                .with_theme(Some(slint::winit_030::winit::window::Theme::Dark))
        })
        .select()?;
    let ui = SetupWindow::new()?;
    ui.set_target_version(setup::version().into());
    ui.set_brand_icon(slint::Image::load_from_svg_data(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/setup-brand-dark.svg"
    )))?);
    ui.set_desktop_shortcut(true);
    install_window_chrome(ui.as_weak(), 10);
    Ok(ui)
}

fn install_window_chrome(weak: slint::Weak<SetupWindow>, retries: u8) {
    slint::Timer::single_shot(Duration::from_millis(16), move || {
        if let Some(ui) = weak.upgrade() {
            let handle = hwnd(&ui);
            if handle != 0 {
                if !setup_window::install(handle) {
                    log::error!("安装器标题栏初始化失败");
                }
            } else if retries > 0 {
                install_window_chrome(ui.as_weak(), retries - 1);
            }
        }
    });
}

fn reset(ui: &SetupWindow, state: &str) {
    ui.set_state_id(state.into());
    ui.set_status_title("".into());
    ui.set_description("".into());
    ui.set_note("".into());
    ui.set_error("".into());
    ui.set_detail("".into());
    ui.set_status_icon(0);
    ui.set_show_form(false);
    ui.set_show_progress(false);
    ui.set_show_download(false);
    ui.set_show_steps(false);
    ui.set_busy(false);
    ui.set_bad(false);
    ui.set_primary_enabled(true);
    ui.set_cancel_visible(true);
    ui.set_cancel_label("取消".into());
    ui.set_progress(-1.0);
    ui.set_progress_label("".into());
    ui.set_progress_caption(false);
    ui.set_rows(ModelRc::new(VecModel::default()));
}

fn rows(ui: &SetupWindow, values: &[(&str, String)]) {
    ui.set_rows(ModelRc::new(VecModel::from(
        values
            .iter()
            .map(|(label, value)| SetupRow {
                label: (*label).into(),
                value: value.into(),
            })
            .collect::<Vec<_>>(),
    )));
}

fn show_form(ui: &SetupWindow, c: &Context) {
    reset(ui, "form");
    ui.set_directory(c.directory.to_string_lossy().as_ref().into());
    ui.set_heading(
        format!(
            "{} Sleepy Doll",
            if c.uninstall {
                "卸载"
            } else if c.update {
                "更新"
            } else {
                "安装"
            }
        )
        .into(),
    );
    if c.uninstall {
        ui.set_description("将删除程序和全部用户数据。".into());
        rows(
            ui,
            &[
                ("安装位置", c.directory.display().to_string()),
                ("当前版本", c.version.clone()),
            ],
        );
        ui.set_primary_label("卸载".into());
    } else if c.update {
        rows(
            ui,
            &[
                ("安装位置", c.directory.display().to_string()),
                ("当前版本", c.version.clone()),
                ("更新至", setup::version().into()),
            ],
        );
        ui.set_primary_label("开始更新".into());
        if newer(&c.version) {
            ui.set_state_id("newer".into());
            ui.set_status_title("已安装较新版本".into());
            ui.set_status_icon(7);
            ui.set_primary_label("打开 Sleepy Doll".into());
        }
    } else {
        ui.set_show_form(true);
        ui.set_note("配置和会话记录也会保存在此文件夹中。".into());
        ui.set_primary_label("安装".into());
    }
}

fn newer(version: &str) -> bool {
    semver::Version::parse(version)
        .ok()
        .zip(semver::Version::parse(setup::version()).ok())
        .is_some_and(|(a, b)| a > b)
}

fn show_runtime(ui: &SetupWindow) {
    reset(ui, "runtime");
    let current = runtime::machine_version();
    ui.set_status_title(
        if current.is_some() {
            "需要升级 WebView2"
        } else {
            "需要安装 WebView2"
        }
        .into(),
    );
    ui.set_status_icon(if current.is_some() { 4 } else { 3 });
    if let Some(version) = current {
        rows(
            ui,
            &[
                ("当前版本", version),
                ("最低版本", runtime::MINIMUM_VERSION.into()),
                ("安装方式", "联网下载微软稳定版".into()),
            ],
        );
    } else {
        rows(
            ui,
            &[
                ("运行组件", "Microsoft Edge WebView2".into()),
                ("安装方式", "联网下载，整机安装".into()),
            ],
        );
    }
    ui.set_primary_label("安装并继续".into());
}

fn bind(ui: &SetupWindow, ctx: Arc<Mutex<Context>>) {
    let weak = ui.as_weak();
    let c = ctx.clone();
    ui.on_primary(move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.get_busy() {
            return;
        }
        match ui.get_state_id().as_str() {
            "done" | "newer" => {
                if c.lock().unwrap().uninstall {
                    let _ = slint::quit_event_loop();
                } else {
                    launch(&ui, &c.lock().unwrap());
                }
            }
            "runtime" | "runtime-failed" => start(&ui, c.clone(), true),
            "failed" => show_form(&ui, &c.lock().unwrap()),
            _ => {
                if !c.lock().unwrap().uninstall {
                    // 登记可能在窗口打开后变化；更新始终使用已登记目录。
                    if let Some(installed) = setup::installed() {
                        let mut state = c.lock().unwrap();
                        state.update = true;
                        state.directory = installed.directory;
                        state.version = installed.version.unwrap_or_default();
                    }
                    let mut state = c.lock().unwrap();
                    if !state.update {
                        state.directory =
                            PathBuf::from(setup::resolve_directory(ui.get_directory().as_str()));
                        state.shortcut = ui.get_desktop_shortcut();
                    }
                    if let Err(e) = setup::validate_directory(&state.directory.to_string_lossy()) {
                        log::warn!("安装位置无效：{e}");
                        ui.set_error("安装位置不可用，请选择其他文件夹。".into());
                        return;
                    }
                    if newer(&state.version) {
                        show_form(&ui, &state);
                        return;
                    }
                    if !state.update && setup::is_foreign_directory(&state.directory) {
                        ui.set_modal_title("此文件夹已有其他文件".into());
                        ui.set_modal_description("安装会写入程序文件，可能覆盖同名文件。".into());
                        ui.set_modal_detail(state.directory.display().to_string().into());
                        ui.set_modal_primary_label("仍然安装".into());
                        ui.set_modal_cancel_label("返回修改".into());
                        ui.set_modal_visible(true);
                        return;
                    }
                }
                continue_install(&ui, c.clone());
            }
        }
    });
    let weak = ui.as_weak();
    let c = ctx.clone();
    ui.on_modal_primary(move || {
        if let Some(ui) = weak.upgrade() {
            if ui.get_state_id() == "uninstall-blocked" {
                c.lock().unwrap().close_blockers = true;
            }
            ui.set_modal_visible(false);
            continue_install(&ui, c.clone());
        }
    });
    let weak = ui.as_weak();
    ui.on_modal_cancel(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_modal_visible(false);
        }
    });
    let weak = ui.as_weak();
    ui.on_directory_edited(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.set_error("".into());
        }
    });
    let weak = ui.as_weak();
    ui.on_browse(move || {
        if let Some(ui) = weak.upgrade() {
            let owner = hwnd(&ui);
            let initial = PathBuf::from(ui.get_directory().as_str());
            let weak = ui.as_weak();
            ui.set_busy(true);
            ui.set_primary_enabled(false);
            std::thread::spawn(move || {
                let result = setup::browse_for_directory(owner, "选择安装位置", &initial);
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_busy(false);
                    ui.set_primary_enabled(true);
                    match result {
                        Ok(Some(dir)) => {
                            ui.set_directory(dir.display().to_string().into());
                            ui.set_error("".into());
                        }
                        Ok(None) => {}
                        Err(e) => ui.set_error(e.to_string().into()),
                    }
                });
            });
        }
    });
    let weak = ui.as_weak();
    ui.on_open_download(move || {
        if let Some(ui) = weak.upgrade()
            && let Err(e) = setup::launch(std::path::Path::new(runtime::DOWNLOAD_PAGE))
        {
            ui.set_error(e.to_string().into());
        }
    });
    let weak = ui.as_weak();
    ui.on_cancel(move || {
        if let Some(ui) = weak.upgrade() {
            request_close(&ui);
        }
    });
    let weak = ui.as_weak();
    ui.on_dismiss(move || {
        if let Some(ui) = weak.upgrade() {
            request_close(&ui);
        }
    });
    let weak = ui.as_weak();
    ui.window().on_close_requested(move || {
        if weak.upgrade().is_some_and(|ui| ui.get_busy()) {
            slint::CloseRequestResponse::KeepWindowShown
        } else {
            slint::CloseRequestResponse::HideWindow
        }
    });
    let weak = ui.as_weak();
    ui.on_minimize(move || {
        if let Some(ui) = weak.upgrade() {
            ui.window().set_minimized(true);
        }
    });
}

fn request_close(ui: &SetupWindow) {
    if ui.get_busy() {
        return;
    }
    // 先隐藏窗口，再退出事件循环，关闭动作不等待后台缓存清理。
    if let Err(error) = ui.hide() {
        log::error!("关闭安装窗口失败：{error}");
    }
    if let Err(error) = slint::quit_event_loop() {
        log::error!("退出安装事件循环失败：{error}");
    }
}

fn continue_install(ui: &SetupWindow, ctx: Arc<Mutex<Context>>) {
    let context = ctx.lock().unwrap().clone();
    if context.uninstall && !context.close_blockers {
        match open_archive()
            .and_then(|archive| setup::uninstall::blocking_programs(&archive, &context.directory))
        {
            Ok(owners) if !owners.is_empty() => {
                ui.set_state_id("uninstall-blocked".into());
                ui.set_modal_title("关闭占用程序并卸载？".into());
                ui.set_modal_description("将请求以下程序正常退出，然后自动继续卸载。".into());
                ui.set_modal_detail(owners.join("\n").into());
                ui.set_modal_primary_label("关闭并卸载".into());
                ui.set_modal_cancel_label("取消".into());
                ui.set_modal_visible(true);
                return;
            }
            Err(error) => {
                ui.set_error(error.to_string().into());
                return;
            }
            _ => {}
        }
    }
    if !ctx.lock().unwrap().uninstall && runtime::requirement_message().is_some() {
        show_runtime(ui);
    } else {
        start(ui, ctx, false);
    }
}

fn start(ui: &SetupWindow, ctx: Arc<Mutex<Context>>, runtime_first: bool) {
    let c = ctx.lock().unwrap().clone();
    ctx.lock().unwrap().close_blockers = false;
    reset(ui, "busy");
    ui.set_busy(true);
    ui.set_primary_enabled(false);
    ui.set_cancel_visible(false);
    ui.set_show_progress(true);
    ui.set_status_title(
        if runtime_first {
            "正在准备运行组件"
        } else if c.uninstall {
            "正在卸载 Sleepy Doll"
        } else if c.update {
            "正在更新 Sleepy Doll"
        } else {
            "正在安装 Sleepy Doll"
        }
        .into(),
    );
    ui.set_primary_label("处理中…".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        if runtime_first {
            let w = weak.clone();
            if let Err(error) = runtime::ensure(&setup::scratch_root(), &mut |stage| {
                let verify = stage == "verify";
                let _ = w.upgrade_in_event_loop(move |ui| {
                    ui.set_status_title(
                        if verify {
                            "正在确认运行环境"
                        } else {
                            "正在准备运行组件"
                        }
                        .into(),
                    );
                });
            }) {
                log::error!("WebView2 补装失败：{error}");
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    reset(&ui, "runtime-failed");
                    ui.set_status_title("WebView2 安装未完成".into());
                    ui.set_status_icon(2);
                    ui.set_bad(true);
                    ui.set_show_download(true);
                    ui.set_primary_label("重试".into());
                });
                return;
            }
        }
        let outcome = if c.repair_runtime {
            Ok(())
        } else {
            open_archive().and_then(|archive| {
                let w = weak.clone();
                let uninstall = c.uninstall;
                let update = c.update;
                let mut report = move |p: f64, _message: &str| {
                    let _ = w.upgrade_in_event_loop(move |ui| {
                        ui.set_status_title(
                            if uninstall {
                                "正在卸载 Sleepy Doll"
                            } else if update {
                                "正在更新 Sleepy Doll"
                            } else {
                                "正在安装 Sleepy Doll"
                            }
                            .into(),
                        );
                        ui.set_progress((p.clamp(0.0, 1.0) * 100.0) as f32);
                        ui.set_progress_caption(true);
                    });
                };
                if c.uninstall {
                    if c.close_blockers {
                        setup::uninstall::close_blocking_programs(&archive, &c.directory)?;
                    }
                    setup::uninstall(&archive, &c.directory, &mut report)
                } else if c.update {
                    setup::update(&archive, &c.directory, &mut report)
                } else {
                    setup::install(&archive, &c.directory, c.shortcut, &mut report)
                }
            })
        };
        let _ = weak.upgrade_in_event_loop(move |ui| match outcome {
            Ok(()) => {
                reset(&ui, "done");
                ui.set_status_icon(1);
                ui.set_status_title(
                    if c.uninstall {
                        "已卸载 Sleepy Doll"
                    } else if c.repair_runtime {
                        "WebView2 已就绪"
                    } else if c.update {
                        "更新完成"
                    } else {
                        "安装完成"
                    }
                    .into(),
                );
                ui.set_description(
                    if c.uninstall {
                        "程序和用户数据已删除。"
                    } else {
                        ""
                    }
                    .into(),
                );
                ui.set_primary_label(
                    if c.uninstall {
                        "关闭"
                    } else {
                        "启动 Sleepy Doll"
                    }
                    .into(),
                );
                ui.set_cancel_visible(!c.uninstall);
                ui.set_cancel_label("关闭".into());
            }
            Err(e) => {
                reset(&ui, "failed");
                ui.set_bad(true);
                ui.set_status_icon(2);
                ui.set_status_title(
                    if c.uninstall {
                        "卸载未完成"
                    } else if c.update {
                        "更新未完成"
                    } else {
                        "安装未完成"
                    }
                    .into(),
                );
                ui.set_error("操作未完成，请检查详细信息后重试。".into());
                ui.set_detail(e.to_string().into());
                ui.set_primary_label("返回重试".into());
                ui.set_cancel_label("关闭".into());
            }
        });
    });
}

fn launch(ui: &SetupWindow, c: &Context) {
    match setup::launch(&c.directory.join(setup::EXECUTABLE)) {
        Ok(()) => {
            let _ = slint::quit_event_loop();
        }
        Err(e) => ui.set_error(e.to_string().into()),
    }
}
fn open_archive() -> Result<setup::Archive, setup::Error> {
    let mut bytes = Vec::new();
    DeflateDecoder::new(PAYLOAD)
        .read_to_end(&mut bytes)
        .map_err(|e| setup::Error::message(format!("载荷无法解压：{e}")))?;
    setup::Archive::new(MANIFEST, bytes)
}
struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        cleanup();
    }
}
fn cleanup() {
    let scratch = setup::scratch_root();
    if scratch.exists() {
        let mut removals = vec![setup::Removal::Tree(scratch.clone())];
        for parent in scratch.ancestors().skip(1).take(2) {
            removals.push(setup::Removal::Empty(parent.into()));
        }
        if let Err(e) = setup::cleanup_after_exit(&removals) {
            native_message(&e.to_string());
        }
    }
}

fn apply_update(pid: u32) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, WaitForSingleObject},
        };
        let handle = OpenProcess(0x00100000, 0, pid);
        if !handle.is_null() {
            let result = WaitForSingleObject(handle, 60_000);
            CloseHandle(handle);
            if result != 0 {
                return Err("原程序未退出，更新已停止".into());
            }
        }
    }
    let installed = setup::installed().ok_or("未找到原安装目录")?;
    if runtime::requirement_message().is_some() {
        runtime::ensure(&setup::scratch_root(), &mut |_| {}).map_err(setup::Error::message)?;
    }
    setup::update(&open_archive()?, &installed.directory, &mut |_, _| {})?;
    setup::launch(&installed.directory.join(setup::EXECUTABLE))?;
    Ok(())
}

#[cfg(windows)]
fn hwnd(ui: &SetupWindow) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    ui.window()
        .window_handle()
        .window_handle()
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
            _ => None,
        })
        .unwrap_or(0)
}
#[cfg(not(windows))]
fn hwnd(_: &SetupWindow) -> isize {
    0
}
#[cfg(windows)]
fn center(ui: &SetupWindow) {
    unsafe {
        use windows_sys::Win32::{
            Foundation::{POINT, RECT},
            Graphics::Gdi::{
                GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
                MonitorFromWindow,
            },
            UI::{
                HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
                WindowsAndMessaging::{GetCursorPos, GetWindowRect},
            },
        };
        let handle = hwnd(ui) as windows_sys::Win32::Foundation::HWND;
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let monitor = if handle.is_null() {
            MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST)
        } else {
            MonitorFromWindow(handle, MONITOR_DEFAULTTONEAREST)
        };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..std::mem::zeroed()
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return;
        }
        let mut dpi_x = 96;
        let mut dpi_y = 96;
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let mut rect: RECT = std::mem::zeroed();
        let (width, height) = if !handle.is_null() && GetWindowRect(handle, &mut rect) != 0 {
            (rect.right - rect.left, rect.bottom - rect.top)
        } else {
            ((720 * dpi_x / 96) as i32, (460 * dpi_y / 96) as i32)
        };
        let work = info.rcWork;
        // 使用 Slint 的窗口位置 API：窗口尚未创建时也会保存位置，供首帧使用。
        ui.window().set_position(slint::PhysicalPosition::new(
            work.left + (work.right - work.left - width) / 2,
            work.top + (work.bottom - work.top - height) / 2,
        ));
    }
}
#[cfg(not(windows))]
fn center(_: &SetupWindow) {}
pub fn native_message(message: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
        let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
        let title: Vec<u16> = "Sleepy Doll 安装程序\0".encode_utf16().collect();
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
    #[cfg(not(windows))]
    eprintln!("{message}");
}

/// 开发入口只绘制隔离页面，不执行安装、下载、卸载或登记。
#[cfg(debug_assertions)]
#[allow(dead_code)]
pub fn preview() -> Result<(), Box<dyn std::error::Error>> {
    let ui = create_window()?;
    let c = Context {
        directory: PathBuf::from("D:\\Sleepy Doll"),
        version: "0.1.2".into(),
        update: false,
        uninstall: false,
        repair_runtime: false,
        shortcut: true,
        close_blockers: false,
    };
    show_form(&ui, &c);
    // 预览与正式窗口共用窗口操作回调；仅屏蔽会改动安装内容的操作。
    bind(&ui, Arc::new(Mutex::new(c.clone())));
    ui.on_primary(|| {});
    ui.on_modal_primary(|| {});
    ui.on_open_download(|| {});
    ui.on_browse(|| {});
    center(&ui);
    ui.show()?;
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--check-interaction") {
        let action = args.get(2).cloned().unwrap_or_else(|| "close".into());
        let modal = action.starts_with("modal-");
        if modal {
            ui.set_modal_visible(true);
        }
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(300), move || {
            use slint::platform::{PointerEventButton, WindowEvent};
            let ui = weak.upgrade().unwrap();
            #[cfg(windows)]
            if action == "modal-drag" {
                unsafe {
                    use windows_sys::Win32::{
                        Foundation::RECT,
                        UI::WindowsAndMessaging::{
                            GetWindowRect, HTCAPTION, HTCLIENT, SendMessageW, WM_NCHITTEST,
                        },
                    };
                    let handle = hwnd(&ui) as _;
                    let mut r: RECT = std::mem::zeroed();
                    assert_ne!(GetWindowRect(handle, &mut r), 0);
                    let scale = ui.window().scale_factor();
                    let screen_point = |x: f32, y: f32| -> isize {
                        let x = r.left + (x * scale).round() as i32;
                        let y = r.top + (y * scale).round() as i32;
                        ((y as u16 as usize) << 16 | x as u16 as usize) as isize
                    };
                    assert_eq!(
                        SendMessageW(handle, WM_NCHITTEST, 0, screen_point(100.0, 16.0)),
                        HTCAPTION as isize
                    );
                    assert_eq!(
                        SendMessageW(handle, WM_NCHITTEST, 0, screen_point(697.0, 16.0)),
                        HTCLIENT as isize
                    );
                }
                let _ = slint::quit_event_loop();
                return;
            }
            let (x, y) = if action == "cancel" {
                (577.0, 426.0)
            } else if action == "minimize" {
                (651.0, 16.0)
            } else {
                (697.0, 16.0)
            };
            let position = slint::LogicalPosition::new(x, y);
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
            if action == "minimize" {
                let w = ui.as_weak();
                slint::Timer::single_shot(Duration::from_millis(200), move || {
                    assert!(w.upgrade().unwrap().window().is_minimized());
                    let _ = slint::quit_event_loop();
                });
            }
        });
        slint::Timer::single_shot(Duration::from_secs(3), || {
            eprintln!("窗口操作未生效");
            std::process::exit(3);
        });
        ui.run()?;
        return Ok(());
    }
    if args.get(1).is_some_and(|s| s == "--check-browse") {
        let weak = ui.as_weak();
        slint::Timer::single_shot(Duration::from_millis(300), move || {
            let ui = weak.upgrade().unwrap();
            let owner = hwnd(&ui);
            std::thread::spawn(move || {
                let result = setup::browse_for_directory(
                    owner,
                    "选择安装位置",
                    std::path::Path::new("E:\\BetterGIProject\\Sleepy-Doll"),
                );
                println!("folder dialog result: {result:?}");
                assert!(result.is_ok());
                let _ = slint::quit_event_loop();
            });
        });
        ui.run()?;
        return Ok(());
    }
    if let Some(path) = args.get(1) {
        let path = PathBuf::from(path);
        let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/.tmp/installer-native")
            .canonicalize()?;
        let path = path.canonicalize()?;
        if !path.starts_with(allowed) {
            return Err("预览截图必须在本任务验证目录内".into());
        }
        let weak = ui.as_weak();
        let timer = slint::Timer::default();
        let mut frame = 0;
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(300),
            move || {
                if let Some(ui) = weak.upgrade() {
                    match frame {
                        0 => {}
                        1 => ui.set_error("不能安装到系统目录，请选择其他位置。".into()),
                        2 => ui.set_error("".into()),
                        3 => {
                            let mut ctx = c.clone();
                            ctx.update = true;
                            show_form(&ui, &ctx);
                        }
                        4 => {
                            let mut ctx = c.clone();
                            ctx.uninstall = true;
                            show_form(&ui, &ctx);
                        }
                        5 => {
                            reset(&ui, "runtime-failed");
                            ui.set_status_title("WebView2 下载失败".into());
                            ui.set_status_icon(2);
                            ui.set_show_download(true);
                            ui.set_primary_label("重试".into());
                        }
                        _ => {
                            let _ = slint::quit_event_loop();
                            return;
                        }
                    }
                    let shot = ui.as_weak();
                    let file = path.join(format!("state-{frame}.png"));
                    slint::Timer::single_shot(Duration::from_millis(220), move || {
                        if let Some(ui) = shot.upgrade() {
                            let buffer = ui.window().take_snapshot().unwrap();
                            let mut encoder = png::Encoder::new(
                                std::fs::File::create(file).unwrap(),
                                buffer.width(),
                                buffer.height(),
                            );
                            encoder.set_color(png::ColorType::Rgba);
                            encoder.set_depth(png::BitDepth::Eight);
                            encoder
                                .write_header()
                                .unwrap()
                                .write_image_data(buffer.as_bytes())
                                .unwrap();
                        }
                    });
                    frame += 1;
                }
            },
        );
        ui.run()?;
    } else {
        ui.run()?;
    }
    Ok(())
}
