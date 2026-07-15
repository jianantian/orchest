//! Countdown HTML generation — Agent-as-Tool subagent.
//!
//! A lightweight Orchest subagent that generates a birthday countdown HTML
//! block. It runs with [`ContextMode::Fresh`] (no parent conversation
//! context), receiving only the birthday/scenario info as structured input.
//!
//! The caller (create_gift handler) invokes this subagent as a background
//! task after the gift is created.

use std::sync::Arc;

use orchest::model::ModelAdapter;
use orchest::run::AgentConfig;
use orchest::tool::agent_as_tool::ContextMode;
use orchest::tool::registry::ToolRegistry;
use orchest::tool::ToolError;
use serde_json::Value;

use crate::gift::GiftStore;
use crate::prompts::COUNTDOWN_TEMPLATE;

use chrono::Local;

/// Build a countdown subagent as an Orchest tool.
///
/// The subagent receives `{name, birthday, scenario, lyric_snippet}` as
/// input, generates the birthday countdown HTML block, and returns it.
pub fn countdown_tool(
    model: Arc<dyn ModelAdapter>,
) -> Result<Arc<dyn orchest::tool::Tool>, Box<dyn std::error::Error + Send + Sync>> {
    let config = AgentConfig::builder("music-gift/countdown")
        .system_prompt(COUNTDOWN_TEMPLATE.as_str())
        .max_steps(1)
        .build()
        .map_err(|e| format!("building countdown config: {e}"))?;

    let tool = config
        .as_tool(
            "generate_countdown",
            "Generate a birthday countdown HTML block with CSS animations and a live timer.",
        )
        .model(model)
        .registry(ToolRegistry::new())
        .context_mode(ContextMode::Fresh)
        .input_mapper(|input: Value| {
            let name = input.get("name").and_then(Value::as_str).unwrap_or("Someone");
            let birthday = input.get("birthday").and_then(Value::as_str).unwrap_or("");
            let scenario = input.get("scenario").and_then(Value::as_str).unwrap_or("");
            let lyric_snippet = input.get("lyric_snippet").and_then(Value::as_str).unwrap_or("");

            let (month, day, days_until, target_date) =
                parse_birthday_info(birthday).unwrap_or_else(|| {
                    ("Jan".into(), "1".into(), 0, "2025-01-01".into())
                });

            let prompt = build_countdown_prompt(
                name, scenario, &month, &day, days_until, &target_date, lyric_snippet, "",
            );

            Ok(prompt)
        })
        .output_extractor(|output: Value| {
            let html = output
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            serde_json::json!({ "html": strip_code_fences(&html) })
        })
        .build()
        .map_err(|e| format!("building countdown tool: {e}"))?;

    Ok(tool)
}

/// Execute the countdown subagent and save the result to disk.
pub async fn run_countdown(
    model: Arc<dyn ModelAdapter>,
    store: &GiftStore,
    data_dir: &std::path::Path,
    gift_id: &str,
    name: &str,
    birthday: &str,
    scenario: &str,
    lyric_snippet: &str,
) {
    let tool = match countdown_tool(model) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("countdown: failed to build tool: {e}");
            let _ = store.update_countdown_status(gift_id, "failed");
            return;
        }
    };

    let ctx = tool_context();
    let input = serde_json::json!({
        "name": name,
        "birthday": birthday,
        "scenario": scenario,
        "lyric_snippet": lyric_snippet,
    });

    let result = match tool.execute(input, &ctx).await {
        Ok(output) => output,
        Err(e) => {
            eprintln!("countdown: subagent error: {e}");
            let _ = store.update_countdown_status(gift_id, "failed");
            return;
        }
    };

    let html = match result {
        orchest::tool::ToolOutput::Immediate(value) => {
            value.get("html").and_then(Value::as_str).unwrap_or("").to_string()
        }
        _ => {
            eprintln!("countdown: unexpected async output");
            String::new()
        }
    };

    if html.is_empty() {
        eprintln!("countdown: empty HTML output");
        let _ = store.update_countdown_status(gift_id, "failed");
        return;
    }

    let dir = data_dir.join("countdown");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("countdown: failed to create dir {}: {e}", dir.display());
        let _ = store.update_countdown_status(gift_id, "failed");
        return;
    }

    let path = dir.join(format!("{gift_id}.html"));
    if let Err(e) = std::fs::write(&path, &html) {
        eprintln!("countdown: failed to write {}: {e}", path.display());
        let _ = store.update_countdown_status(gift_id, "failed");
        return;
    }

    let _ = store.update_countdown_status(gift_id, "ready");
}

fn tool_context() -> orchest::tool::ToolContext {
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

// ── Helpers ────────────────────────────────────────────────────────────────

fn build_countdown_prompt(
    name: &str, scenario: &str, month: &str, day: &str,
    days_until: i64, target_date: &str, lyric_snippet: &str, previous_error: &str,
) -> String {
    let retry_note = if previous_error.is_empty() {
        String::new()
    } else {
        format!("\n\n【IMPORTANT】Previous generation failed: {previous_error}. Fix this attempt.")
    };

    let template = COUNTDOWN_TEMPLATE.as_str();
    if template.is_empty() {
        return format!(
            "Generate a birthday countdown HTML page for {name} whose birthday is {month} {day} ({days_until} days away). Target date: {target_date}. Scene: {scenario}."
        );
    }

    template
        .replace("{name}", name)
        .replace("{scenario}", scenario)
        .replace("{month}", month)
        .replace("{day}", day)
        .replace("{days_until}", &days_until.to_string())
        .replace("{target_date}", target_date)
        .replace("{lyric_snippet}", lyric_snippet)
        .replace("{previous_error}", &retry_note)
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
    let months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
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

fn strip_code_fences(html: &str) -> String {
    let trimmed = html.trim();
    let without_open = trimmed.strip_prefix("```html")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_close = without_open.strip_suffix("```").unwrap_or(without_open);
    without_close.trim().to_string()
}
