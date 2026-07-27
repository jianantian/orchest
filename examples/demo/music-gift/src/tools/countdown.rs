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
    /// Why the previous attempt was rejected; set only on the retry. Rides
    /// through the tool input JSON into the prompt's `{previous_error}` slot.
    pub previous_error: Option<String>,
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
    let html = generate_html(&tool, params).await?;

    let dir = sink.data_dir.join("countdown");
    write_html_atomic(&dir, &sink.gift_id, &html)?;

    sink.store
        .update_countdown_status(&sink.gift_id, "ready")
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Generate the countdown HTML, retrying once with the failure reason fed
/// back into the prompt's `{previous_error}` slot (the template reserves it
/// for exactly this). Both validation failures (empty / truncated output) and
/// tool execution errors get the one retry; a second failure is returned so
/// the caller can mark the countdown `failed`.
async fn generate_html(
    tool: &Arc<dyn orchest::tool::Tool>,
    params: &CountdownParams,
) -> Result<String, String> {
    match generate_once(tool, params, None).await {
        Ok(html) => Ok(html),
        Err(first_err) => {
            eprintln!("[music-gift] countdown attempt failed, retrying once: {first_err}");
            generate_once(tool, params, Some(&first_err)).await
        }
    }
}

/// One generation attempt: run the subagent, extract the HTML, validate its
/// completeness. `previous_error` rides through the input JSON to the tool's
/// input_mapper, which rebuilds the prompt with the retry note filled in.
async fn generate_once(
    tool: &Arc<dyn orchest::tool::Tool>,
    params: &CountdownParams,
    previous_error: Option<&str>,
) -> Result<String, String> {
    let mut input = serde_json::json!({
        "name": params.name,
        "birthday": params.birthday,
        "scenario": params.scenario,
        "lyric_snippet": params.lyric_snippet,
    });
    if let Some(err) = previous_error {
        input["previous_error"] = Value::String(err.to_string());
    }

    // A failed child run arrives as `Err(ToolError)` (v0.15, issue 003) whose
    // message carries the child_run_id, the failure reason, and the consumed
    // budget — no more `details["error"]` scraping. Propagating it lets
    // `generate_html` retry once with this reason fed into the prompt.
    let result = match tool.call_oneshot(input).await {
        Ok(output) => output,
        Err(e) => return Err(e.to_string()),
    };

    // On success an agent-as-tool always answers with `Structured` (it carries
    // the child run's budget usage); `model_output` is what our
    // output_extractor built, i.e. `{ "html": ... }`. `Immediate` is accepted
    // for test doubles. Matching only on `Immediate` made every single
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

    validate_html(&html)?;
    Ok(html)
}

/// The output contract is `<style>…</style>` + `<div>…</div>` +
/// `<script>…</script>` in that order, so a block not ending in `</script>`
/// was cut off by the model's output-token cap — and an unclosed script tag
/// kills every timer in the scene. Rejecting it here keeps truncated HTML
/// from landing on disk as `ready`.
fn validate_html(html: &str) -> Result<(), String> {
    let trimmed = html.trim();
    if trimmed.is_empty() {
        return Err("countdown subagent produced empty HTML".into());
    }
    if !trimmed.ends_with("</script>") {
        return Err("truncated countdown HTML: does not end with </script>".into());
    }
    Ok(())
}

/// Write the block as `<dir>/<gift_id>.html` atomically: temp file in the
/// same directory, then rename. A crash mid-write leaves a stray `.tmp` file
/// instead of a half-written page the route would happily serve.
fn write_html_atomic(dir: &std::path::Path, gift_id: &str, html: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create countdown dir: {e}"))?;
    let path = dir.join(format!("{gift_id}.html"));
    let tmp = dir.join(format!("{gift_id}.html.tmp"));
    std::fs::write(&tmp, html).map_err(|e| format!("write countdown HTML: {e}"))?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("move countdown HTML into place: {e}"));
    }
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
        previous_error: input
            .get("previous_error")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
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

    let previous_error = p
        .previous_error
        .as_deref()
        .map(|err| {
            format!(
                "\n【RETRY — the previous attempt failed】\nReason: {err}\nRegenerate the complete block: non-empty, ending with </script>."
            )
        })
        .unwrap_or_default();

    template
        .replace("{name}", &p.name)
        .replace("{scenario}", &p.scenario)
        .replace("{month}", &month)
        .replace("{day}", &day)
        .replace("{days_until}", &days_until.to_string())
        .replace("{target_date}", &target_date)
        .replace("{lyric_snippet}", &p.lyric_snippet)
        .replace("{previous_error}", &previous_error)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("valid test date")
    }

    fn sample_params() -> CountdownParams {
        CountdownParams {
            name: "Momo".into(),
            birthday: "12-31".into(),
            scenario: "walking the dog in the first snow".into(),
            lyric_snippet: "snow falls quietly on the rooftops".into(),
            previous_error: None,
        }
    }

    const GOOD_HTML: &str = "<style>#birthday-section{}</style><div id=\"birthday-section\"></div><script>(function(){})();</script>";
    /// The classic output-token-cap cut: the script never closes.
    const TRUNCATED_HTML: &str =
        "<style>#birthday-section{}</style><div id=\"birthday-section\"></div><script>(function(){";

    #[test]
    fn validate_html_accepts_complete_block() {
        assert!(validate_html(GOOD_HTML).is_ok());
        // Trailing whitespace after </script> is fine.
        assert!(validate_html(&format!("{GOOD_HTML}\n  ")).is_ok());
    }

    #[test]
    fn validate_html_rejects_empty_and_truncated() {
        assert!(validate_html("").unwrap_err().contains("empty"));
        assert!(validate_html("  \n ").unwrap_err().contains("empty"));
        let err = validate_html(TRUNCATED_HTML).unwrap_err();
        assert!(err.contains("</script>"), "truncation reason: {err}");
    }

    #[test]
    fn build_prompt_leaves_no_retry_note_without_error() {
        let prompt = build_prompt(&sample_params());
        assert!(!prompt.contains("{previous_error}"));
        assert!(!prompt.contains("RETRY"));
    }

    #[test]
    fn build_prompt_fills_retry_note_with_previous_error() {
        let mut p = sample_params();
        p.previous_error = Some("truncated countdown HTML: does not end with </script>".into());
        let prompt = build_prompt(&p);
        assert!(prompt.contains("RETRY"));
        assert!(prompt.contains("does not end with </script>"));
    }

    /// The retry path depends on the template keeping this slot.
    #[test]
    fn template_keeps_previous_error_slot() {
        assert!(crate::prompts::COUNTDOWN_TEMPLATE.contains("{previous_error}"));
    }

    /// A scripted stand-in for the countdown subagent: answers with the
    /// queued outputs in call order and records every input it saw.
    struct ScriptedTool {
        outputs: Vec<Result<String, String>>,
        calls: Mutex<Vec<Value>>,
        metadata: orchest::tool::ToolMetadata,
    }

    impl ScriptedTool {
        fn new(outputs: Vec<Result<String, String>>) -> Arc<Self> {
            Arc::new(Self {
                outputs,
                calls: Mutex::new(Vec::new()),
                metadata: orchest::tool::ToolMetadata::default(),
            })
        }
    }

    #[async_trait::async_trait]
    impl orchest::tool::Tool for ScriptedTool {
        fn name(&self) -> &str {
            "scripted"
        }
        fn description(&self) -> &str {
            "scripted countdown tool"
        }
        fn input_schema(&self) -> &orchest::tool::JsonSchema {
            &Value::Null
        }
        fn output_schema(&self) -> Option<&orchest::tool::JsonSchema> {
            None
        }
        fn metadata(&self) -> &orchest::tool::ToolMetadata {
            &self.metadata
        }
        async fn execute(
            &self,
            input: Value,
            _ctx: &orchest::tool::ToolContext,
        ) -> Result<orchest::tool::ToolOutput, orchest::tool::ToolError> {
            let mut calls = self.calls.lock().expect("calls lock");
            calls.push(input);
            match self.outputs.get(calls.len() - 1) {
                Some(Ok(html)) => Ok(orchest::tool::ToolOutput::Immediate(
                    serde_json::json!({ "html": html }),
                )),
                Some(Err(e)) => Err(orchest::tool::ToolError::fatal(e.clone())),
                None => Err(orchest::tool::ToolError::fatal("scripted tool exhausted")),
            }
        }
    }

    #[tokio::test]
    async fn valid_first_attempt_skips_retry() {
        let scripted = ScriptedTool::new(vec![Ok(GOOD_HTML.into())]);
        let tool: Arc<dyn orchest::tool::Tool> = scripted.clone();
        let html = generate_html(&tool, &sample_params())
            .await
            .expect("valid html");
        assert_eq!(html, GOOD_HTML);
        assert_eq!(scripted.calls.lock().expect("calls").len(), 1);
    }

    #[tokio::test]
    async fn retry_recovers_from_truncated_first_attempt() {
        let scripted = ScriptedTool::new(vec![Ok(TRUNCATED_HTML.into()), Ok(GOOD_HTML.into())]);
        let tool: Arc<dyn orchest::tool::Tool> = scripted.clone();
        let html = generate_html(&tool, &sample_params())
            .await
            .expect("retry succeeds");
        assert_eq!(html, GOOD_HTML);
        let calls = scripted.calls.lock().expect("calls");
        assert_eq!(calls.len(), 2);
        // The first attempt carries no error; the retry must name the failure
        // so the model knows what to fix.
        assert!(calls[0].get("previous_error").is_none());
        let err = calls[1]["previous_error"].as_str().unwrap_or("");
        assert!(
            err.contains("</script>"),
            "retry input should explain the truncation: {err}"
        );
    }

    #[tokio::test]
    async fn tool_error_is_retried_too() {
        let scripted = ScriptedTool::new(vec![
            Err("provider overloaded".into()),
            Ok(GOOD_HTML.into()),
        ]);
        let tool: Arc<dyn orchest::tool::Tool> = scripted.clone();
        let html = generate_html(&tool, &sample_params())
            .await
            .expect("retry succeeds");
        assert_eq!(html, GOOD_HTML);
        let calls = scripted.calls.lock().expect("calls");
        assert_eq!(calls.len(), 2);
        let err = calls[1]["previous_error"].as_str().unwrap_or("");
        assert!(err.contains("provider overloaded"), "retry reason: {err}");
    }

    #[tokio::test]
    async fn second_failure_surfaces_error() {
        let scripted = ScriptedTool::new(vec![Ok(TRUNCATED_HTML.into()), Ok(String::new())]);
        let tool: Arc<dyn orchest::tool::Tool> = scripted.clone();
        let err = generate_html(&tool, &sample_params()).await.unwrap_err();
        assert!(
            err.contains("empty"),
            "expected the second failure reason, got: {err}"
        );
        assert_eq!(scripted.calls.lock().expect("calls").len(), 2);
    }

    #[test]
    fn write_html_atomic_leaves_full_file_and_no_tmp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("countdown");
        write_html_atomic(&dir, "gift-1", GOOD_HTML).expect("write");
        let written = std::fs::read_to_string(dir.join("gift-1.html")).expect("read back");
        assert_eq!(written, GOOD_HTML);
        assert!(
            !dir.join("gift-1.html.tmp").exists(),
            "temp file must be renamed away"
        );
    }

    #[test]
    fn write_html_atomic_overwrites_existing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("countdown");
        write_html_atomic(&dir, "gift-1", "old").expect("first write");
        write_html_atomic(&dir, "gift-1", GOOD_HTML).expect("second write");
        let written = std::fs::read_to_string(dir.join("gift-1.html")).expect("read back");
        assert_eq!(written, GOOD_HTML);
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
