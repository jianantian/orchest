//! System prompt, lyrics parsing, and chat agent loop.

use std::path::Path;
use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::run::{AgentConfig, AgentRun, RunInput};
use orchest::tool::builtin::ReadFileTool;
use orchest::tool::registry::ToolRegistry;
use orchest_protocol::{ChatModel, ContentBlock, Message, StreamEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::gift::GiftMeta;
use crate::tools::collect_info;

/// Parsed lyrics result extracted from the LLM's full response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedLyrics {
    pub has_lyrics: bool,
    pub lyrics: String,
    pub style: String,
    pub title: String,
    pub vocal: String,
}

/// Parse the LLM's full text response for structured lyrics tags.
pub fn parse_lyrics(full_text: &str) -> ParsedLyrics {
    let lyrics = extract_lyrics(full_text)
        .unwrap_or_default()
        .trim()
        .to_string();
    // A lyric block that parsed to nothing is not a lyric, whatever tags the
    // model emitted. Reporting has_lyrics=true with empty lyrics let an empty
    // string flow all the way to the music provider.
    let has_lyrics = !lyrics.is_empty();
    let style = extract_between(full_text, "<<<STYLE>>>", "<<<STYLE_END>>>")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| GiftMeta::DEFAULT_STYLE.to_string());
    let title = extract_between(full_text, "<<<TITLE>>>", "<<<TITLE_END>>>")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    // The model rarely emits a bare "male"/"female": it writes descriptive
    // vocals like "male, warm" or "男声, 温柔". A strict equality check dropped
    // all of those and silently defaulted every song to a female vocal. Detect
    // the gender by substring, testing "female"/"女" first since "female"
    // contains "male".
    let vocal = extract_between(full_text, "<<<VOCAL>>>", "<<<VOCAL_END>>>")
        .map(|s| {
            let low = s.to_lowercase();
            if low.contains("female") || s.contains('女') {
                "female"
            } else if low.contains("male") || s.contains('男') {
                "male"
            } else {
                GiftMeta::DEFAULT_VOCAL
            }
            .to_string()
        })
        .unwrap_or_else(|| GiftMeta::DEFAULT_VOCAL.to_string());
    ParsedLyrics {
        has_lyrics,
        lyrics,
        style,
        title,
        vocal,
    }
}

/// Extract the pure lyric text, independent of tag ordering.
///
/// The two prompts disagree on layout: the generation prompt (`lyrics.md`) puts
/// the `<<<STYLE>>>`/`<<<TITLE>>>`/`<<<VOCAL>>>` blocks *inside* the lyric block,
/// before the lyric text; the review prompt (`review.md`) puts them after
/// `<<<END>>>`. The review pass normally reformats to the latter, but when it
/// fails we parse the raw generation output instead — so the extractor must
/// tolerate metadata tags appearing before the lyrics and strip them out,
/// rather than shipping `<<<STYLE>>>…` to the music provider as "lyrics".
fn extract_lyrics(text: &str) -> Option<String> {
    const OPEN: &str = "<<<LYRICS>>>";
    let start = text.find(OPEN)? + OPEN.len();
    let rest = &text[start..];
    // Prefer the explicit terminator; fall back to the review-summary header,
    // then to end of text. (The model sometimes drops <<<END>>>.)
    let end = rest
        .find("<<<END>>>")
        .or_else(|| rest.find("## Review"))
        .unwrap_or(rest.len());
    Some(strip_meta_tags(&rest[..end]))
}

/// Remove any `<<<STYLE>>>…`, `<<<TITLE>>>…`, `<<<VOCAL>>>…` blocks (and stray
/// lone markers) from a lyric fragment, leaving only the sung text.
fn strip_meta_tags(s: &str) -> String {
    let mut out = s.to_string();
    for (open, close) in [
        ("<<<STYLE>>>", "<<<STYLE_END>>>"),
        ("<<<TITLE>>>", "<<<TITLE_END>>>"),
        ("<<<VOCAL>>>", "<<<VOCAL_END>>>"),
    ] {
        while let (Some(a), Some(b)) = (out.find(open), out.find(close)) {
            if b >= a {
                out.replace_range(a..b + close.len(), "");
            } else {
                break;
            }
        }
    }
    // Drop any orphaned single markers (e.g. a lone <<<READY>>> or <<<STYLE>>>).
    while let Some(a) = out.find("<<<") {
        match out[a..].find(">>>") {
            Some(rel) => out.replace_range(a..a + rel + 3, ""),
            None => break,
        }
    }
    out.trim().to_string()
}

fn extract_between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}

/// Extract the "## Review Pass" table from the reviewed output (if present).
///
/// The reviewer appends a review summary after the `<<<END>>>` tag.
pub fn extract_review_summary(reviewed: &str) -> Option<String> {
    if let Some(idx) = reviewed.find("## Review Pass") {
        // Blank lines are skipped, not treated as terminators: the reviewer
        // separates the header from the table with one, and stopping there
        // truncated the summary to just the "## Review Pass" line — the
        // frontend's fix-count badge never saw a 🔧.
        let summary = reviewed[idx..]
            .lines()
            .take_while(|l| {
                let t = l.trim();
                t.is_empty()
                    || t.starts_with('|')
                    || t.starts_with('#')
                    || t.starts_with("Verdict")
                    || t.starts_with('-')
            })
            .collect::<Vec<_>>()
            .join("\n");
        let summary = summary.trim();
        if summary.is_empty() {
            None
        } else {
            Some(summary.to_string())
        }
    } else {
        None
    }
}

pub mod message;
pub use message::*;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    Delta {
        text: String,
    },
    /// Emitted when the review pass starts (after the chat stream ends, before
    /// `Done`). The review is a second full LLM call taking tens of seconds;
    /// without an event the client sits silent with a disabled input.
    Reviewing,
    Done {
        has_lyrics: bool,
        lyrics: String,
        style: String,
        title: String,
        vocal: String,
        /// Review report from the second-pass review agent (may be empty if review skipped).
        review: Option<String>,
        /// Pipeline stages that fell back during this turn (e.g. ["review"]
        /// when the review pass failed and the lyrics are unreviewed). Empty
        /// when everything ran. The frontend shows a hint for non-empty.
        degraded: Vec<String>,
    },
    Error {
        error: String,
    },
}
/// Start a chat agent run using Orchest's AgentRun and stream events via tx.
/// Returns the full text output.
pub async fn run_chat_agent(
    model: Arc<dyn ChatModel>,
    messages: Vec<Message>,
    tx: mpsc::Sender<SseEvent>,
    skills_dir: Option<&str>,
) -> AppResult<String> {
    let mut builder = AgentConfig::builder("music-gift/chat").max_steps(5);
    if let Some(dir) = skills_dir {
        builder = builder.skills_dir(dir);
    }
    let config = builder
        .build()
        .map_err(|e| AppError::Llm(format!("building agent config: {e}")))?;

    let mut tool_registry = ToolRegistry::new();
    tool_registry
        .register(collect_info::create_tool())
        .map_err(|e| AppError::Llm(format!("registering collect_info: {e}")))?;

    // Register ReadFileTool so agent can load skill content on demand.
    let read_file = Arc::new(ReadFileTool::new());
    if let Some(dir) = skills_dir {
        let skill_path = Path::new(dir).join("lyrics-writer").join("SKILL.md");
        if skill_path.exists() {
            read_file
                .register_skill("lyrics-writer".to_string(), skill_path)
                .await;
        }
    }
    tool_registry
        .register(read_file)
        .map_err(|e| AppError::Llm(format!("registering read_file: {e}")))?;

    let blocks: Vec<ContentBlock> = messages.iter().flat_map(|m| m.content.clone()).collect();
    let input = RunInput::from_blocks(blocks).map_err(|e| AppError::Llm(e.to_string()))?;
    let (handle, mut rx) = AgentRun::start(
        config,
        input,
        model as Arc<dyn orchest::model::ModelAdapter>,
        tool_registry,
    );
    let mut full_text = String::new();

    while let Some(event) = rx.recv().await {
        #[allow(clippy::collapsible_match)]
        match event {
            RuntimeEvent::ModelStreamChunk { delta } => {
                if let StreamEvent::Text { delta: text } = delta {
                    full_text.push_str(&text);
                    let _ = tx.send(SseEvent::Delta { text: text.clone() }).await;
                }
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                if full_text.is_empty() {
                    if let Some(text) = output.as_str() {
                        full_text = text.to_string();
                    }
                }
            }
            RuntimeEvent::RunFailed { error } => {
                let _ = tx.send(SseEvent::Error { error }).await;
                return Err(AppError::Llm("agent run failed".to_string()));
            }
            other => {
                let name = match &other {
                    RuntimeEvent::ToolCallStarted { .. } => "ToolCallStarted",
                    RuntimeEvent::ToolCallUpdate { .. } => "ToolCallUpdate",
                    RuntimeEvent::ToolCallCompleted { .. } => "ToolCallCompleted",
                    RuntimeEvent::ToolCallFailed { .. } => "ToolCallFailed",
                    RuntimeEvent::SkillContentRead { .. } => "SkillContentRead",
                    _ => "other",
                };
                eprintln!("[music-gift] agent event: {name}");
            }
        }
    }

    handle.wait().await;
    Ok(full_text)
}

/// Review system prompt compiled into the binary.
static REVIEW_PROMPT: &str = include_str!("../prompts/review.md");

/// Outcome of the review pass: the (possibly unreviewed) text plus a
/// degradation flag the caller surfaces to the client.
pub struct ReviewOutcome {
    pub text: String,
    /// True when the review did not run to completion and `text` is the
    /// original chat output.
    pub degraded: bool,
}

/// Run a second-pass review agent on the raw chat output.
///
/// The reviewer checks pronunciation, performance cues, structure,
/// and content issues using a 10-point checklist derived from
/// bitwize-music's lyric-reviewer skill (CC0).
///
/// Returns corrected output in the same tag format. Falls back to
/// the original on error — every fallback is logged and flagged degraded
/// (previously the failures were silent or eprintln-only).
pub async fn run_review_pass(model: Arc<dyn ChatModel>, raw_output: &str) -> ReviewOutcome {
    let config = match AgentConfig::builder("music-gift/review")
        .max_steps(1)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(stage = "review", error = %e, "review: building config failed; using unreviewed output");
            return ReviewOutcome {
                text: raw_output.to_string(),
                degraded: true,
            };
        }
    };
    let messages = vec![
        Message {
            role: orchest_protocol::Role::System,
            content: vec![ContentBlock::Text(REVIEW_PROMPT.to_string())],
        },
        Message {
            role: orchest_protocol::Role::User,
            content: vec![ContentBlock::Text(raw_output.to_string())],
        },
    ];

    let blocks: Vec<ContentBlock> = messages.into_iter().flat_map(|m| m.content).collect();
    let input = match RunInput::from_blocks(blocks) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!(stage = "review", error = %e, "review: building input failed; using unreviewed output");
            return ReviewOutcome {
                text: raw_output.to_string(),
                degraded: true,
            };
        }
    };

    let tool_registry = ToolRegistry::new(); // Review agent uses no tools.
    let (handle, mut rx) = AgentRun::start(config, input, model, tool_registry);

    let mut reviewed = String::new();
    let mut run_failed = false;
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk {
                delta: StreamEvent::Text { delta: text },
            } => {
                reviewed.push_str(&text);
            }
            RuntimeEvent::RunCompleted { output, .. } => {
                if reviewed.is_empty() {
                    if let Some(text) = output.as_str() {
                        reviewed = text.to_string();
                    }
                }
            }
            RuntimeEvent::RunFailed { error } => {
                tracing::warn!(stage = "review", error = %error, "review: agent run failed; using unreviewed output");
                run_failed = true;
            }
            _ => {}
        }
    }
    handle.wait().await;

    if reviewed.is_empty() {
        if !run_failed {
            tracing::warn!(
                stage = "review",
                "review: empty output; using unreviewed output"
            );
        }
        ReviewOutcome {
            text: raw_output.to_string(),
            degraded: true,
        }
    } else {
        tracing::debug!(chars = reviewed.len(), "review: done");
        ReviewOutcome {
            text: reviewed,
            degraded: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchest_protocol::{MediaSource, Role};

    #[test]
    fn parse_lyrics_extracts_all_tags() {
        let text = "<<<LYRICS>>>\n[verse 1]\nHello world\n[chorus]\nSing loud\n<<<END>>>
<<<STYLE>>>warm and gentle<<<STYLE_END>>>
<<<TITLE>>>Hello<<<TITLE_END>>>
<<<VOCAL>>>female<<<VOCAL_END>>>";
        let result = parse_lyrics(text);
        assert!(result.has_lyrics);
        assert!(result.lyrics.contains("[verse 1]"));
        assert_eq!(result.style, "warm and gentle");
        assert_eq!(result.title, "Hello");
        assert_eq!(result.vocal, "female");
    }

    #[test]
    fn parse_lyrics_uses_defaults_when_tags_missing() {
        let text = "Just some text without any tags";
        let result = parse_lyrics(text);
        assert!(!result.has_lyrics);
        assert_eq!(result.style, GiftMeta::DEFAULT_STYLE);
        assert_eq!(result.title, "");
        assert_eq!(result.vocal, "female");
    }

    /// The review pass omits `<<<END>>>` (its prompt template never showed one).
    /// That used to yield has_lyrics=true with empty lyrics, and the empty
    /// string reached the music provider, which invented an English song.
    #[test]
    fn parse_lyrics_recovers_when_end_marker_missing() {
        let text = "<<<LYRICS>>>\n[verse 1 — tender]\n那天风很轻\n<<<STYLE>>>warm<<<STYLE_END>>>
<<<TITLE>>>第一次骑车<<<TITLE_END>>>
<<<VOCAL>>>male<<<VOCAL_END>>>";
        let result = parse_lyrics(text);
        assert!(result.has_lyrics);
        assert!(result.lyrics.contains("那天风很轻"));
        assert!(!result.lyrics.contains("<<<STYLE>>>"));
        assert_eq!(result.style, "warm");
        assert_eq!(result.title, "第一次骑车");
        assert_eq!(result.vocal, "male");
    }

    /// When the review pass fails, the raw generation output is parsed instead,
    /// and it puts STYLE/TITLE/VOCAL *inside* the lyric block, before the text.
    /// Those tags must never end up in the lyrics sent to the music provider.
    #[test]
    fn parse_lyrics_strips_metadata_tags_from_raw_generation_format() {
        let raw = "sure, here you go\n<<<LYRICS>>>\n\
<<<STYLE>>>warm folk, guitar<<<STYLE_END>>>\n\
<<<TITLE>>>Wheels at Dusk<<<TITLE_END>>>\n\
<<<VOCAL>>>male, warm<<<VOCAL_END>>>\n\
[verse 1 — gentle]\n你摔了又站起来\n[chorus — soaring]\n骑吧 阿杰\n<<<END>>>";
        let result = parse_lyrics(raw);
        assert!(result.has_lyrics);
        assert!(
            !result.lyrics.contains("<<<"),
            "lyrics still polluted: {:?}",
            result.lyrics
        );
        assert!(result.lyrics.starts_with("[verse 1 — gentle]"));
        assert!(result.lyrics.contains("骑吧 阿杰"));
        // Tags are still extracted for their own fields.
        assert_eq!(result.style, "warm folk, guitar");
        assert_eq!(result.title, "Wheels at Dusk");
        assert_eq!(result.vocal, "male");
    }

    /// Raw format AND a dropped `<<<END>>>` — the worst case, and the one my
    /// first fix regressed on (find-first-`<<<` truncated to empty).
    #[test]
    fn parse_lyrics_handles_raw_format_without_end_marker() {
        let raw = "<<<LYRICS>>>\n\
<<<STYLE>>>warm<<<STYLE_END>>>\n\
<<<TITLE>>>T<<<TITLE_END>>>\n\
<<<VOCAL>>>female<<<VOCAL_END>>>\n\
[verse 1 — soft]\n第一句\n第二句";
        let result = parse_lyrics(raw);
        assert!(result.has_lyrics);
        assert!(!result.lyrics.contains("<<<"));
        assert!(result.lyrics.contains("第一句"));
    }

    /// An empty lyric block is not a lyric, whatever tags surround it.
    #[test]
    fn parse_lyrics_reports_no_lyrics_when_block_is_empty() {
        let text = "<<<LYRICS>>>\n\n<<<END>>><<<STYLE>>>warm<<<STYLE_END>>>";
        let result = parse_lyrics(text);
        assert!(!result.has_lyrics);
        assert_eq!(result.lyrics, "");
    }

    #[test]
    fn parse_lyrics_validates_vocal_gender() {
        let text = "<<<LYRICS>>>\ntest\n<<<END>>><<<VOCAL>>>invalid<<<VOCAL_END>>>";
        let result = parse_lyrics(text);
        assert_eq!(result.vocal, "female");

        let text2 = "<<<LYRICS>>>\ntest\n<<<END>>><<<VOCAL>>>male<<<VOCAL_END>>>";
        let result2 = parse_lyrics(text2);
        assert_eq!(result2.vocal, "male");
    }

    /// Descriptive vocals ("male, warm", "男声…") must map to the right gender,
    /// not silently fall back to female.
    #[test]
    fn parse_lyrics_detects_descriptive_vocal_gender() {
        let cases = [
            ("<<<VOCAL>>>male, warm, breathy<<<VOCAL_END>>>", "male"),
            ("<<<VOCAL>>>female, soft legato<<<VOCAL_END>>>", "female"),
            ("<<<VOCAL>>>男声, 温柔, 略带沙哑<<<VOCAL_END>>>", "male"),
            ("<<<VOCAL>>>女声, 空灵<<<VOCAL_END>>>", "female"),
        ];
        for (tag, want) in cases {
            let text = format!("<<<LYRICS>>>\nla la\n<<<END>>>{tag}");
            assert_eq!(parse_lyrics(&text).vocal, want, "for {tag}");
        }
    }

    /// The reviewer puts a blank line between the "## Review Pass" header and
    /// the table. Stopping at that blank line truncated the summary to ~14
    /// chars, and the frontend's fix-count badge never saw a 🔧.
    #[test]
    fn extract_review_summary_keeps_table_past_blank_lines() {
        let reviewed = "<<<LYRICS>>>\nla la\n<<<END>>>\n---\n## Review Pass\n\n| # | Check | Status | Detail |\n| 1 | Pronunciation | 🔧 | 2 fixed |\n| 2 | Cues | ✅ | |\n\nVerdict: READY (2 auto-fixed)\n";
        let s = extract_review_summary(reviewed).expect("summary present");
        assert!(s.contains("## Review Pass"));
        assert!(s.contains("Pronunciation"));
        assert!(s.contains('🔧'));
        assert!(s.contains("Verdict: READY"));
        assert!(!s.contains("<<<END>>>"));
    }

    #[test]
    fn extract_review_summary_none_without_header() {
        assert!(extract_review_summary("no review here").is_none());
    }

    #[test]
    fn build_messages_injects_photo_blocks_into_first_user_message() {
        let system = Message {
            role: Role::System,
            content: vec![ContentBlock::Text("system prompt".into())],
        };
        let incoming = vec![IncomingMessage {
            role: "user".into(),
            content: "hello".into(),
        }];
        let photos = vec![ContentBlock::Image {
            source: MediaSource::Base64 {
                media_type: "image/jpeg".into(),
                data: "fake".into(),
            },
            detail: None,
        }];
        let messages = build_messages(system, &incoming, &photos);
        assert_eq!(messages.len(), 2);
        let user_content = &messages[1].content;
        assert_eq!(user_content.len(), 2);
    }

    #[test]
    fn build_system_message_includes_meta() {
        let meta = serde_json::json!({"name": "Alice"});
        let msg = build_system_message(&meta, 0);
        let content = match &msg.content[0] {
            ContentBlock::Text(s) => s.clone(),
            _ => String::new(),
        };
        assert!(content.contains("Alice"));
        assert!(content.contains("Known info"));
    }
}
