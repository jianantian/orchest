use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use research_pipeline_demo::findings::{is_safe_repository_path, render_markdown, validate_json};

#[derive(Debug, Parser)]
#[command(
    name = "seam-report",
    about = "Validate and render Research Pipeline evidence"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Validate {
        #[arg(long)]
        findings: PathBuf,
    },
    Render {
        #[arg(long)]
        findings: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    Check {
        #[arg(long)]
        findings: PathBuf,
        #[arg(long)]
        report: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Validate { findings } => {
            load(&findings)?;
            println!("findings contract is valid: {}", findings.display());
        }
        Command::Render { findings, out } => {
            validate_output_path(&out)?;
            let rendered = render_markdown(&load(&findings)?)?;
            fs::write(&out, rendered).with_context(|| format!("write {}", out.display()))?;
            println!("rendered seam report: {}", out.display());
        }
        Command::Check { findings, report } => {
            let expected = normalize(&render_markdown(&load(&findings)?)?);
            let actual = fs::read_to_string(&report)
                .with_context(|| format!("read {}", report.display()))
                .map(|contents| normalize(&contents))?;
            if actual != expected {
                bail!("seam report is stale: {}", report.display());
            }
            println!("seam report is current: {}", report.display());
        }
    }
    Ok(())
}

fn load(path: &Path) -> Result<research_pipeline_demo::findings::FindingsDocument> {
    let contents = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    validate_json(&contents).map_err(Into::into)
}

fn validate_output_path(path: &Path) -> Result<()> {
    let display = path.to_string_lossy();
    if is_safe_repository_path(&display) {
        Ok(())
    } else {
        bail!("unsafe report output path: {}", path.display());
    }
}

fn normalize(contents: &str) -> String {
    contents.replace("\r\n", "\n")
}
