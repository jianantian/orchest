use std::collections::{HashMap, HashSet};

use super::super::*;

pub(super) const MAX_DIAGNOSTIC_LENGTH: usize = 1_000;
const MAX_TEXT_LENGTH: usize = 4_000;

pub fn is_safe_repository_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(':')
        && path
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

pub(super) fn validate_repository_path(path: &str) -> Result<(), FindingsError> {
    if is_safe_repository_path(path) {
        Ok(())
    } else {
        invalid(format!("unsafe repository path {path:?}"))
    }
}

pub(super) fn is_stable_symbol(symbol: &str) -> bool {
    let symbol = symbol.trim();
    !symbol.is_empty() && !is_line_locator(symbol)
}

fn is_line_locator(symbol: &str) -> bool {
    let lower = symbol.trim().to_ascii_lowercase();
    suffix_is_line_number(&lower)
        || lower
            .rsplit_once([':', '#'])
            .is_some_and(|(_, suffix)| suffix_is_line_number(suffix))
}

fn suffix_is_line_number(suffix: &str) -> bool {
    let suffix = strip_line_prefix(suffix);
    is_decimal(suffix) || is_numeric_range(suffix)
}

fn is_numeric_range(value: &str) -> bool {
    let leading_digits = value
        .bytes()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if leading_digits == 0 || leading_digits == value.len() {
        return false;
    }

    let through_separator = value.trim_end_matches(|character: char| character.is_ascii_digit());
    if through_separator.len() == value.len() {
        return false;
    }

    let separator = &through_separator[leading_digits..];
    !separator.is_empty()
        && separator
            .chars()
            .all(|character| !character.is_ascii_digit())
}

fn strip_line_prefix(value: &str) -> &str {
    let value = value.trim();
    value
        .strip_prefix("lines")
        .or_else(|| value.strip_prefix("line"))
        .or_else(|| value.strip_prefix('l'))
        .unwrap_or(value)
        .trim()
}

pub(super) fn validate_issue_ref(issue_ref: &str) -> Result<(), FindingsError> {
    required("action issueRef", issue_ref)?;
    if issue_ref.strip_prefix('#').is_some_and(is_decimal) {
        Ok(())
    } else {
        invalid(format!(
            "action issueRef {issue_ref:?} must use canonical #<number> form"
        ))
    }
}

pub(super) fn validate_date(date: &str) -> Result<(), FindingsError> {
    required("executed run date", date)?;
    let parts = date.split('-').collect::<Vec<_>>();
    let valid_shape = parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|part| is_decimal(part));
    if !valid_shape {
        return invalid(format!("executed run date {date:?} must use YYYY-MM-DD"));
    }
    let year = parts[0]
        .parse::<u16>()
        .map_err(|_| FindingsError::Validation(format!("invalid calendar date {date:?}")))?;
    let month = parts[1]
        .parse::<u8>()
        .map_err(|_| FindingsError::Validation(format!("invalid calendar date {date:?}")))?;
    let day = parts[2]
        .parse::<u8>()
        .map_err(|_| FindingsError::Validation(format!("invalid calendar date {date:?}")))?;
    let maximum_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    };
    if day == 0 || day > maximum_day {
        return invalid(format!("invalid calendar date {date:?}"));
    }
    Ok(())
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}

pub(super) fn validate_revision(
    name: &str,
    revision: &str,
    allow_git_self: bool,
) -> Result<(), FindingsError> {
    required(name, revision)?;
    if revision == "git:self" {
        if allow_git_self {
            return Ok(());
        }
        return invalid(format!(
            "{name} cannot use git:self; it is reserved for post-commit verification evidence"
        ));
    }
    if (7..=40).contains(&revision.len())
        && revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        invalid(format!(
            "{name} must be git:self or a 7-40 character lowercase hexadecimal Git object id"
        ))
    }
}

pub(super) fn validate_distinct_refs(owner: &str, refs: &[String]) -> Result<(), FindingsError> {
    let mut seen = HashSet::new();
    for reference in refs {
        required(owner, reference)?;
        if !seen.insert(reference.as_str()) {
            return invalid(format!("{owner} contains duplicate ref {reference}"));
        }
    }
    Ok(())
}

pub(super) fn validate_private_text(name: &str, value: &str) -> Result<(), FindingsError> {
    let lower = value.to_ascii_lowercase();
    let secret_shaped = lower.contains("authorization: bearer")
        || lower.contains("api_key=")
        || lower.contains("api-key:")
        || lower.contains("-----begin private key-----")
        || lower
            .split(|character: char| character.is_whitespace() || character == '"')
            .any(|word| {
                word.strip_prefix("sk-")
                    .is_some_and(|secret| secret.len() >= 16)
            });
    if secret_shaped {
        return invalid(format!("{name} contains a secret-shaped value"));
    }

    let machine_local = value.contains("/Users/")
        || value.contains("/home/")
        || value.contains("/tmp/")
        || value.contains("\\Users\\")
        || value
            .as_bytes()
            .get(1..3)
            .is_some_and(|bytes| bytes == b":\\" || bytes == b":/");
    if machine_local {
        return invalid(format!("{name} contains a machine-local path"));
    }
    Ok(())
}

pub(super) fn validate_rendered_strings(document: &FindingsDocument) -> Result<(), FindingsError> {
    validate_private_values([
        document.kind.as_str(),
        document.iteration.as_str(),
        document.subject.as_str(),
        document.executive_summary.as_str(),
    ])?;
    validate_private_values(document.readiness_verdict.reason.iter().map(String::as_str))?;
    validate_private_values(document.readiness_verdict.refs.iter().map(String::as_str))?;

    for item in &document.api_checklist {
        validate_private_values([
            item.id.as_str(),
            item.api_surface.as_str(),
            item.public_path.as_str(),
            item.requirement.as_str(),
        ])?;
        validate_private_values(item.evidence_refs.iter().map(String::as_str))?;
        validate_private_values(item.finding_refs.iter().map(String::as_str))?;
    }
    for run in &document.runs {
        validate_private_values([run.id.as_str(), run.summary.as_str()])?;
        validate_private_values(
            [
                run.command.as_deref(),
                run.date.as_deref(),
                run.revision.as_deref(),
                run.provider.as_deref(),
                run.model.as_deref(),
                run.diagnostic_excerpt.as_deref(),
            ]
            .into_iter()
            .flatten(),
        )?;
        validate_private_values(run.evidence_refs.iter().map(String::as_str))?;
    }
    for evidence in &document.evidence {
        validate_private_values([evidence.id.as_str(), evidence.summary.as_str()])?;
        validate_private_values(
            [
                evidence.path.as_deref(),
                evidence.symbol.as_deref(),
                evidence.command.as_deref(),
                evidence.run_ref.as_deref(),
                evidence.result.as_deref(),
            ]
            .into_iter()
            .flatten(),
        )?;
    }
    for finding in &document.findings {
        validate_private_values([
            finding.id.as_str(),
            finding.title.as_str(),
            finding.api_surface.as_str(),
            finding.description.as_str(),
            finding.observed_consequence.as_str(),
            finding.workaround.as_str(),
            finding.verification.summary.as_str(),
        ])?;
        validate_private_values(finding.evidence_refs.iter().map(String::as_str))?;
        validate_private_values(finding.verification.commands.iter().map(String::as_str))?;
        validate_private_values(
            finding
                .verification
                .evidence_refs
                .iter()
                .map(String::as_str),
        )?;
        if let Some(action) = &finding.action {
            validate_private_values([action.owner.as_str(), action.summary.as_str()])?;
            validate_private_values(
                [action.issue_ref.as_deref(), action.revision.as_deref()]
                    .into_iter()
                    .flatten(),
            )?;
        }
    }
    Ok(())
}

fn validate_private_values<'a>(
    values: impl IntoIterator<Item = &'a str>,
) -> Result<(), FindingsError> {
    for value in values {
        validate_private_text("rendered field", value)?;
    }
    Ok(())
}

pub(super) fn validate_unique<'a>(
    kind: &str,
    ids: impl Iterator<Item = &'a String>,
) -> Result<(), FindingsError> {
    let mut seen = HashSet::new();
    for id in ids {
        required(&format!("{kind} id"), id)?;
        if !seen.insert(id.as_str()) {
            return invalid(format!("duplicate {kind} id {id}"));
        }
    }
    Ok(())
}

pub(super) fn validate_evidence_refs(
    owner: &str,
    refs: &[String],
    evidence_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    for reference in refs {
        ensure_ref(owner, reference, evidence_ids, "evidence")?;
    }
    Ok(())
}

pub(super) fn validate_finding_refs(
    owner: &str,
    refs: &[String],
    finding_ids: &HashSet<&str>,
) -> Result<(), FindingsError> {
    for reference in refs {
        ensure_ref(owner, reference, finding_ids, "finding")?;
    }
    Ok(())
}

pub(super) fn ensure_ref(
    owner: &str,
    reference: &str,
    known_ids: &HashSet<&str>,
    target: &str,
) -> Result<(), FindingsError> {
    if known_ids.contains(reference) {
        Ok(())
    } else {
        invalid(format!("{owner} has unknown {target} ref {reference}"))
    }
}

pub(super) fn map_get<'a, T>(
    map: &HashMap<&str, &'a T>,
    key: &str,
    kind: &str,
) -> Result<&'a T, FindingsError> {
    map.get(key)
        .copied()
        .ok_or_else(|| FindingsError::Validation(format!("{kind} ref disappeared")))
}

pub(super) fn required(name: &str, value: &str) -> Result<(), FindingsError> {
    bounded(name, value, MAX_TEXT_LENGTH)?;
    if value.trim().is_empty() {
        invalid(format!("{name} is required"))
    } else {
        Ok(())
    }
}

pub(super) fn bounded(name: &str, value: &str, maximum: usize) -> Result<(), FindingsError> {
    if value.len() > maximum {
        invalid(format!("{name} exceeds {maximum} characters"))
    } else {
        Ok(())
    }
}

pub(super) fn invalid<T>(message: impl Into<String>) -> Result<T, FindingsError> {
    Err(FindingsError::Validation(message.into()))
}

fn is_decimal(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
}

pub(super) fn checklist_status_name(status: ChecklistStatus) -> &'static str {
    match status {
        ChecklistStatus::Planned => "planned",
        ChecklistStatus::Exercised => "exercised",
        ChecklistStatus::Failed => "failed",
        ChecklistStatus::Blocked => "blocked",
        ChecklistStatus::NotApplicable => "not-applicable",
    }
}

pub(super) fn verification_status_name(status: VerificationStatus) -> &'static str {
    match status {
        VerificationStatus::NotRun => "not-run",
        VerificationStatus::Passed => "passed",
        VerificationStatus::Failed => "failed",
        VerificationStatus::NotApplicable => "not-applicable",
    }
}
