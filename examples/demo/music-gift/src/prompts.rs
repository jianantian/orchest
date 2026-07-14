//! Prompt templates loaded at compile time from `prompts/*.md` files.
//!
//! Each `LazyLock<String>` embeds the file content via `include_str!` so
//! no runtime I/O is needed.

use std::sync::LazyLock;

/// System prompt for the chat assistant.
pub static SYSTEM_PROMPT: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/system.md").to_string());

#[allow(dead_code)]
/// Lyrics generation skill prompt.
pub static LYRICS_SKILL: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/lyrics.md").to_string());

/// Template for the birthday countdown HTML generator.
///
/// Placeholders substituted at runtime by `tools::countdown::build_countdown_prompt`:
/// `{name}`, `{scenario}`, `{month}`, `{day}`, `{days_until}`,
/// `{target_date}`, `{lyric_snippet}`, `{previous_error}`.
pub static COUNTDOWN_TEMPLATE: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/countdown.md").to_string());
