//! Music Gift demo - axum server bootstrap.
//!
//! A "Moment"-style AI music gift app: chat with an LLM to craft personalized
//! lyrics, submit to Mureka for music generation, and share the resulting gift.
//!
//! See `README.md` for the full product spec.

mod agent;
mod config;
mod error;
mod gift;
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
    /// Directory for SQLite DB, photos, and audio files (default: ./data).
    #[arg(long, default_value = "data")]
    data_dir: PathBuf,

    /// Directory containing built frontend assets (default: frontend/dist).
    #[arg(long, default_value = "frontend/dist")]
    static_dir: PathBuf,

    /// Port to listen on (overrides MUSIC_GIFT_PORT env var).
    #[arg(long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Load .env if present (for local dev convenience).
    let _ = dotenv_optional();

    let cli = Cli::parse();
    let config = config::load_config()?;

    // Ensure data directory exists
    std::fs::create_dir_all(&cli.data_dir)?;
    std::fs::create_dir_all(cli.data_dir.join("countdown"))?;

    // Open SQLite gift store
    let db_path = cli.data_dir.join("gifts.db");
    let gift_store = gift::GiftStore::open(&db_path.to_string_lossy())?;

    let port = cli.port.unwrap_or(config.port);
    let static_dir = if cli.static_dir.exists() {
        Some(cli.static_dir)
    } else {
        None
    };

    let state = AppState {
        chat_model: config.chat_model,
        gen_task: config.gen_task,
        gift_store,
        data_dir: cli.data_dir,
    };

    let app = build_router(state, static_dir);

    let addr = format!("0.0.0.0:{port}");
    println!("music-gift listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Minimal `.env` loader: reads `.env` in the CWD and sets vars that aren't
/// already set. No dependency on dotenv crate.
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
