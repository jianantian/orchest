use research_pipeline_demo::findings::{validate_json, FindingsDocument};
use serde_json::{json, Value};

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

fn valid_value() -> Value {
    serde_json::from_str(&fixture("valid")).expect("valid fixture should be JSON")
}

fn assert_value_invalid(value: &Value, fragment: &str) {
    let error = validate_json(&serde_json::to_string(value).expect("serialize mutated fixture"))
        .expect_err("mutated fixture must be invalid");
    assert!(
        error.to_string().contains(fragment),
        "fixture should mention {fragment}, got: {error}"
    );
}

#[test]
fn accepts_the_closed_v1_contract() {
    validate_json(&fixture("valid")).expect("valid v1 fixture should validate");
}

#[test]
fn canonical_final_executed_rows_use_the_containing_commit_revision() {
    let canonical = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("findings.json"),
    )
    .expect("canonical findings should be readable");
    let document = validate_json(&canonical).expect("canonical findings should validate");

    for run_id in [
        "run-worker-deterministic",
        "run-supervisor-watcher-deterministic",
        "run-failure-escalation-deterministic",
        "run-watcher-order-deterministic",
    ] {
        let run = document
            .runs
            .iter()
            .find(|run| run.id == run_id)
            .unwrap_or_else(|| panic!("canonical findings must contain {run_id}"));
        assert_eq!(
            run.revision.as_deref(),
            Some("git:self"),
            "{run_id} must cite the containing commit that has its claimed test target"
        );
    }
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

#[test]
fn rejects_unknown_values_for_every_closed_enum() {
    let canonical: Value =
        serde_json::from_str(include_str!("../findings.json")).expect("canonical JSON");
    let paths = [
        vec!["readinessVerdict", "status"],
        vec!["apiChecklist", "0", "status"],
        vec!["runs", "0", "kind"],
        vec!["runs", "0", "status"],
        vec!["evidence", "0", "kind"],
        vec!["findings", "0", "classification"],
        vec!["findings", "0", "status"],
        vec!["findings", "0", "verification", "status"],
    ];

    for path in paths {
        let mut mutated = canonical.clone();
        let mut target = &mut mutated;
        for segment in &path[..path.len() - 1] {
            target = if let Ok(index) = segment.parse::<usize>() {
                &mut target[index]
            } else {
                &mut target[*segment]
            };
        }
        target[path[path.len() - 1]] = json!("future-value");
        let error = serde_json::from_value::<FindingsDocument>(mutated)
            .expect_err("unknown enum value must fail closed");
        assert!(
            error.to_string().contains("unknown variant"),
            "path {path:?} should reject its enum, got {error}"
        );
    }
}

#[test]
fn rejects_duplicate_ids_for_every_graph_collection() {
    for collection in ["apiChecklist", "runs", "evidence"] {
        let mut value = valid_value();
        let duplicate = value[collection][0].clone();
        value[collection]
            .as_array_mut()
            .expect("collection array")
            .push(duplicate);
        assert_value_invalid(&value, "duplicate");
    }
}

#[test]
fn rejects_non_canonical_repository_path_shapes() {
    for path in [
        "/tmp/evidence.md",
        "../evidence.md",
        "docs/../evidence.md",
        "C:\\work\\evidence.md",
        "C:evidence.md",
        "docs//evidence.md",
        "./docs/evidence.md",
    ] {
        let mut value = valid_value();
        value["evidence"][0]["path"] = json!(path);
        let expected = if path == "/tmp/evidence.md" || path == "C:\\work\\evidence.md" {
            "machine-local path"
        } else {
            "unsafe repository path"
        };
        assert_value_invalid(&value, expected);
    }
}

#[test]
fn rejects_executed_runs_without_date_and_revision() {
    for field in ["date", "revision"] {
        let mut value = valid_value();
        value["runs"][0] = json!({
            "id": "run-test",
            "kind": "test",
            "status": "passed",
            "required": true,
            "command": "cargo test -p research-pipeline-demo",
            "date": "2026-07-31",
            "revision": "abcdef0",
            "summary": "The deterministic suite passed.",
            "evidenceRefs": []
        });
        value["readinessVerdict"]["refs"] = json!(["run-test"]);
        value["runs"][0]
            .as_object_mut()
            .expect("run object")
            .remove(field);
        assert_value_invalid(&value, field);
    }
}

#[test]
fn rejects_invalid_execution_dates() {
    let mut value = valid_value();
    value["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "passed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "July 31",
        "revision": "abcdef0",
        "summary": "The deterministic suite passed.",
        "evidenceRefs": []
    });
    value["readinessVerdict"]["refs"] = json!(["run-test"]);
    assert_value_invalid(&value, "YYYY-MM-DD");
}

#[test]
fn rejects_non_reciprocal_run_and_evidence_references() {
    let mut evidence_missing_reverse = valid_value();
    evidence_missing_reverse["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "passed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "2026-07-31",
        "revision": "abcdef0",
        "summary": "The deterministic suite passed.",
        "evidenceRefs": []
    });
    evidence_missing_reverse["evidence"][0] = json!({
        "id": "ev-contract",
        "kind": "test",
        "summary": "The suite passed.",
        "path": "examples/demo/research-pipeline/tests/findings_contract.rs",
        "symbol": "accepts_the_closed_v1_contract",
        "command": "cargo test -p research-pipeline-demo",
        "runRef": "run-test",
        "result": "passed"
    });
    evidence_missing_reverse["readinessVerdict"]["refs"] = json!(["run-test"]);
    assert_value_invalid(&evidence_missing_reverse, "must list evidence");

    let mut run_missing_reverse = valid_value();
    run_missing_reverse["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "passed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "2026-07-31",
        "revision": "abcdef0",
        "summary": "The deterministic suite passed.",
        "evidenceRefs": ["ev-contract"]
    });
    run_missing_reverse["readinessVerdict"]["refs"] = json!(["run-test"]);
    run_missing_reverse["runs"][0]["evidenceRefs"] = json!(["ev-contract"]);
    assert_value_invalid(&run_missing_reverse, "must point back");
}

#[test]
fn rejects_evidence_commands_that_differ_from_the_run_command() {
    let mut value = valid_value();
    value["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "passed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "2026-07-31",
        "revision": "abcdef0",
        "summary": "The deterministic suite passed.",
        "evidenceRefs": ["ev-contract"]
    });
    value["evidence"][0]["kind"] = json!("test");
    value["evidence"][0]["runRef"] = json!("run-test");
    value["evidence"][0]["command"] = json!("cargo test --workspace");
    value["evidence"][0]["result"] = json!("passed");
    value["readinessVerdict"]["refs"] = json!(["run-test"]);
    assert_value_invalid(&value, "must match run");
}

#[test]
fn rejects_final_untriaged_findings() {
    let mut value = valid_value();
    value["findings"] = json!([{
        "id": "SB-1",
        "title": "Gap",
        "apiSurface": "surface",
        "description": "gap",
        "observedConsequence": "consequence",
        "workaround": "none",
        "classification": "untriaged",
        "status": "open",
        "evidenceRefs": ["ev-contract"],
        "verification": {
            "status": "not-run",
            "commands": [],
            "evidenceRefs": [],
            "summary": "Not run."
        }
    }]);
    assert_value_invalid(&value, "final classification");
}

#[test]
fn rejects_open_blockers_without_owned_issue_references() {
    let mut value = valid_value();
    value["findings"] = json!([{
        "id": "SB-1",
        "title": "Gap",
        "apiSurface": "surface",
        "description": "gap",
        "observedConsequence": "consequence",
        "workaround": "none",
        "classification": "seam-blocker",
        "status": "open",
        "evidenceRefs": ["ev-contract"],
        "action": {
            "owner": "Orchest runtime",
            "summary": "Add the missing public seam."
        },
        "verification": {
            "status": "not-run",
            "commands": [],
            "evidenceRefs": [],
            "summary": "Not run."
        }
    }]);
    assert_value_invalid(&value, "issueRef");
}

#[test]
fn rejects_deferred_findings_without_an_owned_decision() {
    let mut value = valid_value();
    value["findings"] = json!([{
        "id": "P1-1",
        "title": "Backlog",
        "apiSurface": "surface",
        "description": "gap",
        "observedConsequence": "consequence",
        "workaround": "none",
        "classification": "post-1.0-backlog",
        "status": "deferred",
        "evidenceRefs": ["ev-contract"],
        "verification": {
            "status": "not-applicable",
            "commands": [],
            "evidenceRefs": ["ev-contract"],
            "summary": "Deferred by the release decision."
        }
    }]);
    assert_value_invalid(&value, "deferred finding");
}

#[test]
fn rejects_implemented_findings_claiming_passed_verification() {
    let mut value: Value = serde_json::from_str(&fixture("report-valid")).expect("fixture JSON");
    value["findings"][1]["status"] = json!("implemented");
    value["findings"][1]["evidenceRefs"] = json!(["ev-contract", "ev-test"]);
    value["findings"][1]["verification"] = json!({
        "status": "passed",
        "commands": ["cargo test -p research-pipeline-demo"],
        "evidenceRefs": ["ev-test"],
        "summary": "The verifier passed."
    });
    assert_value_invalid(&value, "implemented finding");
}

#[test]
fn rejects_verification_commands_without_matching_executed_evidence() {
    let mut value = valid_value();
    value["findings"] = json!([{
        "id": "P1-1",
        "title": "Backlog",
        "apiSurface": "surface",
        "description": "gap",
        "observedConsequence": "consequence",
        "workaround": "none",
        "classification": "post-1.0-backlog",
        "status": "open",
        "evidenceRefs": ["ev-contract"],
        "verification": {
            "status": "not-run",
            "commands": ["cargo test -p research-pipeline-demo"],
            "evidenceRefs": ["ev-contract"],
            "summary": "Not run."
        }
    }]);
    assert_value_invalid(&value, "not-run verification");
}

#[test]
fn rejects_failed_or_blocked_checklist_rows_without_findings() {
    for status in ["failed", "blocked"] {
        let mut value = valid_value();
        value["apiChecklist"][0]["status"] = json!(status);
        value["apiChecklist"][0]["findingRefs"] = json!([]);
        assert_value_invalid(&value, "finding reference");
    }
}

#[test]
fn rejects_oversized_or_secret_shaped_diagnostics() {
    let mut oversized = valid_value();
    oversized["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "failed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "2026-07-31",
        "revision": "abcdef0",
        "summary": "The deterministic suite failed.",
        "diagnosticExcerpt": "x".repeat(1_001),
        "evidenceRefs": []
    });
    oversized["readinessVerdict"]["refs"] = json!(["run-test"]);
    assert_value_invalid(&oversized, "diagnosticExcerpt");

    let mut secret = oversized;
    secret["runs"][0]["diagnosticExcerpt"] =
        json!("authorization: Bearer sk-example-secret-token-123456789");
    assert_value_invalid(&secret, "secret-shaped");
}

#[test]
fn rejects_machine_local_paths_in_runtime_results() {
    let mut value = valid_value();
    value["runs"][0] = json!({
        "id": "run-test",
        "kind": "test",
        "status": "failed",
        "required": true,
        "command": "cargo test -p research-pipeline-demo",
        "date": "2026-07-31",
        "revision": "abcdef0",
        "summary": "The deterministic suite failed.",
        "diagnosticExcerpt": "failed at /Users/example/private/worktree/source.rs",
        "evidenceRefs": []
    });
    value["readinessVerdict"]["refs"] = json!(["run-test"]);
    assert_value_invalid(&value, "machine-local path");
}

#[test]
fn rejects_secret_shaped_execution_commands() {
    let mut value: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    let command = "RESEARCH_PIPELINE_API_KEY=sk-example-secret-token-123456789 cargo test";
    value["runs"][1]["command"] = json!(command);
    value["evidence"][0]["command"] = json!(command);

    assert_value_invalid(&value, "secret-shaped");
}

#[test]
fn rejects_oversized_repository_paths() {
    let mut value = valid_value();
    value["evidence"][0]["path"] = json!(format!("docs/{}.md", "x".repeat(4_000)));

    assert_value_invalid(&value, "evidence path");
}

#[test]
fn rejects_private_values_across_rendered_field_categories() {
    let private_value = "sk-example-secret-token-123456789";
    for pointer in [
        "/subject",
        "/readinessVerdict/reason",
        "/apiChecklist/0/apiSurface",
        "/runs/0/provider",
        "/runs/0/model",
        "/runs/0/summary",
        "/evidence/0/symbol",
        "/evidence/0/summary",
        "/findings/0/title",
        "/findings/0/apiSurface",
        "/findings/0/description",
        "/findings/0/observedConsequence",
        "/findings/0/workaround",
        "/findings/0/action/owner",
        "/findings/0/action/summary",
        "/findings/0/verification/summary",
    ] {
        let mut value: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        *value
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("fixture must contain {pointer}")) = json!(private_value);
        assert_value_invalid(&value, "secret-shaped");
    }
}

#[test]
fn permits_environment_variable_names_without_secret_values() {
    let mut value: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    value["runs"][0]["provider"] = json!("RESEARCH_PIPELINE_API_KEY");
    value["runs"][0]["model"] = json!("RESEARCH_PIPELINE_CHAT_MODEL");
    value["readinessVerdict"]["reason"] =
        json!("RESEARCH_PIPELINE_API_KEY and RESEARCH_PIPELINE_CHAT_MODEL are unavailable.");

    validate_json(&serde_json::to_string(&value).expect("serialize mutated fixture"))
        .expect("environment variable names are not secret values");
}

#[test]
fn rejects_non_calendar_execution_dates_and_accepts_real_leap_days() {
    for invalid_date in ["2026-02-31", "2025-02-29", "2026-04-31"] {
        let mut value: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        value["runs"][1]["date"] = json!(invalid_date);
        assert_value_invalid(&value, "calendar date");
    }

    let mut leap_day: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    leap_day["runs"][1]["date"] = json!("2024-02-29");
    validate_json(&serde_json::to_string(&leap_day).expect("serialize leap-day fixture"))
        .expect("a real leap day must validate");
}

#[test]
fn enforces_git_revision_grammar_and_git_self_lifecycle() {
    for invalid_revision in [
        "working-tree@abc123",
        "ABCDEF1",
        "abc123",
        "abcdef0123456789abcdef0123456789abcdef012",
        "git:head",
    ] {
        let mut value: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        value["runs"][1]["revision"] = json!(invalid_revision);
        assert_value_invalid(&value, "revision");
    }

    let mut failed_self: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    failed_self["runs"][1]["status"] = json!("failed");
    failed_self["runs"][1]["revision"] = json!("git:self");
    assert_value_invalid(&failed_self, "git:self");

    let mut source_only_self: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    source_only_self["runs"][1]["revision"] = json!("git:self");
    source_only_self["evidence"][0]["kind"] = json!("runtime-output");
    source_only_self["evidence"][0]
        .as_object_mut()
        .expect("evidence object")
        .remove("command");
    source_only_self["evidence"][0]
        .as_object_mut()
        .expect("evidence object")
        .remove("result");
    source_only_self["apiChecklist"][1]["status"] = json!("planned");
    assert_value_invalid(&source_only_self, "post-commit verification evidence");

    let mut action_self: Value =
        serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
    action_self["findings"][0]["action"]["revision"] = json!("git:self");
    assert_value_invalid(&action_self, "action revision");

    for valid_revision in [
        "8bb9a9b",
        "b21d00743bc29a6cc874c8680dea6c2daa485030",
        "git:self",
    ] {
        let mut value: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        value["runs"][1]["revision"] = json!(valid_revision);
        validate_json(&serde_json::to_string(&value).expect("serialize revision fixture"))
            .unwrap_or_else(|error| panic!("{valid_revision} must validate: {error}"));
    }
}

#[test]
fn rejects_source_line_and_range_locators_while_allowing_rust_paths() {
    for locator in [
        "42",
        "L42",
        "line 42",
        "lines 42-45",
        "source.rs:lines 42-45",
        "source.rs:42-45",
        "source.rs#L42-L45",
        "source.rs:42..45",
        "source.rs:42–45",
        "source.rs:42—45",
        "source.rs:42−45",
        "source.rs:42―45",
        "source.rs:42﹣45",
        "source.rs:42－45",
        "source.rs:LINES 42 TO 45",
    ] {
        let mut value: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        value["evidence"][1]["kind"] = json!("source");
        value["evidence"][1]["symbol"] = json!(locator);
        assert_value_invalid(&value, "line locator");
    }

    for symbol in [
        "orchest::run::RunHandle::attach_watcher",
        "orchest::run::v2::RunHandle2::attach_watcher",
        "crate2::module42::Type7",
    ] {
        let mut rust_path: Value =
            serde_json::from_str(&fixture("report-valid")).expect("report fixture JSON");
        rust_path["evidence"][1]["kind"] = json!("source");
        rust_path["evidence"][1]["symbol"] = json!(symbol);
        validate_json(&serde_json::to_string(&rust_path).expect("serialize Rust path fixture"))
            .unwrap_or_else(|error| {
                panic!("{symbol} must remain a stable Rust item path: {error}")
            });
    }
}
