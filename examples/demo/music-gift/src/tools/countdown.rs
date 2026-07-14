use std::sync::Arc;

use orchest_protocol::{ChatModel, ContentBlock, Message, RequestOptions, Role};

use crate::gift::GiftStore;
use crate::prompts::COUNTDOWN_TEMPLATE;

#[allow(clippy::too_many_arguments)]
/// Build a countdown prompt from the template.
pub fn build_countdown_prompt(
    name: &str,
    scenario: &str,
    month: &str,
    day: &str,
    days_until: i64,
    target_date: &str,
    lyric_snippet: &str,
    previous_error: &str,
) -> String {
    let retry_note = if previous_error.is_empty() {
        String::new()
    } else {
        format!("\n\n【IMPORTANT】Previous generation failed: {previous_error}. Please ensure this is fixed in this attempt.")
    };

    let template = COUNTDOWN_TEMPLATE.as_str();
    if template.is_empty() {
        // Fallback inline template if prompt file is missing
        format!(
            r#"You are a creative frontend engineer. Generate an embeddable birthday countdown HTML block for the person below. It will be inserted at the top of an existing page, with a music player and lyrics below.

【Person Info】
Name/nickname: {name}
Birthday: {month} {day} ({days_until} days away)
Countdown target date (MUST use this exact value): {target_date}
Personal scene: {scenario}

【Lyrics snippet (extract key imagery to strongly tie this block to their story)】
{lyric_snippet}

【Output format (strict)】
Output must be a self-contained HTML block with this structure:

<style>
  /* All styles here, all selectors must start with #birthday-section for namespace isolation */
  /* @import url() allowed for Google Fonts, no other external resources */
</style>

<div id="birthday-section">
  <!-- All content here -->
</div>

<script>
  (function() {{
    /* All JS in IIFE to avoid global pollution */
    /* Use document.getElementById / querySelector to access elements in the div above */
    /* Don't use DOMContentLoaded — execute directly (elements are already in DOM) */
  }})();
</script>

Forbidden: <!DOCTYPE>, <html>, <head>, <body> tags.
Output only the three blocks above, no explanatory text.

【Design requirements】
1. Background: match the outer page #f7f3ec or pick a warm coordinating tone; padding 24-32px
2. 1-2 accent colors drawn from the person's story; warm hand-drawn illustration style
3. 2-3 simple SVG line illustrations (viewBox="0 0 100 100"):
   - Stroke only, no fill; stroke-linecap:round; stroke-linejoin:round; stroke-width 2-3px
   - Must relate to the person's specific story — no generic cakes or balloons
4. Live countdown to the second via setInterval, showing days/hours/minutes/seconds in large type
   【CRITICAL】Countdown target must use the date string "{target_date}" exactly: new Date('{target_date}'). Do not calculate the year yourself.
5. 1-2 delightful interactions (must use addEventListener), themed to the story:
   - walking scene → click ground to leave footprints; music scene → click note to make it bounce; etc.
6. The person's name + one personal line drawn from the lyrics/scene (max 30 chars, avoid clichés like "wishing you...")
7. Bottom padding = 0 — music player card sits directly below, seamless join{retry_note}"#
        )
    } else {
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
}

/// Parse a birthday string like "YYYY-MM-DD" or "MM-DD" into month, day,
/// days until next occurrence, and ISO target date string.
pub fn parse_birthday_info(birthday: &str) -> Option<(String, String, i64, String)> {
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

    // Calculate days until next birthday
    let now = chrono::Local::now().date_naive();
    let current_year = now.format("%Y").to_string();
    let target_str = format!("{current_year}-{month_num:02}-{day_num:02}");
    let target = chrono::NaiveDate::parse_from_str(&target_str, "%Y-%m-%d").ok()?;
    let mut days_until = (target - now).num_days();
    if days_until < 0 {
        // Birthday has passed this year, target next year
        days_until += 365;
    }

    Some((
        month_name.to_string(),
        day_str.to_string(),
        days_until,
        target_str,
    ))
}

pub fn strip_code_fences(html: &str) -> String {
    let trimmed = html.trim();
    let without_open = trimmed
        .strip_prefix("```html")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_close = without_open.strip_suffix("```").unwrap_or(without_open);
    without_close.trim().to_string()
}

/// Generate birthday countdown HTML via the LLM and save it to disk.
#[allow(clippy::too_many_arguments)]
pub async fn generate_countdown(
    model: &Arc<dyn ChatModel>,
    store: &GiftStore,
    data_dir: &std::path::Path,
    gift_id: &str,
    name: &str,
    birthday: &str,
    scenario: &str,
    lyric_snippet: &str,
) {
    let (month, day, days_until, target_date) = match parse_birthday_info(birthday) {
        Some(v) => v,
        None => {
            let _ = store.update_countdown_status(gift_id, "failed");
            return;
        }
    };

    let prompt = build_countdown_prompt(
        name, scenario, &month, &day, days_until, &target_date, lyric_snippet, "",
    );

    let messages = [Message {
        role: Role::User,
        content: vec![ContentBlock::Text(prompt)],
    }];
    let options = RequestOptions {
        max_tokens: Some(8192),
        ..Default::default()
    };

    let result = model.complete(&messages, &[], &options, None).await;
    match result {
        Ok(response) => {
            let html = response
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");

            let html = strip_code_fences(&html);

            let dir = data_dir.join("countdown");
            if let Err(e) = tokio::fs::create_dir_all(&dir).await {
                eprintln!("countdown: failed to create dir {}: {e}", dir.display());
                let _ = store.update_countdown_status(gift_id, "failed");
                return;
            }

            let path = dir.join(format!("{gift_id}.html"));
            if let Err(e) = tokio::fs::write(&path, &html).await {
                eprintln!("countdown: failed to write {}: {e}", path.display());
                let _ = store.update_countdown_status(gift_id, "failed");
                return;
            }

            let _ = store.update_countdown_status(gift_id, "ready");
        }
        Err(e) => {
            eprintln!("countdown: LLM error for gift {gift_id}: {e}");
            let _ = store.update_countdown_status(gift_id, "failed");
        }
    }
}
