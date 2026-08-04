//! Message preparation: system prompt, photo blocks, and chat message assembly.

use orchest_protocol::{ContentBlock, MediaSource, Message, Role};
use serde::Deserialize;
use serde_json::Value;

use crate::prompts::{STUDIO_SYSTEM_PROMPT, SYSTEM_PROMPT};

pub fn build_system_message(meta: &Value, photo_count: usize) -> Message {
    let mut system = SYSTEM_PROMPT.as_str().to_string();
    if !meta.is_null() {
        system.push_str("\n\nKnown info:\n");
        system.push_str(&serde_json::to_string_pretty(meta).unwrap_or_default());
    }
    push_photo_note(&mut system, photo_count);
    Message {
        role: Role::System,
        content: vec![ContentBlock::Text(system)],
    }
}

/// System message for studio mode: the co-editing prompt plus the user's
/// current draft (present fields only). No guided "Known info" meta block —
/// studio works on the draft, not on the intake form.
pub fn build_studio_system_message(draft: Option<&StudioDraft>, photo_count: usize) -> Message {
    let mut system = STUDIO_SYSTEM_PROMPT.as_str().to_string();
    if let Some(draft) = draft {
        let mut current = serde_json::Map::new();
        for (key, value) in [
            ("lyrics", &draft.lyrics),
            ("style", &draft.style),
            ("title", &draft.title),
            ("vocal", &draft.vocal),
        ] {
            if let Some(value) = value {
                current.insert(key.to_string(), Value::String(value.clone()));
            }
        }
        if !current.is_empty() {
            system.push_str("\n\nCurrent draft:\n");
            system.push_str(
                &serde_json::to_string_pretty(&Value::Object(current)).unwrap_or_default(),
            );
        }
    }
    push_photo_note(&mut system, photo_count);
    Message {
        role: Role::System,
        content: vec![ContentBlock::Text(system)],
    }
}

/// Append the photo-attachment note shared by both system prompts.
fn push_photo_note(system: &mut String, photo_count: usize) {
    if photo_count > 0 {
        let plural = if photo_count == 1 { "" } else { "s" };
        system.push_str(&format!(
            "\n\nThe user uploaded {photo_count} photo{plural} (attached to the first message). \
             Look closely: find a specific frame, expression, object, quality of light - \
             something only they would recognize - and use it in your follow-up question \
             or directly in the lyrics. Don't describe the photo abstractly."
        ));
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
    /// Collaboration mode: `"studio"` switches to the co-editing prompt and
    /// skips the elevate/review pipeline. Absent = guided mode (unchanged).
    #[serde(default)]
    pub mode: Option<String>,
    /// The user's current working draft; only meaningful in studio mode.
    #[serde(default)]
    pub draft: Option<StudioDraft>,
}

/// The client's current working draft, sent with studio-mode chat requests.
/// An absent field means "no value yet", not "cleared".
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct StudioDraft {
    pub lyrics: Option<String>,
    pub style: Option<String>,
    pub title: Option<String>,
    pub vocal: Option<String>,
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

    fn text_of(msg: &Message) -> String {
        match &msg.content[0] {
            ContentBlock::Text(s) => s.clone(),
            _ => String::new(),
        }
    }

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

    #[test]
    fn studio_system_message_contains_draft_and_not_guided_prompt() {
        let draft = StudioDraft {
            lyrics: Some("[verse 1]\n骑吧 阿杰".to_string()),
            style: Some("warm folk".to_string()),
            title: Some("Wheels at Dusk".to_string()),
            vocal: Some("male".to_string()),
        };
        let msg = build_studio_system_message(Some(&draft), 0);
        let content = text_of(&msg);
        assert!(content.contains(STUDIO_SYSTEM_PROMPT.as_str()));
        assert!(
            !content.contains(SYSTEM_PROMPT.as_str()),
            "studio message must not embed the guided prompt"
        );
        assert!(
            !content.contains("Known info"),
            "studio message must not carry the guided meta block"
        );
        for field in ["骑吧 阿杰", "warm folk", "Wheels at Dusk", "male"] {
            assert!(content.contains(field), "draft field missing: {field}");
        }
    }

    #[test]
    fn studio_system_message_omits_absent_draft_fields() {
        let draft = StudioDraft {
            style: Some("City Pop".to_string()),
            ..StudioDraft::default()
        };
        let content = text_of(&build_studio_system_message(Some(&draft), 0));
        assert!(content.contains("City Pop"));
        assert!(
            !content.contains("\"lyrics\"") && !content.contains("\"title\""),
            "absent draft fields must not appear, got: {content}"
        );

        // No draft at all: the message is the bare studio prompt.
        let content = text_of(&build_studio_system_message(None, 0));
        assert_eq!(content, STUDIO_SYSTEM_PROMPT.as_str());
    }

    #[test]
    fn studio_system_message_includes_photo_note() {
        let content = text_of(&build_studio_system_message(None, 2));
        assert!(content.contains("2 photos"));
    }

    #[test]
    fn chat_request_without_mode_deserializes_as_guided() {
        // Back-compat: existing clients send neither mode nor draft.
        let req: ChatRequest = serde_json::from_value(serde_json::json!({
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .expect("deserialize");
        assert!(req.mode.is_none());
        assert!(req.draft.is_none());
    }
}
