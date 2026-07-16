//! System prompt, lyrics parsing, and chat agent loop.

use std::sync::Arc;

use orchest::events::RuntimeEvent;
use orchest::run::{AgentConfig, AgentRun, RunInput};
use orchest::tool::registry::ToolRegistry;
use orchest_protocol::{ChatModel, ContentBlock, Message, StreamEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};
use crate::tools::collect_info;

/// Default style if the LLM didn't emit one.
const DEFAULT_STYLE: &str = "healing and warm";

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
    let has_lyrics = full_text.contains("<<<LYRICS>>>");
    let lyrics = extract_between(full_text, "<<<LYRICS>>>", "<<<END>>>")
        .unwrap_or_default()
        .trim()
        .to_string();
    let style = extract_between(full_text, "<<<STYLE>>>", "<<<STYLE_END>>>")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_STYLE.to_string());
    let title = extract_between(full_text, "<<<TITLE>>>", "<<<TITLE_END>>>")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let vocal = extract_between(full_text, "<<<VOCAL>>>", "<<<VOCAL_END>>>")
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s == "female" || s == "male")
        .unwrap_or_else(|| "female".to_string());
    ParsedLyrics {
        has_lyrics,
        lyrics,
        style,
        title,
        vocal,
    }
}

fn extract_between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}


pub mod message;
pub use message::*;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    Delta { text: String },
    Done {
        has_lyrics: bool,
        lyrics: String,
        style: String,
        title: String,
        vocal: String,
    },
    Error { error: String },
}

/// Start a chat agent run using Orchest's AgentRun and stream events via tx.
/// Returns the full text output.
pub async fn run_chat_agent(
    model: Arc<dyn ChatModel>,
    messages: Vec<Message>,
    tx: mpsc::Sender<SseEvent>,
) -> AppResult<String> {
    let config = AgentConfig::builder("music-gift/chat")
        .max_steps(5)
        .build()
        .map_err(|e| AppError::Llm(format!("building agent config: {e}")))?;

    let mut tool_registry = ToolRegistry::new();
    tool_registry
        .register(collect_info::create_tool())
        .map_err(|e| AppError::Llm(format!("registering collect_info: {e}")))?;

    let blocks: Vec<ContentBlock> = messages.iter().flat_map(|m| m.content.clone()).collect();

    let input = RunInput::from_blocks(blocks)
        .map_err(|e| AppError::Llm(e.to_string()))?;
    let (handle, mut rx) =
        AgentRun::start(config, input, model as Arc<dyn orchest::model::ModelAdapter>, tool_registry);

    let mut full_text = String::new();

    while let Some(event) = rx.recv().await {
        #[allow(clippy::collapsible_match)]
        match event {
            RuntimeEvent::ModelStreamChunk { delta } => {
                if let StreamEvent::Text { delta: text } = delta {
                    full_text.push_str(&text);
                    let _ = tx
                        .send(SseEvent::Delta {
                            text: text.clone(),
                        })
                        .await;
                }
            }
            RuntimeEvent::RunCompleted { output } => {
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
            _ => {}
        }
    }

    handle.wait().await;
    Ok(full_text)
}

/// Review system prompt compiled into the binary.
static REVIEW_PROMPT: &str = include_str!("../prompts/review.md");

/// Run a second-pass review agent on the raw chat output.
///
/// The reviewer checks pronunciation, performance cues, structure,
/// and content issues using a 10-point checklist derived from
/// bitwize-music's lyric-reviewer skill (CC0).
///
/// Returns corrected output in the same tag format. Falls back to
/// the original on error.
pub async fn run_review_pass(
    model: Arc<dyn ChatModel>,
    raw_output: &str,
) -> String {
    let config = match AgentConfig::builder("music-gift/review")
        .max_steps(1)
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[music-gift] review: building config: {e}");
            return raw_output.to_string();
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
            eprintln!("[music-gift] review: building input: {e}");
            return raw_output.to_string();
        }
    };

    let tool_registry = ToolRegistry::new(); // Review agent uses no tools.
    let (handle, mut rx) = AgentRun::start(config, input, model, tool_registry);

    let mut reviewed = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk { delta } => {
                if let StreamEvent::Text { delta: text } = delta {
                    reviewed.push_str(&text);
                }
            }
            RuntimeEvent::RunCompleted { output } => {
                if reviewed.is_empty() {
                    if let Some(text) = output.as_str() {
                        reviewed = text.to_string();
                    }
                }
            }
            RuntimeEvent::RunFailed { error } => {
                eprintln!("[music-gift] review: agent failed: {error}");
            }
            _ => {}
        }
    }
    handle.wait().await;

    if reviewed.is_empty() {
        eprintln!("[music-gift] review: empty output, falling back to original");
        raw_output.to_string()
    } else {
        reviewed
    }
}

#[cfg(test)]
mod tests {
    use orchest_protocol::{MediaSource, Role};
    use super::*;

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
        assert_eq!(result.style, DEFAULT_STYLE);
        assert_eq!(result.title, "");
        assert_eq!(result.vocal, "female");
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
