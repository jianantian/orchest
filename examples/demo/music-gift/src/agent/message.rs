//! Message preparation: system prompt, photo blocks, and chat message assembly.

use orchest_protocol::{ContentBlock, MediaSource, Message, Role};
use serde::Deserialize;
use serde_json::Value;

use crate::prompts::{LYRICS_SKILL, SYSTEM_PROMPT};

pub fn build_system_message(meta: &Value, photo_count: usize) -> Message {
    let mut system = format!("{}\n\n{}", SYSTEM_PROMPT.as_str(), LYRICS_SKILL.as_str());
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

#[derive(Debug, Deserialize)]
pub struct IncomingMessage {
    pub role: String,
    pub content: String,
}

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
            let mut content = photo_blocks.to_vec();
            content.push(ContentBlock::Text(msg.content.clone()));
            messages.push(Message { role, content });
            photo_injected = true;
        } else if role != Role::System {
            messages.push(Message {
                role,
                content: vec![ContentBlock::Text(msg.content.clone())],
            });
        }
    }
    messages
}
