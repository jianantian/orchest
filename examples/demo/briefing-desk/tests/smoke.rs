//! Black-box smoke test for the `--fake` CLI path: no network credentials,
//! exercises the full materials -> transcribe -> read-image -> write ->
//! synthesize pipeline against the real issue-001 fixture corpus.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_briefing-desk"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/research")
}

#[test]
fn fake_run_writes_brief_and_audio() {
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
        stdout.contains("[transcribe]"),
        "missing transcribe step: {stdout}"
    );
    assert!(stdout.contains("[vision]"), "missing vision step: {stdout}");
    assert!(stdout.contains("[write]"), "missing write step: {stdout}");
    assert!(
        stdout.contains("[synthesize]"),
        "missing synthesize step: {stdout}"
    );

    let brief = std::fs::read_to_string(&output).expect("brief written");
    assert!(!brief.trim().is_empty(), "brief should not be empty");

    let audio = output.with_extension("wav");
    assert!(
        audio.exists(),
        "audio brief should exist by default (no --no-tts)"
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
fn resume_is_a_clear_stub_for_now() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = tmp.path().join("brief.md");

    let result = bin()
        .args([
            "resume",
            "--session",
            "does-not-matter-yet",
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
        "resume is not implemented until issue 004"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("issue 004"),
        "stub error should point to issue 004: {stderr}"
    );
}
