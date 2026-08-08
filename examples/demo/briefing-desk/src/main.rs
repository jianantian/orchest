//! Briefing Desk: a local, multimedia research-brief demo built on Orchest.
//! See `README.md` for the product spec. See issue docs under
//! `docs/archive/iteration/v0_10/issues/` for what each issue adds.

mod app;
mod eval;
mod execution;
mod harness;
mod media;
mod tools;

use std::path::PathBuf;
use std::process::ExitCode;

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
        /// Skip audio synthesis of the brief.
        #[arg(long)]
        no_tts: bool,
    },
    /// Eval Lab: run corpus cases or compare baseline/candidate.
    Eval {
        #[command(subcommand)]
        command: EvalCommands,
    },
}

#[derive(Subcommand)]
enum EvalCommands {
    /// Run eval cases for one or more splits and record sensitive artifacts.
    Run {
        /// Unique run label (directory under evals/runs/; never overwritten).
        #[arg(long)]
        label: String,
        /// Comma-separated splits: optimization, validation, scorecard.
        #[arg(long)]
        split: String,
        /// Required confirmation that sensitive trajectories will be recorded.
        #[arg(long)]
        record_sensitive: bool,
        /// Extra confirmation required for scorecard (sealed process contract).
        #[arg(long, default_value_t = false)]
        confirm_sealed: bool,
        /// Override runs root (default: package evals/runs).
        #[arg(long)]
        runs_dir: Option<PathBuf>,
        /// Override cases.json path.
        #[arg(long)]
        cases: Option<PathBuf>,
        /// Override fixtures directory.
        #[arg(long)]
        fixtures: Option<PathBuf>,
        /// Override session seeds directory.
        #[arg(long)]
        seeds: Option<PathBuf>,
        /// Materials directory for tool discovery (default: fixtures/research).
        #[arg(long)]
        materials: Option<PathBuf>,
        /// Git repo root for dirty-path preflight (default: workspace root).
        #[arg(long)]
        repo_root: Option<PathBuf>,
        /// Use built-in scripted model (offline / CI). Hidden from normal use.
        #[arg(long, hide = true)]
        scripted: bool,
    },
    /// Compare baseline and candidate run directories.
    Compare {
        /// Baseline run label.
        baseline: String,
        /// Candidate run label.
        candidate: String,
        /// Override runs root.
        #[arg(long)]
        runs_dir: Option<PathBuf>,
        /// Directory for compare JSON/Markdown reports.
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
    /// Export an allowlist-only, deterministic evidence bundle.
    ExportEvidence {
        baseline: String,
        candidate: String,
        #[arg(long)]
        runs_dir: Option<PathBuf>,
        #[arg(long)]
        out_dir: PathBuf,
        #[arg(long)]
        decision: String,
        #[arg(long, default_value = "not_run")]
        scorecard_state: String,
    },
    /// Verify a committed evidence bundle and recompute its comparison.
    VerifyEvidence { bundle: PathBuf },
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            materials,
            question,
            output,
            session,
            no_tts,
        } => match app::run(app::RunArgs {
            materials,
            question,
            output,
            session,
            no_tts,
        })
        .await
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        Commands::Resume {
            session,
            question,
            output,
            no_tts,
        } => match app::resume(app::ResumeArgs {
            session,
            question,
            output,
            no_tts,
        })
        .await
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        },
        Commands::Eval { command } => match command {
            EvalCommands::Run {
                label,
                split,
                record_sensitive,
                confirm_sealed,
                runs_dir,
                cases,
                fixtures,
                seeds,
                materials,
                repo_root,
                scripted,
            } => {
                eval::cli::cmd_eval_run(eval::cli::EvalRunCli {
                    label,
                    split,
                    record_sensitive,
                    confirm_sealed,
                    runs_dir,
                    cases,
                    fixtures,
                    seeds,
                    materials,
                    repo_root,
                    scripted,
                })
                .await
            }
            EvalCommands::Compare {
                baseline,
                candidate,
                runs_dir,
                out_dir,
            } => eval::cli::cmd_eval_compare(eval::cli::EvalCompareCli {
                baseline,
                candidate,
                runs_dir,
                out_dir,
            }),
            EvalCommands::ExportEvidence {
                baseline,
                candidate,
                runs_dir,
                out_dir,
                decision,
                scorecard_state,
            } => eval::cli::cmd_eval_export_evidence(eval::cli::EvalExportEvidenceCli {
                baseline,
                candidate,
                runs_dir,
                out_dir,
                decision,
                scorecard_state,
            }),
            EvalCommands::VerifyEvidence { bundle } => eval::cli::cmd_eval_verify_evidence(bundle),
        },
    }
}
