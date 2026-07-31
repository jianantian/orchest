use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use research_pipeline_demo::findings::{
    render_markdown, validate_json, FindingClassification, FindingStatus,
};

fn report_fixture() -> String {
    fs::read_to_string("tests/fixtures/findings/report-valid.json")
        .expect("report fixture should be readable")
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("demo manifest must live below the repository root")
        .to_path_buf()
}

#[test]
fn renders_every_required_report_section() {
    let document = validate_json(&report_fixture()).expect("report fixture should validate");
    let rendered = render_markdown(&document).expect("report should render");

    for required in [
        "# Renderer Contract seam gap analysis",
        "## Executive summary",
        "## Readiness verdict",
        "## Seam API checklist",
        "## Findings summary",
        "### SB-2 — Nested events bypass watchers",
        "**Action owner:** Orchest runtime",
        "**Issue:** #901",
        "## Verification evidence",
        "## Run evidence",
        "`git:self` denotes the commit containing the canonical findings file",
        "### Live-provider boundary",
        "configured-by-env",
        "## Evidence catalogue",
        "## v1.0 and Multivac M2 implications",
        "SB-2",
        "run-live",
    ] {
        assert!(
            rendered.contains(required),
            "rendered report must contain {required:?}\n{rendered}"
        );
    }
    assert!(
        !rendered.ends_with("\n\n"),
        "rendered report must end with exactly one newline"
    );
}

#[test]
fn rendering_is_deterministic_across_collection_order() {
    let document = validate_json(&report_fixture()).expect("report fixture should validate");
    let expected = render_markdown(&document).expect("report should render");
    let mut reordered = document.clone();
    reordered.api_checklist.reverse();
    reordered.runs.reverse();
    reordered.evidence.reverse();
    reordered.findings.reverse();
    reordered.readiness_verdict.refs.reverse();
    for item in &mut reordered.api_checklist {
        item.evidence_refs.reverse();
        item.finding_refs.reverse();
    }
    for finding in &mut reordered.findings {
        finding.evidence_refs.reverse();
        finding.verification.evidence_refs.reverse();
    }

    assert_eq!(
        render_markdown(&reordered).expect("reordered report should render"),
        expected
    );
}

#[test]
fn stale_check_fails_without_rewriting_the_report() {
    let temp = tempfile::tempdir().expect("temporary report directory");
    let findings = temp.path().join("findings.json");
    let report = temp.path().join("report.md");
    fs::write(&findings, report_fixture()).expect("write findings fixture");
    fs::write(&report, "stale report\n").expect("write stale report");

    let output = Command::new(env!("CARGO_BIN_EXE_seam-report"))
        .arg("check")
        .arg("--findings")
        .arg(&findings)
        .arg("--report")
        .arg(&report)
        .output()
        .expect("run seam-report check");

    assert!(!output.status.success(), "stale check must fail");
    assert_eq!(
        fs::read_to_string(report).expect("read report after check"),
        "stale report\n",
        "check must not rewrite stale Markdown"
    );
}

#[test]
fn check_is_byte_for_byte_including_line_endings() {
    let temp = tempfile::tempdir().expect("temporary report directory");
    let findings = temp.path().join("findings.json");
    let report = temp.path().join("report.md");
    fs::write(&findings, report_fixture()).expect("write findings fixture");
    let document = validate_json(&report_fixture()).expect("report fixture should validate");
    let rendered = render_markdown(&document).expect("report should render");
    fs::write(&report, rendered.replace('\n', "\r\n")).expect("write CRLF report");

    let output = Command::new(env!("CARGO_BIN_EXE_seam-report"))
        .arg("check")
        .arg("--findings")
        .arg(&findings)
        .arg("--report")
        .arg(&report)
        .output()
        .expect("run seam-report check");

    assert!(
        !output.status.success(),
        "byte-for-byte check must reject line-ending drift"
    );
}

#[test]
fn check_resolves_the_repository_report_without_writing_it() {
    let temp = tempfile::tempdir().expect("temporary working directory");
    let root = repository_root();
    let canonical_report = root.join("docs/review/v0_11_seam_gap_analysis.md");
    let bytes_before = fs::read(&canonical_report).expect("read canonical report before test");
    let modified_before = fs::metadata(&canonical_report)
        .and_then(|metadata| metadata.modified())
        .expect("read canonical report modification time before test");
    let check = Command::new(env!("CARGO_BIN_EXE_seam-report"))
        .current_dir(temp.path())
        .arg("check")
        .arg("--findings")
        .arg(root.join("examples/demo/research-pipeline/findings.json"))
        .arg("--report")
        .arg("docs/review/v0_11_seam_gap_analysis.md")
        .output()
        .expect("run seam-report check from a different working directory");

    assert!(
        check.status.success(),
        "repository-bound check failed: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(
        !temp.path().join("docs").exists(),
        "check must not create a report relative to the process working directory"
    );
    assert_eq!(
        fs::read(&canonical_report).expect("read canonical report after test"),
        bytes_before,
        "ordinary tests must not change canonical report bytes"
    );
    assert_eq!(
        fs::metadata(&canonical_report)
            .and_then(|metadata| metadata.modified())
            .expect("read canonical report modification time after test"),
        modified_before,
        "ordinary tests must not write the tracked canonical report"
    );
}

#[test]
fn release_implications_render_implemented_blockers_as_unresolved_not_open() {
    let mut document = validate_json(&report_fixture()).expect("report fixture should validate");
    let finding = document
        .findings
        .iter_mut()
        .find(|finding| finding.id == "SB-2")
        .expect("fixture must contain SB-2");
    finding.classification = FindingClassification::ReleaseBlocker;
    finding.status = FindingStatus::Implemented;

    let rendered = render_markdown(&document).expect("implemented report should render");

    assert!(rendered.contains("Unresolved release blockers:"));
    assert!(rendered.contains("- SB-2 — Nested events bypass watchers (`implemented`, #901)"));
    assert!(!rendered.contains("Open release blockers:"));
}

#[test]
fn seam_implications_render_implemented_blockers_with_actual_status() {
    let mut document = validate_json(&report_fixture()).expect("report fixture should validate");
    let finding = document
        .findings
        .iter_mut()
        .find(|finding| finding.id == "SB-2")
        .expect("fixture must contain SB-2");
    finding.status = FindingStatus::Implemented;

    let rendered = render_markdown(&document).expect("implemented report should render");

    assert!(rendered.contains("Unresolved supervised-delegation seam blockers:"));
    assert!(rendered.contains("- SB-2 — Nested events bypass watchers (`implemented`, #901)"));
}
