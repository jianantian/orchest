//! Offline CLI integration tests for eval run / compare (scripted model).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_briefing-desk"))
}

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Clean git worktree root for preflight: use a temp git repo that only has
/// optional harness dirty, by pointing --repo-root at a fresh repo.
fn clean_repo_root() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp repo");
    let status = Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .status()
        .expect("git init");
    assert!(status.success());
    for (key, value) in [
        ("user.name", "Briefing Desk Eval Test"),
        ("user.email", "eval-test@example.invalid"),
    ] {
        let status = Command::new("git")
            .args(["config", "--local", key, value])
            .current_dir(dir.path())
            .status()
            .expect("git config");
        assert!(status.success(), "failed to configure {key}");
    }
    // Empty commit so rev-parse works.
    let status = Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(dir.path())
        .status()
        .expect("git commit");
    assert!(status.success());
    dir
}

fn eval_run(args: &[&str]) -> std::process::Output {
    let mut cmd = bin();
    cmd.arg("eval").arg("run");
    for a in args {
        cmd.arg(a);
    }
    cmd.output().expect("spawn eval run")
}

#[test]
fn preflight_requires_record_sensitive() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let out = eval_run(&[
        "--label",
        "no-sensitive",
        "--split",
        "optimization",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--scripted",
    ]);
    assert!(
        !out.status.success(),
        "must fail without --record-sensitive"
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("record-sensitive") || err.contains("record_sensitive"),
        "stderr={err}"
    );
    // No half-finished run dir.
    assert!(!runs.path().join("no-sensitive").exists());
}

#[test]
fn preflight_scorecard_requires_confirm_sealed() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let out = eval_run(&[
        "--label",
        "score-no-seal",
        "--split",
        "scorecard",
        "--record-sensitive",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--scripted",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("confirm-sealed"), "stderr={err}");
    assert!(!runs.path().join("score-no-seal").exists());
}

#[test]
fn preflight_unknown_split() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let out = eval_run(&[
        "--label",
        "bad-split",
        "--split",
        "train",
        "--record-sensitive",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--scripted",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unknown split") || err.contains("train"),
        "stderr={err}"
    );
}

#[test]
fn preflight_label_conflict() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(runs.path().join("taken")).unwrap();
    let out = eval_run(&[
        "--label",
        "taken",
        "--split",
        "optimization",
        "--record-sensitive",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--scripted",
    ]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("already exists") || err.contains("overwrite"),
        "stderr={err}"
    );
}

#[test]
fn scripted_optimization_run_writes_artifacts() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let materials = package_root().join("fixtures/research");
    let out = eval_run(&[
        "--label",
        "scripted-opt",
        "--split",
        "optimization",
        "--record-sensitive",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--materials",
        materials.to_str().unwrap(),
        "--scripted",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "scripted run should succeed\nstdout={stdout}\nstderr={stderr}"
    );

    let run_dir = runs.path().join("scripted-opt");
    assert!(run_dir.join("manifest.json").is_file());
    assert!(run_dir.join("harness/snapshot.json").is_file());
    assert!(run_dir.join("harness/snapshot.sha256").is_file());
    assert!(run_dir.join("effective-config/snapshot.json").is_file());
    assert!(run_dir.join("effective-config/snapshot.sha256").is_file());
    assert!(run_dir.join("results.json").is_file());
    assert!(run_dir.join("summary.md").is_file());

    let effective: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(run_dir.join("effective-config/snapshot.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(effective["schema_version"], "3");
    assert_eq!(effective["main"]["request_options"]["max_tokens"], 4096);
    let profiles = effective["case_profiles"].as_array().unwrap();
    assert!(profiles
        .iter()
        .any(|profile| profile["session_mode"] == "none"));
    assert!(profiles
        .iter()
        .any(|profile| profile["session_mode"] == "follow_up_from_seed"));
    let all_tools: Vec<_> = profiles
        .iter()
        .flat_map(|profile| profile["tools"].as_array().into_iter().flatten())
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for name in [
        "review_report",
        "transcribe_audio",
        "describe_image",
        "write_report",
    ] {
        assert!(all_tools.contains(&name), "missing tool {name}");
    }

    // At least one case attempt four-file set.
    let cases_dir = run_dir.join("cases");
    assert!(cases_dir.is_dir());
    let mut found_attempt = false;
    let mut found_completed_terminal = false;
    for case in std::fs::read_dir(&cases_dir).unwrap() {
        let case = case.unwrap().path();
        for att in std::fs::read_dir(&case).unwrap() {
            let att = att.unwrap().path();
            for name in [
                "trajectory.jsonl",
                "output.md",
                "attempt.json",
                "scores.json",
            ] {
                assert!(
                    att.join(name).is_file(),
                    "missing {} in {}",
                    name,
                    att.display()
                );
            }
            let attempt: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(att.join("attempt.json")).unwrap())
                    .unwrap();
            if attempt["status"] == "completed" {
                assert_eq!(attempt["terminal_kind"], "run_completed");
                assert_eq!(attempt["stop_reason"], "end_turn");
                found_completed_terminal = true;
            }
            found_attempt = true;
        }
    }
    assert!(found_attempt, "expected at least one attempt directory");
    assert!(
        found_completed_terminal,
        "expected a completed terminal record"
    );

    let retained_followup = walk_files(&cases_dir).into_iter().any(|path| {
        path.file_name().and_then(|name| name.to_str()) == Some("trajectory.jsonl")
            && std::fs::read_to_string(path)
                .map(|text| text.contains("followup_session_resumed"))
                .unwrap_or(false)
    });
    assert!(retained_followup, "expected retained follow-up seed event");

    // optimization = 1 rep per case; 10 cases.
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(run_dir.join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema_version"], "3");
    assert_eq!(manifest["label"], "scripted-opt");
    assert_eq!(manifest["repetition"], 1);
    assert_eq!(manifest["case_ids"].as_array().unwrap().len(), 10);
    assert_eq!(manifest["case_policies"].as_object().unwrap().len(), 10);
    for policy in manifest["case_policies"].as_object().unwrap().values() {
        assert!(policy["split"].is_string());
        assert!(policy["must_pass"].is_boolean());
        assert!(policy["weight"].is_number());
        assert!(policy["tags"].is_array());
    }
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn scripted_scorecard_is_rejected_even_with_confirm_sealed() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let materials = package_root().join("fixtures/research");

    let out = eval_run(&[
        "--label",
        "sealed-scripted",
        "--split",
        "scorecard",
        "--record-sensitive",
        "--confirm-sealed",
        "--runs-dir",
        runs.path().to_str().unwrap(),
        "--repo-root",
        repo.path().to_str().unwrap(),
        "--materials",
        materials.to_str().unwrap(),
        "--scripted",
    ]);
    assert!(!out.status.success(), "scripted scorecard must fail");
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        err.contains("scripted") || err.contains("injected") || err.contains("scorecard"),
        "stderr/stdout={err}"
    );
    assert!(
        !runs.path().join("sealed-scripted").exists(),
        "must not create sealed scripted run dir"
    );
}

#[test]
fn compare_incomparable_when_labels_missing() {
    let runs = tempfile::tempdir().unwrap();
    let out = bin()
        .args([
            "eval",
            "compare",
            "nope-a",
            "nope-b",
            "--runs-dir",
            runs.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

/// Helper: write two synthetic results via scripted optimization runs and compare.
#[test]
fn compare_eligible_path_with_identical_scripted_runs() {
    let repo = clean_repo_root();
    let runs = tempfile::tempdir().unwrap();
    let materials = package_root().join("fixtures/research");

    for label in ["cmp-base", "cmp-cand"] {
        let out = eval_run(&[
            "--label",
            label,
            "--split",
            "validation",
            "--record-sensitive",
            "--runs-dir",
            runs.path().to_str().unwrap(),
            "--repo-root",
            repo.path().to_str().unwrap(),
            "--materials",
            materials.to_str().unwrap(),
            "--scripted",
        ]);
        assert!(
            out.status.success(),
            "label={label} stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Patch scores for a deterministic +10 delta and pin tokens/latency so the
    // compare resource gates cannot flake on wall-clock jitter from scripted runs.
    patch_results_for_eligibility(runs.path(), "cmp-base", "cmp-cand");
    let before_baseline = snapshot_tree(&runs.path().join("cmp-base"));
    let before_candidate = snapshot_tree(&runs.path().join("cmp-cand"));

    let out = bin()
        .args([
            "eval",
            "compare",
            "cmp-base",
            "cmp-cand",
            "--runs-dir",
            runs.path().to_str().unwrap(),
            "--out-dir",
            runs.path().join("_compare").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "compare CLI should succeed for patched identical scripted runs\nstdout={stdout}\nstderr={stderr}"
    );
    let compare_files: Vec<_> = std::fs::read_dir(runs.path().join("_compare"))
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    let json = compare_files
        .iter()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .expect("json report");
    let markdown = compare_files
        .iter()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .expect("markdown report");
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(json).unwrap()).unwrap();
    assert_eq!(report["status"], "eligible_for_review");
    assert!(std::fs::read_to_string(markdown)
        .unwrap()
        .contains("**status**: `eligible_for_review`"));
    assert_eq!(
        snapshot_tree(&runs.path().join("cmp-base")),
        before_baseline
    );
    assert_eq!(
        snapshot_tree(&runs.path().join("cmp-cand")),
        before_candidate
    );
}

fn patch_results_for_eligibility(runs_root: &Path, base: &str, cand: &str) {
    // Stable resource numbers so compare's tokens/latency gates are not exposed to
    // wall-clock jitter from the preceding scripted runs (the flake that showed up
    // as empty stderr + non-zero exit when status was not_eligible).
    const STABLE_GATE_TOKENS: u64 = 100;
    const STABLE_LATENCY_MS: u64 = 100;

    for label in [base, cand] {
        let path = runs_root.join(label).join("results.json");
        if !path.is_file() {
            continue;
        }
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let score = if label == cand { 80.0 } else { 70.0 };
        v["all_completed"] = serde_json::json!(true);
        v["any_inconclusive"] = serde_json::json!(false);
        v["any_resource_incomplete"] = serde_json::json!(false);
        for tag in [
            "tool_selection",
            "tool_chaining",
            "modality_coverage",
            "conflict_reconciliation",
            "report_structure",
            "citation_quality",
            "followup_grounding",
        ] {
            v["per_tag"][tag] = serde_json::json!(score);
        }
        let mut validation_tokens: Vec<u64> = Vec::new();
        let mut validation_latencies: Vec<u64> = Vec::new();
        if let Some(cases) = v.get_mut("cases").and_then(serde_json::Value::as_array_mut) {
            for case in cases {
                case["passed"] = serde_json::json!(true);
                case["score"] = serde_json::json!(score);
                let is_validation = case
                    .get("split")
                    .and_then(|s| s.as_str())
                    .is_some_and(|s| s == "validation");
                if let Some(attempts) = case
                    .get_mut("attempts")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for attempt in attempts {
                        attempt["status"] = serde_json::json!("completed");
                        attempt["grader_status"] = serde_json::json!("completed");
                        attempt["passed"] = serde_json::json!(true);
                        attempt["score"] = serde_json::json!(score);
                        attempt["resource_coverage"] = serde_json::json!("complete");
                        attempt["gate_total_tokens"] = serde_json::json!(STABLE_GATE_TOKENS);
                        attempt["wall_latency_ms"] = serde_json::json!(STABLE_LATENCY_MS);
                        if is_validation {
                            validation_tokens.push(STABLE_GATE_TOKENS);
                            validation_latencies.push(STABLE_LATENCY_MS);
                        }
                    }
                }
            }
        }
        v["overall"] = serde_json::json!(score);
        // Keep aggregates consistent with attempt rows so load_run's results
        // contract validation succeeds.
        let has_validation = !validation_tokens.is_empty();
        v["validation_attempt_gate_tokens"] = serde_json::json!(validation_tokens);
        v["validation_completed_latencies_ms"] = serde_json::json!(validation_latencies);
        if has_validation {
            v["validation_mean_gate_tokens"] = serde_json::json!(STABLE_GATE_TOKENS as f64);
            v["validation_median_latency_ms"] = serde_json::json!(STABLE_LATENCY_MS as f64);
        } else {
            v["validation_mean_gate_tokens"] = serde_json::Value::Null;
            v["validation_median_latency_ms"] = serde_json::Value::Null;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    }
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    snapshot_tree_into(root, root, &mut files);
    files
}

fn snapshot_tree_into(root: &Path, directory: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if entry.file_type().unwrap().is_dir() {
            snapshot_tree_into(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                std::fs::read(path).unwrap(),
            );
        }
    }
}
