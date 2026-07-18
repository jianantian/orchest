//! Prompt templates for the music-gift demo.
//!
//! Static prompts (`SYSTEM_PROMPT`, `COUNTDOWN_TEMPLATE`) are embedded at
//! compile time via `include_str!`. Music prompt skills are loaded from disk
//! at startup so providers can be added without recompilation.

use std::collections::HashMap;
use std::sync::LazyLock;

/// System prompt for the chat assistant.
pub static SYSTEM_PROMPT: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/system.md").to_string());

/// Template for the birthday countdown HTML generator.
///
/// Placeholders substituted at runtime by `tools::countdown::build_countdown_prompt`:
/// `{name}`, `{scenario}`, `{month}`, `{day}`, `{days_until}`,
/// `{target_date}`, `{lyric_snippet}`, `{previous_error}`.
pub static COUNTDOWN_TEMPLATE: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/countdown.md").to_string());
/// Per-provider music prompt generation skills loaded at startup.
pub static MUSIC_PROMPT_SKILLS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    for provider in &["suno", "mureka", "minimax"] {
        if let Ok(content) =
            std::fs::read_to_string(format!("prompts/music_prompt/{}.md", provider))
        {
            eprintln!("[music-gift] loaded music prompt skill: {provider}");
            m.insert(provider.to_string(), content);
        } else {
            eprintln!("[music-gift] music prompt skill not found: {provider}");
        }
    }
    m
});
