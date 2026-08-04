//! Prompt templates for the music-gift demo.
//!
//! All prompts — `SYSTEM_PROMPT`, `STUDIO_SYSTEM_PROMPT`, `COUNTDOWN_TEMPLATE`,
//! and the per-provider music prompt skills — are embedded at compile time via
//! `include_str!`, so the binary never depends on the runtime working directory.

use std::collections::HashMap;
use std::sync::LazyLock;

/// System prompt for the chat assistant.
pub static SYSTEM_PROMPT: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/system.md").to_string());

/// System prompt for studio mode: collaborative co-editing of an existing
/// draft. The model replies conversationally and emits marker blocks only
/// for the fields it changed — no elevate/review pipeline, no guided
/// protocol markers.
pub static STUDIO_SYSTEM_PROMPT: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/studio.md").to_string());

/// Template for the birthday countdown HTML generator.
///
/// Placeholders substituted at runtime by `tools::countdown::build_countdown_prompt`:
/// `{name}`, `{scenario}`, `{month}`, `{day}`, `{days_until}`,
/// `{target_date}`, `{lyric_snippet}`, `{previous_error}`.
pub static COUNTDOWN_TEMPLATE: LazyLock<String> =
    LazyLock::new(|| include_str!("../prompts/countdown.md").to_string());

/// Per-provider music prompt generation skills, embedded at compile time
/// (like `prompts/review.md` in agent.rs). Reading them from disk at runtime
/// made the set depend on the process CWD, silently dropped missing files,
/// and then mis-reported the gap as "unknown music provider".
pub static MUSIC_PROMPT_SKILLS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    HashMap::from([
        (
            "suno".to_string(),
            include_str!("../prompts/music_prompt/suno.md").to_string(),
        ),
        (
            "mureka".to_string(),
            include_str!("../prompts/music_prompt/mureka.md").to_string(),
        ),
        (
            "minimax".to_string(),
            include_str!("../prompts/music_prompt/minimax.md").to_string(),
        ),
    ])
});
