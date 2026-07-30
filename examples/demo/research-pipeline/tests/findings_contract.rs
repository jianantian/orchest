use research_pipeline_demo::findings::validate_json;

fn fixture(name: &str) -> String {
    let path = format!("tests/fixtures/findings/{name}.json");
    std::fs::read_to_string(path).expect("contract fixture should be readable")
}

fn assert_invalid(name: &str, fragment: &str) {
    let error = validate_json(&fixture(name)).expect_err("fixture must be invalid");
    assert!(
        error.to_string().contains(fragment),
        "{name} should mention {fragment}, got: {error}"
    );
}

#[test]
fn accepts_the_closed_v1_contract() {
    validate_json(&fixture("valid")).expect("valid v1 fixture should validate");
}

#[test]
fn rejects_an_unknown_schema_version() {
    let error = validate_json(&fixture("unknown-schema"))
        .expect_err("unknown schema versions must fail closed");

    assert!(error.to_string().contains("schemaVersion"));
}

#[test]
fn rejects_duplicate_ids() {
    assert_invalid("duplicate-finding-id", "duplicate finding id");
}

#[test]
fn rejects_broken_references() {
    assert_invalid("broken-reference", "unknown evidence ref");
}

#[test]
fn rejects_unsafe_repository_paths() {
    assert_invalid("unsafe-path", "unsafe repository path");
}

#[test]
fn rejects_verified_findings_without_a_passed_verifier() {
    assert_invalid("verified-without-verification", "verified finding");
}

#[test]
fn rejects_ready_with_an_open_blocker() {
    assert_invalid("ready-with-open-blocker", "open blocker");
}

#[test]
fn rejects_ready_when_a_required_live_run_was_not_run() {
    assert_invalid("ready-with-live-not-run", "required live-provider run");
}

#[test]
fn rejects_a_command_claim_without_an_executed_run() {
    assert_invalid("command-without-executed-run", "executed run");
}

#[test]
fn rejects_exercised_checklists_without_an_executed_run() {
    assert_invalid(
        "exercised-with-not-run-evidence",
        "executed test, smoke, or live run",
    );
}

#[test]
fn rejects_not_run_records_that_claim_an_executed_command() {
    assert_invalid("not-run-with-command", "not-run run");
}

#[test]
fn rejects_source_symbols_that_are_line_locators() {
    assert_invalid("source-symbol-line-locator", "stable symbol");
}

#[test]
fn rejects_live_provider_rows_with_a_blank_provider() {
    assert_invalid("live-provider-blank-provider", "provider");
}

#[test]
fn rejects_live_provider_rows_with_a_blank_model() {
    assert_invalid("live-provider-blank-model", "model");
}

#[test]
fn rejects_executed_runs_with_a_blank_command() {
    assert_invalid("executed-run-blank-command", "command");
}

#[test]
fn rejects_verified_findings_backed_only_by_documentation() {
    assert_invalid(
        "verified-with-source-only-evidence",
        "passed verification evidence",
    );
}

#[test]
fn rejects_non_ready_verdicts_with_a_blank_reason() {
    assert_invalid("non-ready-blank-reason", "readiness reason");
}

#[test]
fn rejects_a_verified_finding_backed_by_a_failed_verifier_run() {
    assert_invalid("verified-with-failed-run", "passed verification evidence");
}

#[test]
fn rejects_a_verified_finding_backed_by_a_partial_verifier_run() {
    assert_invalid("verified-with-partial-run", "passed verification evidence");
}
