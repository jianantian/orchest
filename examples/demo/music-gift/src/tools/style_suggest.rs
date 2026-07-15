//! Style suggestion: analyze lyrics and recommend matching styles from the
//! catalog using a single LLM call (no agent loop needed).

use std::sync::Arc;

use orchest_provider_http::chat;
use orchest_protocol::{ChatModel, ContentBlock, Message, RequestOptions, Role};

use crate::error::AppResult;

const STYLE_CATALOG: &str = include_str!("../../frontend/src/lib/styles.ts");

/// Suggest 3-5 styles matching the given lyrics content.
/// Uses a one-shot LLM call — no agent, no tools, no streaming.
pub async fn suggest_styles(
    model: &Arc<dyn ChatModel>,
    lyrics: &str,
) -> AppResult<Vec<String>> {
    let prompt = format!(
        r#"You are a music style analyst. Given song lyrics, recommend 3-5 music production styles from the catalog below that best match the lyrical content.

Consider: emotional tone, rhythm, imagery, theme, and energy level.

Return ONLY a JSON array of style strings, nothing else. Example: ["warm acoustic", "gentle ballad"]

CATALOG:
{STYLE_CATALOG}

LYRICS:
{lyrics}"#
    );

    let messages = [Message {
        role: Role::User,
        content: vec![ContentBlock::Text(prompt)],
    }];

    let options = RequestOptions {
        max_tokens: Some(256),
        ..Default::default()
    };

    let response = chat(model.as_ref(), &messages, &[], &options).await
        .map_err(|e| crate::error::AppError::Llm(e.to_string()))?;

    let text: String = response.content.iter()
        .filter_map(|b| match b {
            ContentBlock::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    let text = text.trim();
    let json_str = text
        .strip_prefix("```json").unwrap_or(text)
        .strip_prefix("```").unwrap_or(text)
        .strip_suffix("```").unwrap_or(text)
        .trim();

    let styles: Vec<String> = serde_json::from_str(json_str)
        .unwrap_or_else(|_| vec![
            "warm acoustic".into(),
            "gentle ballad".into(),
        ]);

    Ok(styles)
}
