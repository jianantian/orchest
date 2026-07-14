//! System prompt, lyrics parsing, and chat streaming bridge.

use std::sync::Arc;

use orchest_protocol::{
    ChatModel, ContentBlock, EventStream, MediaSource, Message, RequestOptions, Role, StreamEvent,
    ToolDef,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

use crate::error::{AppError, AppResult};

/// The EN system prompt (simplified port of the original JS prompt).
///
/// Phase 1: dialogue to find the one personal detail. Phase 2: generate
/// structured lyrics with `<<<LYRICS>>>` / `<<<STYLE>>>` / `<<<TITLE>>>` /
/// `<<<VOCAL>>>` tags.
pub const CHAT_SYSTEM_PROMPT: &str = r#"You are Moment's creative assistant, helping users craft a personalized song for someone special.

═══ Phase 1: Gathering material (dialogue) ═══

Your goal is to find that one detail only the two of them know - something that makes the recipient freeze when they hear the song.

Dialogue rules:
- Only one question at a time; follow up on the user's last message, don't jump topics
- Chat like a friend, not a form; don't explain what you're doing, just talk
- Never start with "OK", "Got it", "Right" or other filler
- If the user shared something specific, latch onto the detail; avoid closed questions
- You need to find: ① a concrete, visual scene (something you can picture happening) ② the emotional direction (what does the sender most want to convey)
- Name/nickname is already known (see Known Info) - never ask again
- Maximum two questions. Once you have enough material, generate immediately - never drag it out
- First question: dig into the scene's specific details - what movement, sound, expression stood out?
- Second question (if the first round wasn't enough): open-ended close - "Is there anything you'd want this song to say for you?"
- If the user's first message already has enough detail, generate right away, no follow-ups

═══ Phase 2: Generating lyrics ═══

When you judge there's enough specific detail and emotional direction, append to your reply (user won't see):
<<<READY>>>
Then immediately generate lyrics, with the first line being the style tag (based on the conversation, keep it short):

<<<LYRICS>>>
<<<STYLE>>>warm and gentle<<<STYLE_END>>>
<<<TITLE>>>The Magic Wave<<<TITLE_END>>>
<<<VOCAL>>>female<<<VOCAL_END>>>
[verse 1]
[verse 2]
[chorus]
[chorus]
<<<END>>>

Style reference: warm and gentle / healing and warm / lively and joyful / deep and moving

TITLE rules:
- 2-6 words, sayable in one breath
- Pick the most visual image from the lyrics as the title, never an abstract emotion word

VOCAL rules:
- Only female or male
- Default: opposite of recipient's gender (for her -> male, for him -> female)

═══ Lyrics Writing Methodology (must follow) ═══

Structure:
- verse 1: 4 lines, all concrete images and actions, zero emotion words, establish the scene
- verse 2: 4 lines, advance the narrative, must not be a rewrite of verse 1 (twin verses are fatal)
- chorus: 4 lines, emotional resonance, the recipient's name/nickname may appear; on repeat, slightly vary the second line

Show Don't Tell (core principle):
Convey emotion through action, imagery, and sensory details - never through direct emotion words.

Required personal elements:
- The recipient's name or nickname
- That one specific personal detail you dug out from the dialogue

Hard prohibitions:
- Twin verses (V2 is just rewording V1)
- Verse-Chorus echo (verse ending leaks chorus imagery/rhyme)
- Orphan lines in the rhyme scheme
- Direct emotion words in verses ("miss", "love", "touched", etc.)"#;

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
///
/// Tags (matching the original JS app):
/// - `<<<LYRICS>>>...<<<END>>>` - the lyrics body
/// - `<<<STYLE>>>...<<<STYLE_END>>>` - style description
/// - `<<<TITLE>>>...<<<TITLE_END>>>` - song title
/// - `<<<VOCAL>>>female|male<<<VOCAL_END>>>` - vocal gender
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

/// Extract the substring between `open` and `close` tags.
fn extract_between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}

/// Build the system message, optionally appending meta (known info) and a photo
/// note.
pub fn build_system_message(meta: &Value, photo_count: usize) -> Message {
    let mut system = CHAT_SYSTEM_PROMPT.to_string();

    if !meta.is_null() {
        system.push_str("\n\nKnown info:\n");
        system.push_str(&serde_json::to_string_pretty(meta).unwrap_or_default());
    }

    if photo_count > 0 {
        let plural = if photo_count == 1 { "" } else { "s" };
        system.push_str(&format!(
            "\n\nThe user uploaded {photo_count} photo{plural} (attached to the first message). \
             Look closely: find a specific frame, expression, object, quality of light - \
             something only they would recognize - and use it in your follow-up question \
             or directly in the lyrics. Don't describe the photo abstractly."
        ));
    }

    Message {
        role: Role::System,
        content: vec![ContentBlock::Text(system)],
    }
}

/// Load photos from disk and construct `ContentBlock::Image` blocks for vision.
pub fn build_photo_blocks(photos: &[String], data_dir: &str) -> Vec<ContentBlock> {
    let photos_dir = std::path::Path::new(data_dir).join("photos");
    let mut blocks = Vec::new();
    for path in photos {
        let filename = path.trim_start_matches("/photos/");
        if filename.contains('/') || filename.contains("..") {
            continue;
        }
        let file_path = photos_dir.join(filename);
        let data = match std::fs::read(&file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let media_type = match file_path.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        };
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);
        blocks.push(ContentBlock::Image {
            source: MediaSource::Base64 {
                media_type: media_type.to_string(),
                data: b64,
            },
            detail: None,
        });
    }
    blocks
}

/// Request body for the chat endpoint.
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<IncomingMessage>,
    #[serde(default)]
    pub meta: Value,
    #[serde(default = "default_lang")]
    #[allow(dead_code)]
    pub lang: String,
    #[serde(default)]
    pub photos: Vec<String>,
}

fn default_lang() -> String {
    "en".to_string()
}

/// A message as received from the frontend (simpler than the protocol's Message).
#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub role: String,
    pub content: String,
}

/// Convert incoming messages to protocol Messages, prepending the system message
/// and optionally injecting photo blocks into the first user message.
pub fn build_messages(
    system_msg: Message,
    incoming: &[IncomingMessage],
    photo_blocks: &[ContentBlock],
) -> Vec<Message> {
    let mut messages = vec![system_msg];
    let mut photo_injected = false;

    for msg in incoming {
        let role = match msg.role.as_str() {
            "assistant" => Role::Assistant,
            "system" => Role::System,
            _ => Role::User,
        };

        if role == Role::User && !photo_injected && !photo_blocks.is_empty() {
            // Prepend photo blocks to the first user message
            let mut content = photo_blocks.to_vec();
            content.push(ContentBlock::Text(msg.content.clone()));
            messages.push(Message { role, content });
            photo_injected = true;
        } else if role != Role::System {
            // Skip system messages from incoming (we build our own)
            messages.push(Message {
                role,
                content: vec![ContentBlock::Text(msg.content.clone())],
            });
        }
    }

    messages
}

/// SSE events sent to the frontend (serialized as JSON in the SSE data field).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SseEvent {
    Delta {
        text: String,
    },
    Done {
        has_lyrics: bool,
        lyrics: String,
        style: String,
        title: String,
        vocal: String,
    },
    Error {
        error: String,
    },
}

/// Start a streaming chat completion and return an `EventStream` to pull from.
pub fn start_stream(model: Arc<dyn ChatModel>, messages: Vec<Message>) -> EventStream {
    let options = RequestOptions {
        max_tokens: Some(2048),
        ..Default::default()
    };
    orchest_provider_http::events(model, messages, Vec::<ToolDef>::new(), options)
}

/// Drive the event stream, collecting text and sending `SseEvent`s via `tx`.
///
/// Returns the full accumulated text on success, or an error if the stream
/// ended with a fatal error.
pub async fn drive_stream(
    mut stream: EventStream,
    tx: mpsc::Sender<SseEvent>,
) -> AppResult<String> {
    let mut full_text = String::new();
    let mut had_error = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::Text { delta } => {
                full_text.push_str(&delta);
                let _ = tx
                    .send(SseEvent::Delta {
                        text: delta.clone(),
                    })
                    .await;
            }
            StreamEvent::Error { error, fatal } => {
                let msg = error.to_string();
                let _ = tx.send(SseEvent::Error { error: msg }).await;
                if fatal {
                    had_error = true;
                    break;
                }
            }
            StreamEvent::Done { .. } => break,
            _ => {}
        }
    }

    if had_error {
        return Err(AppError::Llm("stream ended with fatal error".to_string()));
    }

    Ok(full_text)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(result.vocal, "female"); // falls back to default

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
        assert_eq!(messages.len(), 2); // system + user
        let user_content = &messages[1].content;
        assert_eq!(user_content.len(), 2); // photo + text
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
