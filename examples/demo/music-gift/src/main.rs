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
    let _ = dotenv_optional();
    let cli = Cli::parse();
    let config = config::load_config()?;

    std::fs::create_dir_all(&cli.data_dir)?;
    std::fs::create_dir_all(cli.data_dir.join("countdown"))?;
    let skills_dir = std::env::current_dir()?.join("skills");
    if !skills_dir.exists() {
        eprintln!("[music-gift] skills directory not found at {:?}, agent will lack lyrics skill", skills_dir);
    }

    let db_path = cli.data_dir.join("gifts.db");
    let gift_store = gift::GiftStore::open(&db_path.to_string_lossy())?;

    // Auth shares gifts.db (AuthStore::open runs `ALTER TABLE gifts ADD COLUMN
    // creator_id`, so it must see the gifts table). Opened after gift_store so
    // that table already exists; a separate connection to the same file.
    let auth_conn = std::sync::Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&db_path)?));
    let auth_store = auth::AuthStore::open(auth_conn)?;

    let port = cli.port.unwrap_or(config.port);
    let static_dir = if cli.static_dir.exists() {
        Some(cli.static_dir)
    } else {
        None
    };

    let state = AppState {
        chat_model: config.chat_model,
        countdown_model: config.countdown_model,
        music_prompt_model: config.music_prompt_model,
        skills_dir,
        gen_task: config.gen_task,
        gift_store,
        auth_store,
        data_dir: cli.data_dir,
        countdown_tool: config.countdown_tool,
    };
    let app = build_router(state, static_dir);

    let addr = format!("0.0.0.0:{port}");
    println!("music-gift listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
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
