//! Briefing Desk: a local, multimedia research-brief demo built on Orchest.
//! See `README.md` for the product spec. This binary is the v0.10 CLI
//! skeleton (issue 002) — see issue docs under `docs/iteration/v0_10/issues/`
//! for what each subsequent issue adds.

mod app;
mod fake_model;
mod media;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "briefing-desk", about = "Local multimedia research-brief demo")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a fresh briefing over a materials directory.
    Run {
        /// Directory containing .md/.txt notes, images and audio.
        #[arg(long)]
        materials: PathBuf,
        /// The research question to answer.
        #[arg(long)]
        question: String,
        /// Where to write the Markdown brief.
        #[arg(long)]
        output: PathBuf,
        /// Session identifier for later resume.
        #[arg(long)]
        session: Option<String>,
        /// Use deterministic fake model/ASR/TTS instead of live providers.
        #[arg(long)]
        fake: bool,
        /// Skip audio synthesis of the brief.
        #[arg(long)]
        no_tts: bool,
    },
    /// Resume a previous session and append a follow-up answer.
    Resume {
        /// Session identifier to resume.
        #[arg(long)]
        session: String,
        /// The follow-up question.
        #[arg(long)]
        question: String,
        /// Where to write the updated Markdown brief.
        #[arg(long)]
        output: PathBuf,
        /// Use deterministic fake model/ASR/TTS instead of live providers.
        #[arg(long)]
        fake: bool,
        /// Skip audio synthesis of the brief.
        #[arg(long)]
        no_tts: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            materials,
            question,
            output,
            session,
            fake,
            no_tts,
        } => {
            app::run(app::RunArgs {
                materials,
                question,
                output,
                session,
                fake,
                no_tts,
            })
            .await
        }
        Commands::Resume {
            session,
            question,
            output,
            fake,
            no_tts,
        } => {
            app::resume(app::ResumeArgs {
                session,
                question,
                output,
                fake,
                no_tts,
            })
            .await
        }
    }
}
