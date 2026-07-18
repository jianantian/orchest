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

use chrono::Local;
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

    let result = tool.execute(input, &tool_context()).await.map_err(|e| e.to_string())?;

    // An agent-as-tool always answers with `Structured` (it carries the child
    // run's budget usage); `model_output` is what our output_extractor built,
    // i.e. `{ "html": ... }`. Matching only on `Immediate` made every single
    // countdown fail here.
    let html = match result {
        orchest::tool::ToolOutput::Immediate(value)
        | orchest::tool::ToolOutput::Structured { model_output: value, .. } => {
            value.get("html").and_then(Value::as_str).unwrap_or("").to_string()
        }
        other => return Err(format!("unexpected countdown tool output: {other:?}")),
    };

    if html.is_empty() {
        return Err("countdown subagent produced empty HTML".into());
    }

    let dir = sink.data_dir.join("countdown");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create countdown dir: {e}"))?;

    let path = dir.join(format!("{}.html", sink.gift_id));
    std::fs::write(&path, &html).map_err(|e| format!("write countdown HTML: {e}"))?;

    sink.store.update_countdown_status(&sink.gift_id, "ready").map_err(|e| e.to_string())?;

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────

pub(crate) fn countdown_params_from_json(input: &Value) -> CountdownParams {
    CountdownParams {
        name: input.get("name").and_then(Value::as_str).unwrap_or("Someone").into(),
        birthday: input.get("birthday").and_then(Value::as_str).unwrap_or("").into(),
        scenario: input.get("scenario").and_then(Value::as_str).unwrap_or("").into(),
        lyric_snippet: input.get("lyric_snippet").and_then(Value::as_str).unwrap_or("").into(),
    }
}

pub(crate) fn build_prompt(p: &CountdownParams) -> String {
    let (month, day, days_until, target_date) = p.month_day().unwrap_or_else(|| {
        ("Jan".into(), "1".into(), 0, "2025-01-01".into())
    });

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
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month_name = months.get(month_num as usize - 1)?;
    let now = Local::now().date_naive();
    let current_year = now.format("%Y").to_string();
    let target_str = format!("{current_year}-{month_num:02}-{day_num:02}");
    let target = chrono::NaiveDate::parse_from_str(&target_str, "%Y-%m-%d").ok()?;
    let mut days_until = (target - now).num_days();
    if days_until < 0 {
        days_until += 365;
    }
    Some((month_name.to_string(), day_str.to_string(), days_until, target_str))
}

pub(crate) fn strip_code_fences(html: &str) -> String {
    let trimmed = html.trim();
    let without_open = trimmed.strip_prefix("```html")
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
