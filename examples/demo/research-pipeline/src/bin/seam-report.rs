use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use research_pipeline_demo::findings::{is_report_path, render_markdown, validate_json};

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
            let output_path = resolve_report_path(&out)?;
            let rendered = render_markdown(&load(&findings)?)?;
            fs::write(&output_path, rendered)
                .with_context(|| format!("write {}", out.display()))?;
            println!("rendered seam report: {}", out.display());
        }
        Command::Check { findings, report } => {
            let report_path = resolve_report_path(&report)?;
            let expected = render_markdown(&load(&findings)?)?;
            let actual = fs::read_to_string(&report_path)
                .with_context(|| format!("read {}", report.display()))?;
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

fn resolve_report_path(path: &Path) -> Result<PathBuf> {
    let manifest_dir = fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
        .context("resolve Research Pipeline manifest directory")?;
    let repository_root = manifest_dir
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .context("Research Pipeline manifest is not nested below a repository root")?;
    resolve_report_path_from_root(&repository_root, path)
}

fn resolve_report_path_from_root(repository_root: &Path, path: &Path) -> Result<PathBuf> {
    let display = path.to_string_lossy();
    if !is_report_path(&display) {
        bail!(
            "report output must be docs/review/v0_11_seam_gap_analysis.md, got {}",
            path.display()
        );
    }
    let repository_root =
        fs::canonicalize(repository_root).context("resolve Research Pipeline repository root")?;
    if !repository_root.join("Cargo.toml").is_file() || !repository_root.join(".git").exists() {
        bail!("Research Pipeline repository root markers are missing");
    }
    let unresolved = repository_root.join(path);
    let parent = unresolved
        .parent()
        .context("canonical report path has no parent")?;
    let parent = fs::canonicalize(parent).context("resolve canonical report parent")?;
    if !parent.starts_with(&repository_root) {
        bail!("canonical report parent escapes repository root");
    }
    let file_name = unresolved
        .file_name()
        .context("canonical report path has no file name")?;
    let resolved = parent.join(file_name);
    match fs::symlink_metadata(&resolved) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("canonical report file must not be a symlink")
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| format!("inspect {}", path.display()));
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_resolution_is_repository_root_relative() {
        let repository = tempfile::tempdir().expect("temporary repository");
        fs::write(repository.path().join("Cargo.toml"), "[workspace]\n")
            .expect("write repository Cargo.toml");
        fs::write(repository.path().join(".git"), "gitdir: elsewhere\n")
            .expect("write repository marker");
        fs::create_dir_all(repository.path().join("docs/review"))
            .expect("create canonical report parent");

        let resolved = resolve_report_path_from_root(
            repository.path(),
            Path::new("docs/review/v0_11_seam_gap_analysis.md"),
        )
        .expect("canonical report path should resolve");

        assert_eq!(
            resolved,
            fs::canonicalize(repository.path())
                .expect("canonical temporary repository")
                .join("docs/review/v0_11_seam_gap_analysis.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn report_resolution_rejects_a_symlinked_parent_escape() {
        use std::os::unix::fs::symlink;

        let repository = tempfile::tempdir().expect("temporary repository");
        let outside = tempfile::tempdir().expect("temporary outside directory");
        fs::write(repository.path().join("Cargo.toml"), "[workspace]\n")
            .expect("write repository Cargo.toml");
        fs::write(repository.path().join(".git"), "gitdir: elsewhere\n")
            .expect("write repository marker");
        fs::create_dir_all(repository.path().join("docs")).expect("create repository docs parent");
        symlink(outside.path(), repository.path().join("docs/review"))
            .expect("create escaping report parent symlink");

        let error = resolve_report_path_from_root(
            repository.path(),
            Path::new("docs/review/v0_11_seam_gap_analysis.md"),
        )
        .expect_err("symlinked report parent must be rejected");

        assert!(
            error.to_string().contains("escapes repository root"),
            "unexpected error: {error:#}"
        );
    }
}
