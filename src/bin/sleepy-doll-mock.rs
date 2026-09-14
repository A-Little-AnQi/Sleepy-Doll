use std::{env, path::PathBuf, sync::Arc, thread};

use sleepy_doll::{
    app::AppController,
    model::mock::{MockBackend, MockFaults},
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
    // 录制的对话由 scripts/build-demo-conversation.py 与前端脚本一起生成。
    // 有就按序回放，没有就退回按关键词选场景的默认行为。
    let replay_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".sleepy-doll/replay.json");
    match std::fs::read_to_string(&replay_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
        Some(fixture) => {
            let prompt = fixture["prompt"].as_str().unwrap_or_default().to_owned();
            let turns = fixture["turns"].as_array().cloned().unwrap_or_default();
            println!("Replay: {} 轮 ← {}", turns.len(), replay_path.display());
            backend.set_replay(prompt, turns);
        }
        None => println!("Replay: 无（{} 不存在）", replay_path.display()),
    }
    // 文件类工具直接读磁盘，让回放里的文件读取与录制时一致。
    if let Ok(host) = env::var("SLEEPY_DOLL_MOCK_BGI_USER") {
        println!("BGI User: {host}");
        backend.set_bgi_user_path(host);
    }

    // 「模拟对话」开演前清掉上一次的会话：每次演都新建一条会把侧栏堆满。
    let database = settings.storage.database.clone();
    backend.set_demo_reset(move |conversation| {
        let connection = rusqlite::Connection::open(&database)?;
        // 先清引用方再清被引用方：runtime_input_keys 引用 runtime_runs，
        // runtime_message_owners 同时引用 runs 与 messages。
        connection.execute(
            "DELETE FROM runtime_input_keys WHERE run_id IN \
             (SELECT id FROM runtime_runs WHERE conversation_id=?1)",
            [conversation],
        )?;
        connection.execute(
            "DELETE FROM runtime_events WHERE conversation_id=?1",
            [conversation],
        )?;
        connection.execute(
            "DELETE FROM runtime_message_owners WHERE message_id IN \
             (SELECT id FROM messages WHERE conversation_id=?1) OR run_id IN \
             (SELECT id FROM runtime_runs WHERE conversation_id=?1)",
            [conversation],
        )?;
        connection.execute(
            "DELETE FROM runtime_runs WHERE conversation_id=?1",
            [conversation],
        )?;
        // tool_calls 对 conversations 有外键，漏掉它会 FOREIGN KEY constraint failed。
        connection.execute(
            "DELETE FROM tool_calls WHERE conversation_id=?1",
            [conversation],
        )?;
        connection.execute(
            "DELETE FROM messages WHERE conversation_id=?1",
            [conversation],
        )?;
        connection.execute("DELETE FROM conversations WHERE id=?1", [conversation])?;
        Ok(())
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
