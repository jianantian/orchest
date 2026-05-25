// Context compaction: summarize old messages to reduce context window usage.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::model::{ContentBlock, Message, ModelAdapter, Role, TokenUsage};

use super::config::AgentConfig;
use super::helpers::emit;

pub(crate) async fn maybe_compact_context(
    config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    messages: &mut Vec<Message>,
    tx: &mpsc::Sender<RuntimeEvent>,
    last_compaction_step: &mut Option<u32>,
    step: u32,
    usage: &TokenUsage,
) {
    let Some(ref compaction) = config.runtime.compaction else {
        return;
    };
    let threshold = compaction.threshold;
    let Some(context_window_size) = config.model.spec.context_window_size else {
        return;
    };
    if !(0.0..=1.0).contains(&threshold) || context_window_size == 0 {
        return;
    }
    if let Some(last) = *last_compaction_step {
        if step.saturating_sub(last) < 5 {
            return;
        }
    }
    let used = usage.input_tokens.saturating_add(usage.output_tokens);
    if (used as f32 / context_window_size as f32) < threshold {
        return;
    }
    let recent_count = compaction.recent_messages;
    if messages.len() <= recent_count + 1 {
        return;
    }

    let system = messages
        .iter()
        .find(|message| matches!(message.role, Role::System))
        .cloned();
    let non_system: Vec<Message> = messages
        .iter()
        .filter(|message| !matches!(message.role, Role::System))
        .cloned()
        .collect();
    if non_system.len() <= recent_count {
        return;
    }
    let split_at = non_system.len() - recent_count;
    let old_messages = &non_system[..split_at];
    let recent_messages = non_system[split_at..].to_vec();
    let history = old_messages
        .iter()
        .map(|message| serde_json::to_string(message).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = crate::prompts::COMPACTION_SUMMARY_PROMPT.replace("{history}", &history);
    let summary_response = model
        .complete(
            &[Message {
                role: Role::User,
                content: vec![ContentBlock::Text(prompt)],
            }],
            &[],
            &config.model.options,
            None,
        )
        .await;

    let response = match summary_response {
        Ok(response) => response,
        Err(error) => {
            emit(
                tx,
                RuntimeEvent::RuntimeWarning {
                    message: format!("context compaction failed: {}", error.message),
                },
            )
            .await;
            return;
        }
    };
    let summary = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let mut compacted = Vec::new();
    if let Some(system) = system {
        compacted.push(system);
    }
    compacted.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text(format!(
            "{}{summary}",
            crate::prompts::COMPACTION_SUMMARY_PREFIX
        ))],
    });
    compacted.extend(recent_messages);
    let removed_messages = messages.len().saturating_sub(compacted.len());
    *messages = compacted;
    *last_compaction_step = Some(step);
    emit(
        tx,
        RuntimeEvent::ContextCompacted {
            removed_messages,
            summary_tokens: response.usage.output_tokens as u32,
        },
    )
    .await;
}
