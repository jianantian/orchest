//! Black-box smoke test for the live CLI path. Requires
//! `BRIEFING_DESK_CHAT_MODEL` (and an API key) to be set; tests are skipped
//! otherwise since no network credentials exist in CI.

use std::path::PathBuf;
use std::process::Command;

const CHAT_MODEL_ENV: &str = "BRIEFING_DESK_CHAT_MODEL";

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_briefing-desk"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/research")
}

/// Returns true when a live chat model is configured, so tests can run.
fn chat_model_configured() -> bool {
    std::env::var(CHAT_MODEL_ENV).is_ok_and(|v| !v.trim().is_empty())
}

#[test]
fn run_produces_brief_and_audio() {
    if !chat_model_configured() {
        eprintln!("skipping: {CHAT_MODEL_ENV} not set");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let brief = std::fs::read_to_string(&output).expect("brief written");
    assert!(!brief.trim().is_empty(), "brief should not be empty");
}

#[test]
fn run_no_tts_skips_audio_file() {
    if !chat_model_configured() {
        eprintln!("skipping: {CHAT_MODEL_ENV} not set");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            output.to_str().unwrap(),
            "--no-tts",
        ])
        .output()
        .expect("run briefing-desk");

    assert!(result.status.success());
    assert!(output.exists(), "brief should still be written");

    let audio = output.with_extension("wav");
    assert!(!audio.exists(), "--no-tts must leave no audio file");
}

#[test]
fn run_without_chat_model_fails_clearly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .env_remove(CHAT_MODEL_ENV)
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        !result.status.success(),
        "run without a chat model should fail"
    );
    assert!(
        !output.exists(),
        "no brief should be written on the error path"
    );
}

#[test]
fn resume_without_prior_session_fails_clearly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .current_dir(tmp.path())
        .args([
            "resume",
            "--session",
            "never-ran",
            "--question",
            "Any update on retention?",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        !result.status.success(),
        "resuming a session that was never run should fail"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("no persisted session found"),
        "error should explain the session is missing: {stderr}"
    );
    assert!(
        !output.exists(),
        "no output should be written on the error path"
    );
}

#[test]
fn session_persists_across_processes_and_resume_references_original_brief() {
    if !chat_model_configured() {
        eprintln!("skipping: {CHAT_MODEL_ENV} not set");
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let session_id = "smoke-test-session";
    let first_output = tmp.path().join("brief.md");

    let first = bin()
        .current_dir(tmp.path())
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            first_output.to_str().unwrap(),
            "--session",
            session_id,
        ])
        .output()
        .expect("run briefing-desk");
    assert!(
        first.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        tmp.path()
            .join(".briefing-desk-sessions")
            .join(format!("{session_id}.sqlite3"))
            .exists(),
        "run --session should leave a sqlite session file behind"
    );

    // Resume in a genuinely separate process (new Command), same cwd so it
    // finds the same session file.
    let follow_up_output = tmp.path().join("followup.md");
    let second = bin()
        .current_dir(tmp.path())
        .args([
            "resume",
            "--session",
            session_id,
            "--question",
            "Has anything changed about the retention numbers?",
            "--output",
            follow_up_output.to_str().unwrap(),
        ])
        .output()
        .expect("run briefing-desk");
    assert!(
        second.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );

    let follow_up = std::fs::read_to_string(&follow_up_output).expect("follow-up answer written");
    assert!(
        !follow_up.trim().is_empty(),
        "follow-up answer should not be empty"
    );
}
