//! Message preparation: system prompt, photo blocks, and chat message assembly.

use orchest_protocol::{ContentBlock, MediaSource, Message, Role};
use serde::Deserialize;
use serde_json::Value;

use crate::prompts::SYSTEM_PROMPT;

pub fn build_system_message(meta: &Value, photo_count: usize) -> Message {
    let mut system = SYSTEM_PROMPT.as_str().to_string();
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
        if role == Role::System {
            // Clients (free-create mode) may send their own system
            // instruction: merge it into the system prompt instead of
            // silently dropping it (previously this arm discarded it).
            // Assumes the leading system message is a single Text block, and
            // that incoming system messages are prompt-level instructions
            // (merging them to the top), not mid-conversation asides.
            if let Some(ContentBlock::Text(system)) = messages[0].content.first_mut() {
                system.push_str("\n\n");
                system.push_str(&msg.content);
            }
        } else if role == Role::User && !photo_injected && !photo_blocks.is_empty() {
            let mut content = photo_blocks.to_vec();
            content.push(ContentBlock::Text(msg.content.clone()));
            messages.push(Message { role, content });
            photo_injected = true;
        } else {
            messages.push(Message {
                role,
                content: vec![ContentBlock::Text(msg.content.clone())],
            });
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_messages_merges_incoming_system_into_system_prompt() {
        let system = build_system_message(&serde_json::json!({"name": "x"}), 0);
        let incoming = vec![
            IncomingMessage {
                role: "system".into(),
                content: "You are a professional songwriter.".into(),
            },
            IncomingMessage {
                role: "user".into(),
                content: "hi".into(),
            },
        ];
        let messages = build_messages(system, &incoming, &[]);
        assert_eq!(messages.len(), 2);
        let Some(ContentBlock::Text(system_text)) = messages[0].content.first() else {
            panic!("system message must be a text block");
        };
        assert!(
            system_text.contains("You are a professional songwriter."),
            "incoming system instruction must be merged, got: {system_text}"
        );
        assert!(matches!(messages[1].role, Role::User));
    }
}
