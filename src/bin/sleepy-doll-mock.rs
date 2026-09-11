use std::{env, path::PathBuf, sync::Arc, thread};

use sleepy_doll::{
    app::AppController,
    mock::{MockBackend, MockFaults},
};

/// The development fixture at the repository root. The mock is a development-only
/// tool excluded from release builds, so baking the manifest directory in is safe
/// and keeps its data next to the fixture that describes it instead of wherever
/// the process happened to be started from.
const CONFIG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/sleepy-doll.mock.config.json");

fn main() {
    if let Err(error) = run() {
        eprintln!("Sleepy Doll mock failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(argument) = env::args_os().nth(1) {
        match argument.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                println!("{}", sleepy_doll::config::USAGE);
                return Ok(());
            }
            "-V" | "--version" => {
                println!("{}", sleepy_doll::config::VERSION);
                return Ok(());
            }
            other => {
                return Err(format!(
                    "sleepy-doll-mock 不接受命令行参数（收到 {other}）；它固定读取 {CONFIG}"
                )
                .into());
            }
        }
    }
    let config = PathBuf::from(CONFIG);
    sleepy_doll::config::seed(&config)?;
    let settings = sleepy_doll::config::AppConfig::load(&config)?;
    let model = settings.active().clone();
    let backend = MockBackend::start("127.0.0.1:47124")?;
    let model_delay_ms = env::var("SLEEPY_DOLL_MOCK_MODEL_DELAY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(320);
    let stream_chunk_delay_ms = env::var("SLEEPY_DOLL_MOCK_CHUNK_DELAY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(80);
    backend.set_faults(MockFaults {
        model_delay_ms,
        stream_chunk_delay_ms,
        ..MockFaults::default()
    });
    let controller = Arc::new(AppController::load(&config)?);
    backend.set_ipc_handler(move |method, params| {
        controller.handle(method, params, Arc::new(|_, _| {}))
    });
    println!("Sleepy Doll mock backend: {}", backend.base_url());
    println!("UI preview: http://localhost:5173/");
    println!(
        "Model timing: ~{model_delay_ms} ms before the first token, ~{stream_chunk_delay_ms} ms per chunk (jittered, with punctuation pauses and occasional stalls; set 0 to disable)"
    );
    println!("Configuration: {}", config.display());
    // The mock endpoints are local, but browser preview IPC forwards model calls
    // to whatever the active model is. Say so instead of claiming offline use.
    println!("Active model: {} · {}", model.name, model.base_url);
    loop {
        thread::park();
    }
}
