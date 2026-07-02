//! Black-box smoke test for the `--fake` CLI path: no network credentials,
//! exercises the full materials -> search -> read -> write (approval) ->
//! synthesize pipeline against the real issue-001 fixture corpus.

use std::path::PathBuf;
use std::process::Command;

const DENY_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_APPROVAL";
const DENY_TTS_APPROVAL_ENV: &str = "BRIEFING_DESK_FAKE_DENY_TTS_APPROVAL";

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_briefing-desk"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/research")
}

#[test]
fn fake_run_search_reads_and_writes_brief_and_audio() {
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
            "--fake",
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("search_fixtures started"),
        "missing search tool call: {stdout}"
    );
    assert!(
        stdout.contains("read_fixture started"),
        "missing read tool call: {stdout}"
    );
    assert!(
        stdout.contains("transcribe_audio started"),
        "missing ASR tool call: {stdout}"
    );
    assert!(
        stdout.contains("describe_image started"),
        "missing vision-placeholder tool call: {stdout}"
    );
    assert!(
        stdout.contains("review_report started"),
        "missing reviewer tool call: {stdout}"
    );
    assert!(
        stdout.contains("[reviewer]"),
        "reviewer sub-agent output should be visible in the event stream: {stdout}"
    );
    assert!(
        stdout.contains("[approval] requested for write_report"),
        "missing approval request: {stdout}"
    );
    assert!(
        stdout.contains("[approval] granted for write_report"),
        "missing approval grant: {stdout}"
    );
    assert!(
        stdout.contains("write_report completed"),
        "missing write_report completion: {stdout}"
    );
    assert!(
        stdout.contains("synthesize_brief started"),
        "missing TTS tool call: {stdout}"
    );

    let brief = std::fs::read_to_string(&output).expect("brief written");
    assert!(!brief.trim().is_empty(), "brief should not be empty");
    assert!(
        brief.contains("Honestly? My team would go straight back to spreadsheets"),
        "brief should include the ASR-transcribed interview quote: {brief}"
    );
    assert!(
        brief.contains("Q1 38%, Q2 40%, Q3 42%"),
        "brief should include the image-derived chart facts: {brief}"
    );

    let audio = output.with_extension("wav");
    assert!(
        audio.exists(),
        "audio brief should exist by default (no --no-tts)"
    );
}

#[test]
fn fake_run_tts_denied_leaves_report_but_no_audio() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .env(DENY_TTS_APPROVAL_ENV, "1")
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            output.to_str().unwrap(),
            "--fake",
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        result.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("[approval] granted for write_report"),
        "write_report should still be approved independently of TTS: {stdout}"
    );
    assert!(
        stdout.contains("[approval] denied for synthesize_brief"),
        "missing TTS denial: {stdout}"
    );
    assert!(
        !stdout.contains("synthesize_brief completed"),
        "denied synthesize_brief must never execute: {stdout}"
    );

    assert!(output.exists(), "report should still be written");
    assert!(
        !output.with_extension("wav").exists(),
        "denied TTS approval must leave no audio file"
    );
}

#[test]
fn fake_run_approval_denied_leaves_no_report() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .env(DENY_APPROVAL_ENV, "1")
        .args([
            "run",
            "--materials",
            fixtures_dir().to_str().unwrap(),
            "--question",
            "Is Loom worth continued investment in Q4?",
            "--output",
            output.to_str().unwrap(),
            "--fake",
        ])
        .output()
        .expect("run briefing-desk");

    assert!(
        result.status.success(),
        "a denied approval should not itself be treated as a CLI failure: stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        stdout.contains("[approval] denied for write_report"),
        "missing approval denial: {stdout}"
    );
    assert!(
        !stdout.contains("write_report completed"),
        "denied write_report must never execute: {stdout}"
    );

    assert!(
        !output.exists(),
        "denied approval must leave the output path absent"
    );
    assert!(
        !output.with_extension("wav").exists(),
        "denied approval must leave no audio file either"
    );
}

#[test]
fn fake_run_no_tts_skips_audio_file() {
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
            "--fake",
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
fn live_mode_without_fake_fails_clearly() {
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
        !result.status.success(),
        "live mode should not silently succeed yet"
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
            "--fake",
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
            "--fake",
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
    // finds the same session file — this is the real cross-process path, not
    // an in-process handle reuse.
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
            "--fake",
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
        follow_up.contains("Briefing Desk (fake smoke run)"),
        "follow-up answer should reference content from the original brief, got: {follow_up}"
    );
    assert!(
        follow_up.contains("Has anything changed about the retention numbers?"),
        "follow-up answer should reference the new question, got: {follow_up}"
    );
}
