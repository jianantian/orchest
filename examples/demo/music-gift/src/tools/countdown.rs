//! Countdown HTML generation — Agent-as-Tool subagent.
//!
//! A lightweight Orchest subagent that generates a birthday countdown HTML
//! block. It runs with `ContextMode::Fresh` (no parent conversation
//! context), receiving only the birthday/scenario info as structured input.
//!
//! The tool is built once at startup (see [`crate::config::build_countdown_tool`])
//! and reused across all `run_countdown` calls.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{Datelike, Local, NaiveDate};
use serde_json::Value;

use crate::gift::GiftStore;
use crate::prompts::COUNTDOWN_TEMPLATE;

/// Parameters for the countdown subagent.
pub struct CountdownParams {
    pub name: String,
    pub birthday: String,
    pub scenario: String,
    pub lyric_snippet: String,
}

impl CountdownParams {
    pub(crate) fn month_day(&self) -> Option<(String, String, i64, String)> {
        parse_birthday_info(&self.birthday)
    }
}

/// Where to save the generated HTML and how to update the gift status.
pub struct CountdownSink {
    pub store: GiftStore,
    pub data_dir: PathBuf,
    pub gift_id: String,
}

/// Execute the pre-built countdown subagent and save the result to disk.
pub async fn run_countdown(
    tool: Arc<dyn orchest::tool::Tool>,
    params: &CountdownParams,
    sink: &CountdownSink,
) -> Result<(), String> {
    let input = serde_json::json!({
        "name": params.name,
        "birthday": params.birthday,
        "scenario": params.scenario,
        "lyric_snippet": params.lyric_snippet,
    });

    let result = tool
        .execute(input, &tool_context())
        .await
        .map_err(|e| e.to_string())?;

    // An agent-as-tool always answers with `Structured` (it carries the child
    // run's budget usage); `model_output` is what our output_extractor built,
    // i.e. `{ "html": ... }`. Matching only on `Immediate` made every single
    // countdown fail here.
    let html = match result {
        orchest::tool::ToolOutput::Immediate(value)
        | orchest::tool::ToolOutput::Structured {
            model_output: value,
            ..
        } => value
            .get("html")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        other => return Err(format!("unexpected countdown tool output: {other:?}")),
    };

    if html.is_empty() {
        return Err("countdown subagent produced empty HTML".into());
    }

    let dir = sink.data_dir.join("countdown");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create countdown dir: {e}"))?;

    let path = dir.join(format!("{}.html", sink.gift_id));
    std::fs::write(&path, &html).map_err(|e| format!("write countdown HTML: {e}"))?;

    sink.store
        .update_countdown_status(&sink.gift_id, "ready")
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

pub(crate) fn countdown_params_from_json(input: &Value) -> CountdownParams {
    CountdownParams {
        name: input
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(crate::gift::GiftMeta::DEFAULT_NAME)
            .into(),
        birthday: input
            .get("birthday")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        scenario: input
            .get("scenario")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        lyric_snippet: input
            .get("lyric_snippet")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
    }
}

pub(crate) fn build_prompt(p: &CountdownParams) -> String {
    let (month, day, days_until, target_date) = p
        .month_day()
        .unwrap_or_else(|| ("Jan".into(), "1".into(), 0, "2025-01-01".into()));

    let template = COUNTDOWN_TEMPLATE.as_str();
    if template.is_empty() {
        return format!(
            "Generate a birthday countdown HTML for {} whose birthday is {} {} ({} days away). Target: {}. Scene: {}.",
            p.name, month, day, days_until, target_date, p.scenario
        );
    }

    template
        .replace("{name}", &p.name)
        .replace("{scenario}", &p.scenario)
        .replace("{month}", &month)
        .replace("{day}", &day)
        .replace("{days_until}", &days_until.to_string())
        .replace("{target_date}", &target_date)
        .replace("{lyric_snippet}", &p.lyric_snippet)
        .replace("{previous_error}", "")
}

fn parse_birthday_info(birthday: &str) -> Option<(String, String, i64, String)> {
    let parts: Vec<&str> = birthday.split('-').collect();
    let (month_str, day_str) = if parts.len() == 3 {
        (parts[1], parts[2])
    } else if parts.len() == 2 {
        (parts[0], parts[1])
    } else {
        return None;
    };
    let month_num: u32 = month_str.parse().ok()?;
    let day_num: u32 = day_str.parse().ok()?;
    if !(1..=12).contains(&month_num) || !(1..=31).contains(&day_num) {
        return None;
    }
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = months.get(month_num as usize - 1)?;
    let today = Local::now().date_naive();
    let target = next_birthday(today, month_num, day_num)?;
    let days_until = (target - today).num_days();
    Some((
        month_name.to_string(),
        day_str.to_string(),
        days_until,
        target.format("%Y-%m-%d").to_string(),
    ))
}

/// The next local calendar occurrence of a birthday month/day (today counts —
/// a birthday today is 0 days away). Calendar addition, not `+365`: a
/// birthday already past this year rolls to next year, so the target date is
/// always in the future and leap years are correct (the old code added 365
/// days to the day count while leaving the target in the past).
///
/// Feb 29 birthdays are observed on Feb 28 in non-leap years (documented
/// choice — the countdown must point at a real date every year), and get the
/// real Feb 29 whenever the upcoming occurrence falls in a leap year.
fn next_birthday(today: NaiveDate, month: u32, day: u32) -> Option<NaiveDate> {
    let in_year = |year: i32| {
        NaiveDate::from_ymd_opt(year, month, day).or_else(|| {
            if month == 2 && day == 29 {
                NaiveDate::from_ymd_opt(year, 2, 28)
            } else {
                None
            }
        })
    };
    let this_year = in_year(today.year())?;
    if this_year >= today {
        Some(this_year)
    } else {
        in_year(today.year() + 1)
    }
}

pub(crate) fn strip_code_fences(html: &str) -> String {
    let trimmed = html.trim();
    let without_open = trimmed
        .strip_prefix("```html")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_close = without_open.strip_suffix("```").unwrap_or(without_open);
    without_close.trim().to_string()
}

pub(crate) fn tool_context() -> orchest::tool::ToolContext {
    orchest::tool::ToolContext {
        run_id: orchest::run::RunId::new(),
        run_depth: 0,
        tool_call_id: "countdown".into(),
        event_tx: None,
        webhook_base_url: None,
        approval_bus: orchest::run::ApprovalBus::default(),
        remaining_budget: Default::default(),
        parent_messages: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("valid test date")
    }

    #[test]
    fn next_birthday_later_this_year_stays_this_year() {
        let target = next_birthday(d("2026-07-26"), 12, 31).expect("target");
        assert_eq!(target, d("2026-12-31"));
    }

    /// The old `+365` bug: a passed birthday kept this year's target date
    /// while reporting a positive day count. Now the target rolls forward.
    #[test]
    fn next_birthday_rolls_to_next_year() {
        let target = next_birthday(d("2026-12-30"), 1, 5).expect("target");
        assert_eq!(target, d("2027-01-05"));
    }

    /// Rolling across a leap day must count it: 2028 is a leap year, so a
    /// Mar 1 birthday from Dec 2027 is 366/365-aware by construction.
    #[test]
    fn next_birthday_across_leap_day() {
        let target = next_birthday(d("2027-12-31"), 3, 1).expect("target");
        assert_eq!(target, d("2028-03-01"));
        assert_eq!((target - d("2027-12-31")).num_days(), 61);
    }

    #[test]
    fn next_birthday_today_is_zero_days() {
        let today = d("2026-07-26");
        let target = next_birthday(today, 7, 26).expect("target");
        assert_eq!(target, today);
        assert_eq!((target - today).num_days(), 0);
    }

    /// Documented 2/29 behavior: observed on Feb 28 in non-leap years.
    #[test]
    fn feb29_observed_on_feb28_in_non_leap_year() {
        let target = next_birthday(d("2026-07-01"), 2, 29).expect("target");
        assert_eq!(target, d("2027-02-28"));
    }

    /// When the next occurrence is in a leap year, the real Feb 29 is used.
    #[test]
    fn feb29_gets_real_leap_day_when_leap_year_is_next() {
        let target = next_birthday(d("2027-03-01"), 2, 29).expect("target");
        assert_eq!(target, d("2028-02-29"));
    }

    #[test]
    fn feb29_in_leap_year_itself() {
        let target = next_birthday(d("2028-01-01"), 2, 29).expect("target");
        assert_eq!(target, d("2028-02-29"));
    }

    #[test]
    fn invalid_month_day_rejected() {
        assert!(next_birthday(d("2026-01-01"), 2, 30).is_none());
        assert!(next_birthday(d("2026-01-01"), 4, 31).is_none());
    }

    #[test]
    fn parse_birthday_info_accepts_md_and_ymd() {
        let (month, day, days, target) = parse_birthday_info("12-31").expect("parsed");
        assert_eq!(month, "Dec");
        assert_eq!(day, "31");
        assert!(days >= 0, "days_until must never be negative");
        // Target date must agree with the day count (the +365 bug had the
        // target in the past while days pointed ahead).
        let target_date = NaiveDate::parse_from_str(&target, "%Y-%m-%d").expect("target date");
        assert_eq!((target_date - Local::now().date_naive()).num_days(), days);

        let (_, day3, _, _) = parse_birthday_info("1990-04-20").expect("parsed");
        assert_eq!(day3, "20");
    }

    #[test]
    fn parse_birthday_info_rejects_garbage() {
        assert!(parse_birthday_info("not-a-date").is_none());
        assert!(parse_birthday_info("13-01").is_none());
        assert!(parse_birthday_info("02-30").is_none());
        assert!(parse_birthday_info("").is_none());
    }
}
