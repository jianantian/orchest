use super::*;

const REPORT_PATH: &str = "docs/iteration/v0_11/seam-gap-analysis.md";

pub fn render_markdown(document: &FindingsDocument) -> Result<String, FindingsError> {
    validate_document(document)?;
    let mut checklist = document.api_checklist.iter().collect::<Vec<_>>();
    checklist.sort_by_key(|item| &item.id);
    let mut findings = document.findings.iter().collect::<Vec<_>>();
    findings.sort_by_key(|finding| &finding.id);
    let mut runs = document.runs.iter().collect::<Vec<_>>();
    runs.sort_by_key(|run| &run.id);
    let mut evidence = document.evidence.iter().collect::<Vec<_>>();
    evidence.sort_by_key(|item| &item.id);

    let mut output = format!(
        "# {} seam gap analysis\n\n\
         ## Executive summary\n\n{}\n\n\
         ## Readiness verdict\n\n\
         **Status:** `{}`\n\n",
        markdown_text(&document.subject),
        markdown_text(&document.executive_summary),
        readiness_name(document.readiness_verdict.status),
    );
    if let Some(reason) = &document.readiness_verdict.reason {
        output.push_str(&format!("**Reason:** {}\n\n", markdown_text(reason)));
    }
    output.push_str(&format!(
        "**References:** {}\n\n\
         ## Seam API checklist\n\n\
         | ID | API surface | Public path | Requirement | Status | Evidence | Findings |\n\
         | --- | --- | --- | --- | --- | --- | --- |\n",
        render_refs(&document.readiness_verdict.refs),
    ));
    for item in checklist {
        output.push_str(&format!(
            "| {} | {} | `{}` | {} | `{}` | {} | {} |\n",
            table_text(&item.id),
            table_text(&item.api_surface),
            code_text(&item.public_path),
            table_text(&item.requirement),
            checklist_status_name(item.status),
            render_refs(&item.evidence_refs),
            render_refs(&item.finding_refs),
        ));
    }
    output.push_str(
        "\n## Findings summary\n\n\
         | ID | Title | Classification | Status | Issue |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    for finding in &findings {
        output.push_str(&format!(
            "| {} | {} | `{}` | `{}` | {} |\n",
            table_text(&finding.id),
            table_text(&finding.title),
            classification_name(finding.classification),
            finding_status_name(finding.status),
            finding
                .action
                .as_ref()
                .and_then(|action| action.issue_ref.as_deref())
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
        ));
    }
    for finding in findings {
        render_finding(finding, &mut output);
    }
    render_verification(document, &mut output);
    render_runs(&runs, &mut output);
    render_live_boundary(&runs, &mut output);
    render_evidence(&mut evidence, &mut output);
    render_implications(document, &mut output);
    Ok(output)
}

pub fn is_report_path(path: &str) -> bool {
    path == REPORT_PATH
}

fn render_finding(finding: &Finding, output: &mut String) {
    output.push_str(&format!(
        "\n### {} — {}\n\n\
         **API surface:** `{}`\n\n\
         **Classification:** `{}`\n\n\
         **Status:** `{}`\n\n\
         **Description:** {}\n\n\
         **Observed consequence:** {}\n\n\
         **Workaround:** {}\n\n\
         **Evidence:** {}\n\n",
        markdown_text(&finding.id),
        markdown_text(&finding.title),
        code_text(&finding.api_surface),
        classification_name(finding.classification),
        finding_status_name(finding.status),
        markdown_text(&finding.description),
        markdown_text(&finding.observed_consequence),
        markdown_text(&finding.workaround),
        render_refs(&finding.evidence_refs),
    ));
    if let Some(action) = &finding.action {
        output.push_str(&format!(
            "**Action owner:** {}\n\n\
             **Action:** {}\n\n\
             **Issue:** {}\n\n",
            markdown_text(&action.owner),
            markdown_text(&action.summary),
            action
                .issue_ref
                .as_deref()
                .map(markdown_text)
                .unwrap_or_else(|| "—".to_string()),
        ));
        if let Some(revision) = &action.revision {
            output.push_str(&format!(
                "**Action revision:** `{}`\n\n",
                code_text(revision)
            ));
        }
    } else {
        output.push_str("**Action:** —\n\n");
    }
    output.push_str(&format!(
        "**Verification status:** `{}`\n\n\
         **Verification summary:** {}\n\n\
         **Verification commands:** {}\n\n\
         **Verification evidence:** {}\n",
        verification_status_name(finding.verification.status),
        markdown_text(&finding.verification.summary),
        render_commands(&finding.verification.commands),
        render_refs(&finding.verification.evidence_refs),
    ));
}

fn render_verification(document: &FindingsDocument, output: &mut String) {
    output.push_str(
        "\n## Verification evidence\n\n\
         | Finding | Status | Commands | Evidence | Summary |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    let mut findings = document.findings.iter().collect::<Vec<_>>();
    findings.sort_by_key(|finding| &finding.id);
    for finding in findings {
        output.push_str(&format!(
            "| {} | `{}` | {} | {} | {} |\n",
            table_text(&finding.id),
            verification_status_name(finding.verification.status),
            render_commands(&finding.verification.commands),
            render_refs(&finding.verification.evidence_refs),
            table_text(&finding.verification.summary),
        ));
    }
}

fn render_runs(runs: &[&RunEvidence], output: &mut String) {
    output.push_str(
        "\n## Run evidence\n\n\
         Revision `git:self` denotes the commit containing the canonical findings file and is \
         reserved for post-commit verification evidence.\n\n\
         | ID | Kind | Status | Required | Command | Date | Revision | Provider | Model | Evidence |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for run in runs {
        output.push_str(&format!(
            "| {} | `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} |\n",
            table_text(&run.id),
            run_kind_name(run.kind),
            run_status_name(run.status),
            if run.required { "yes" } else { "no" },
            run.command
                .as_deref()
                .map(inline_code)
                .unwrap_or_else(|| "—".to_string()),
            run.date
                .as_deref()
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
            run.revision
                .as_deref()
                .map(inline_code)
                .unwrap_or_else(|| "—".to_string()),
            run.provider
                .as_deref()
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
            run.model
                .as_deref()
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
            render_refs(&run.evidence_refs),
        ));
    }
    for run in runs {
        output.push_str(&format!(
            "\n#### {}\n\n{}\n",
            markdown_text(&run.id),
            markdown_text(&run.summary)
        ));
        if let Some(diagnostic) = &run.diagnostic_excerpt {
            output.push_str(&format!(
                "\n**Redacted diagnostic:** {}\n",
                markdown_text(diagnostic)
            ));
        }
    }
}

fn render_live_boundary(runs: &[&RunEvidence], output: &mut String) {
    output.push_str("\n### Live-provider boundary\n");
    let live_runs = runs
        .iter()
        .copied()
        .filter(|run| run.kind == RunKind::LiveProvider)
        .collect::<Vec<_>>();
    if live_runs.is_empty() {
        output.push_str("\nNo live-provider run is declared.\n");
    } else {
        for run in live_runs {
            output.push_str(&format!(
                "\n- **{}** — `{}`; provider `{}`; model `{}`; {}\n",
                markdown_text(&run.id),
                run_status_name(run.status),
                code_text(run.provider.as_deref().unwrap_or("not-applicable")),
                code_text(run.model.as_deref().unwrap_or("not-applicable")),
                markdown_text(&run.summary),
            ));
        }
    }
}

fn render_evidence(evidence: &mut [&Evidence], output: &mut String) {
    output.push_str(
        "\n## Evidence catalogue\n\n\
         | ID | Kind | Locator | Run | Command | Result | Summary |\n\
         | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    evidence.sort_by_key(|item| &item.id);
    for item in evidence {
        output.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} | {} |\n",
            table_text(&item.id),
            evidence_kind_name(item.kind),
            evidence_locator(item),
            item.run_ref
                .as_deref()
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
            item.command
                .as_deref()
                .map(inline_code)
                .unwrap_or_else(|| "—".to_string()),
            item.result
                .as_deref()
                .map(table_text)
                .unwrap_or_else(|| "—".to_string()),
            table_text(&item.summary),
        ));
    }
}

fn render_implications(document: &FindingsDocument, output: &mut String) {
    let mut live_gates = document
        .runs
        .iter()
        .filter(|run| {
            run.required && run.kind == RunKind::LiveProvider && run.status != RunStatus::Passed
        })
        .map(|run| run.id.as_str())
        .collect::<Vec<_>>();
    live_gates.sort_unstable();
    let mut release_blockers = document
        .findings
        .iter()
        .filter(|finding| {
            finding.classification == FindingClassification::ReleaseBlocker
                && finding.status != FindingStatus::Verified
        })
        .collect::<Vec<_>>();
    release_blockers.sort_by_key(|finding| &finding.id);
    let mut seam_blockers = document
        .findings
        .iter()
        .filter(|finding| {
            finding.classification == FindingClassification::SeamBlocker
                && finding.status != FindingStatus::Verified
        })
        .collect::<Vec<_>>();
    seam_blockers.sort_by_key(|finding| &finding.id);
    let mut backlog = document
        .findings
        .iter()
        .filter(|finding| finding.classification == FindingClassification::Post10Backlog)
        .collect::<Vec<_>>();
    backlog.sort_by_key(|finding| &finding.id);

    output.push_str("\n## v1.0 and Multivac M2 implications\n\n### v1.0\n");
    output.push_str(&format!(
        "\nReadiness remains `{}`: {}",
        readiness_name(document.readiness_verdict.status),
        document
            .readiness_verdict
            .reason
            .as_deref()
            .map(markdown_text)
            .unwrap_or_else(|| "no additional reason recorded".to_string()),
    ));
    if !live_gates.is_empty() {
        output.push_str(&format!(
            "\n\nRequired live-provider gates: {}.",
            render_plain_refs(&live_gates)
        ));
    }
    if !release_blockers.is_empty() {
        output.push_str("\n\nUnresolved release blockers:\n");
        for finding in release_blockers {
            output.push_str(&format!(
                "\n- {} — {} (`{}`, {})",
                markdown_text(&finding.id),
                markdown_text(&finding.title),
                finding_status_name(finding.status),
                finding_issue(finding),
            ));
        }
        output.push('\n');
    }

    output.push_str("\n### Multivac M2\n");
    if seam_blockers.is_empty() {
        output.push_str("\nNo unresolved seam blockers are recorded.\n");
    } else {
        output.push_str("\nUnresolved supervised-delegation seam blockers:\n");
        for finding in seam_blockers {
            output.push_str(&format!(
                "\n- {} — {} (`{}`, {})",
                markdown_text(&finding.id),
                markdown_text(&finding.title),
                finding_status_name(finding.status),
                finding_issue(finding),
            ));
        }
        output.push('\n');
    }

    output.push_str("\n### Post-1.0 backlog\n");
    if backlog.is_empty() {
        output.push_str("\nNo post-1.0 backlog findings are recorded.\n");
    } else {
        for finding in backlog {
            output.push_str(&format!(
                "\n- {} — {} (`{}`, {})",
                markdown_text(&finding.id),
                markdown_text(&finding.title),
                finding_status_name(finding.status),
                finding_issue(finding),
            ));
        }
        output.push('\n');
    }
}

fn finding_issue(finding: &Finding) -> String {
    finding
        .action
        .as_ref()
        .and_then(|action| action.issue_ref.as_deref())
        .map(markdown_text)
        .unwrap_or_else(|| "no issue recorded".to_string())
}

fn evidence_locator(evidence: &Evidence) -> String {
    match (&evidence.path, &evidence.symbol) {
        (Some(path), Some(symbol)) => {
            format!("`{}` · `{}`", code_text(path), code_text(symbol))
        }
        (Some(path), None) => inline_code(path),
        (None, Some(symbol)) => inline_code(symbol),
        (None, None) => evidence
            .run_ref
            .as_deref()
            .map(table_text)
            .unwrap_or_else(|| "—".to_string()),
    }
}

fn render_refs(refs: &[String]) -> String {
    let mut refs = refs.iter().map(String::as_str).collect::<Vec<_>>();
    refs.sort_unstable();
    render_plain_refs(&refs)
}

fn render_plain_refs(refs: &[&str]) -> String {
    if refs.is_empty() {
        return "—".to_string();
    }
    refs.iter()
        .map(|reference| inline_code(reference))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_commands(commands: &[String]) -> String {
    let mut commands = commands.iter().map(String::as_str).collect::<Vec<_>>();
    commands.sort_unstable();
    if commands.is_empty() {
        "—".to_string()
    } else {
        commands
            .into_iter()
            .map(inline_code)
            .collect::<Vec<_>>()
            .join("<br>")
    }
}

fn markdown_text(value: &str) -> String {
    value.replace('\r', "").replace('\n', " ")
}

fn table_text(value: &str) -> String {
    markdown_text(value).replace('|', "\\|")
}

fn code_text(value: &str) -> String {
    table_text(value).replace('`', "\\`")
}

fn inline_code(value: &str) -> String {
    format!("`{}`", code_text(value))
}

fn readiness_name(status: ReadinessStatus) -> &'static str {
    match status {
        ReadinessStatus::Ready => "ready",
        ReadinessStatus::Conditional => "conditional",
        ReadinessStatus::Blocked => "blocked",
        ReadinessStatus::Unverified => "unverified",
    }
}

fn checklist_status_name(status: ChecklistStatus) -> &'static str {
    match status {
        ChecklistStatus::Planned => "planned",
        ChecklistStatus::Exercised => "exercised",
        ChecklistStatus::Failed => "failed",
        ChecklistStatus::Blocked => "blocked",
        ChecklistStatus::NotApplicable => "not-applicable",
    }
}

fn run_kind_name(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Fixture => "fixture",
        RunKind::Test => "test",
        RunKind::Smoke => "smoke",
        RunKind::LiveProvider => "live-provider",
    }
}

fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Passed => "passed",
        RunStatus::Failed => "failed",
        RunStatus::NotRun => "not-run",
        RunStatus::Partial => "partial",
    }
}

fn evidence_kind_name(kind: EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Source => "source",
        EvidenceKind::Test => "test",
        EvidenceKind::SmokeRun => "smoke-run",
        EvidenceKind::LiveRun => "live-run",
        EvidenceKind::Documentation => "documentation",
        EvidenceKind::RuntimeOutput => "runtime-output",
    }
}

fn classification_name(classification: FindingClassification) -> &'static str {
    match classification {
        FindingClassification::Untriaged => "untriaged",
        FindingClassification::SeamBlocker => "seam-blocker",
        FindingClassification::ReleaseBlocker => "release-blocker",
        FindingClassification::Post10Backlog => "post-1.0-backlog",
    }
}

fn finding_status_name(status: FindingStatus) -> &'static str {
    match status {
        FindingStatus::Open => "open",
        FindingStatus::Implemented => "implemented",
        FindingStatus::Verified => "verified",
        FindingStatus::Deferred => "deferred",
    }
}

fn verification_status_name(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::NotRun => "not-run",
        VerificationStatus::Passed => "passed",
        VerificationStatus::Failed => "failed",
        VerificationStatus::NotApplicable => "not-applicable",
    }
}
