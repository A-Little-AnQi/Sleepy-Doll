use std::{env, sync::Arc, thread};

use sleepy_doll::{
    app::AppController,
    mock::{MockBackend, MockFaults},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("Sleepy Doll mock failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = sleepy_doll::config::resolve_path();
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
