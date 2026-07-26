//! Music Gift demo - axum server bootstrap.

mod agent;
mod auth;
mod config;
mod error;
mod gift;
mod lrc;
mod prompts;
mod routes;
mod state;
mod tools;

use std::path::PathBuf;

use clap::Parser;

use routes::build_router;
use state::AppState;

#[derive(Parser)]
#[command(name = "music-gift", about = "AI music gift demo on the Orchest SDK")]
struct Cli {
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,
    #[arg(long, default_value = "frontend/dist")]
    static_dir: PathBuf,
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Structured logging for the generation pipeline (failures used to be
    // eprintln-only or fully silent). Logs go to stderr; stdout stays
    // reserved for the "music-gift listening on ..." line tests/smoke.rs
    // parses. Defaults to this crate's info+; RUST_LOG overrides.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("music_gift=info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let _ = dotenv_optional();
    let cli = Cli::parse();
    let config = config::load_config()?;

    std::fs::create_dir_all(&cli.data_dir)?;
    std::fs::create_dir_all(cli.data_dir.join("countdown"))?;
    let skills_dir = std::env::current_dir()?.join("skills");
    if !skills_dir.exists() {
        eprintln!(
            "[music-gift] skills directory not found at {:?}, agent will lack lyrics skill",
            skills_dir
        );
    }

    let db_path = cli.data_dir.join("gifts.db");
    let gift_store = gift::GiftStore::open(&db_path.to_string_lossy())?;

    // Auth shares gifts.db (AuthStore::open runs `ALTER TABLE gifts ADD COLUMN
    // creator_id`, so it must see the gifts table). Opened after gift_store so
    // that table already exists; a separate connection to the same file.
    // busy_timeout matches GiftStore::open — see the comment there.
    let auth_raw = rusqlite::Connection::open(&db_path)?;
    auth_raw.busy_timeout(std::time::Duration::from_secs(5))?;
    let auth_conn = std::sync::Arc::new(std::sync::Mutex::new(auth_raw));
    let auth_store = auth::AuthStore::open(auth_conn)?;

    let port = cli.port.unwrap_or(config.port);
    let static_dir = if cli.static_dir.exists() {
        Some(cli.static_dir)
    } else {
        None
    };

    let state = AppState {
        chat_model: config.chat_model,
        music_prompt_model: config.music_prompt_model,
        skills_dir,
        gen_task: config.gen_task,
        music_provider: config.music_provider,
        gift_store,
        auth_store,
        data_dir: cli.data_dir,
        countdown_tool: config.countdown_tool,
    };
    let app = build_router(state, static_dir);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    // Print only after binding: with MUSIC_GIFT_PORT=0 the OS picks the port,
    // so the pre-bind `addr` would announce ":0" instead of the real one.
    // tests/smoke.rs parses this line to find the server.
    println!("music-gift listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app).await?;
    Ok(())
}

fn dotenv_optional() -> std::io::Result<()> {
    let content = match std::fs::read_to_string(".env") {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
    Ok(())
}
