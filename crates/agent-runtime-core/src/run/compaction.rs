//! Context compaction: summarize old messages to reduce context window usage.

use std::sync::Arc;

use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::model::{ContentBlock, Message, ModelAdapter, Role, TokenUsage};

use super::config::{AgentConfig, RunId};
use super::helpers::emit;

/// Compact the conversation context if the token usage ratio exceeds the
/// configured threshold.  Old messages (minus `recent_messages`) are
/// summarised by the model and replaced with a single summary message.
#[allow(clippy::too_many_arguments)] // justified: compaction logic needs full runtime context; will refactor with CompactionState struct in v0.7
pub(crate) async fn maybe_compact_context(
    config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    messages: &mut Vec<Message>,
    tx: &mpsc::Sender<RuntimeEvent>,
    last_compaction_step: &mut Option<u32>,
    step: u32,
    usage: &TokenUsage,
    run_id: RunId,
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

    let mut compact_ctx = crate::hook::CompactHookContext {
        run_id,
        messages: messages.clone(),
        token_count: usage.input_tokens as u32,
    };
    match crate::hook::runner::run_before_compact(&config.hooks, &mut compact_ctx, tx).await {
        crate::hook::HookAction::Skip | crate::hook::HookAction::Abort(_) => return,
        crate::hook::HookAction::Continue => {}
    }
    *messages = compact_ctx.messages;

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;

    use crate::model::{
        ModelCapabilities, ModelError, ModelResponse, RequestOptions, StopReason, StreamEvent,
        ToolDef,
    };

    use crate::run::CompactionConfig;

    /// Minimal mock that returns a fixed summary text.
    struct SummaryMock {
        call_count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl ModelAdapter for SummaryMock {
        fn provider_name(&self) -> &str {
            "mock"
        }
        fn model_name(&self) -> &str {
            "mock-model"
        }
        fn capabilities(&self) -> ModelCapabilities {
            ModelCapabilities::default()
        }
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDef],
            _options: &RequestOptions,
            _tx: Option<mpsc::Sender<StreamEvent>>,
        ) -> Result<ModelResponse, ModelError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse {
                content: vec![ContentBlock::Text("test summary".into())],
                usage: TokenUsage {
                    output_tokens: 5,
                    ..Default::default()
                },
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            })
        }
    }

    fn budget_none() -> crate::budget::BudgetConfig {
        crate::budget::BudgetConfig {
            max_tokens: None,
            max_tool_calls: None,
            max_duration: None,
            max_cost_usd: None,
        }
    }

    fn make_config(threshold: f32, recent: usize, context_window: Option<u64>) -> AgentConfig {
        AgentConfig {
            system_prompt: "system".into(),
            model: crate::run::ModelConfig {
                spec: crate::model::ModelSpec {
                    provider: "mock".into(),
                    model: "mock".into(),
                    api_key_env: None,
                    api_url: None,
                    max_tokens: None,
                    context_window_size: context_window,
                },
                options: RequestOptions::default(),
            },
            budget: budget_none(),
            skills: crate::run::SkillsConfig::default(),
            runtime: crate::run::RuntimeConfig {
                compaction: Some(CompactionConfig {
                    threshold,
                    recent_messages: recent,
                }),
                ..Default::default()
            },
            hooks: vec![],
            retry_policy: None,
            handoffs: vec![],
        }
    }

    fn user_msg(text: &str) -> Message {
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    fn assistant_msg(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Text(text.into())],
        }
    }

    #[tokio::test]
    async fn no_compaction_when_disabled() {
        let config = AgentConfig {
            system_prompt: "s".into(),
            model: crate::run::ModelConfig::default(),
            budget: budget_none(),
            skills: crate::run::SkillsConfig::default(),
            runtime: crate::run::RuntimeConfig {
                compaction: None,
                ..Default::default()
            },
            hooks: vec![],
            retry_policy: None,
            handoffs: vec![],
        };
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, _rx) = mpsc::channel(16);
        let mut messages = vec![user_msg("a"), assistant_msg("b")];
        let mut last = None;
        let usage = TokenUsage {
            input_tokens: 900,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert_eq!(messages.len(), 2, "messages should be unchanged");
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "model should not be called"
        );
    }

    #[tokio::test]
    async fn no_compaction_below_threshold() {
        let config = make_config(0.8, 2, Some(1000));
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, _rx) = mpsc::channel(16);
        let mut messages = vec![
            user_msg("a"),
            assistant_msg("b"),
            user_msg("c"),
            assistant_msg("d"),
        ];
        let mut last = None;
        // 30% usage — below 80% threshold
        let usage = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert_eq!(
            messages.len(),
            4,
            "messages should be unchanged below threshold"
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn compaction_triggers_above_threshold() {
        let config = make_config(0.8, 2, Some(1000));
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, mut rx) = mpsc::channel(16);
        let mut messages = vec![
            user_msg("a"),
            assistant_msg("b"),
            user_msg("c"),
            assistant_msg("d"),
            user_msg("e"),
        ];
        let mut last = None;
        // 90% usage — above 80% threshold
        let usage = TokenUsage {
            input_tokens: 800,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "model should be called once"
        );
        // Should keep recent_messages=2 non-system messages + 1 summary
        // Original: 5 non-system messages, keep last 2, add 1 summary = 3
        assert_eq!(messages.len(), 3);
        // First message should be the summary
        let summary_text = match &messages[0].content[0] {
            ContentBlock::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(
            summary_text.contains("test summary"),
            "summary should contain mock text"
        );
        assert_eq!(last, Some(10));

        // Should emit ContextCompacted event
        let event = rx.try_recv().expect("should have event");
        assert!(matches!(event, RuntimeEvent::ContextCompacted { .. }));
    }

    #[tokio::test]
    async fn no_compaction_too_few_messages() {
        // recent_messages=5, only 3 non-system messages → not enough to compact
        let config = make_config(0.5, 5, Some(1000));
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, _rx) = mpsc::channel(16);
        let mut messages = vec![user_msg("a"), assistant_msg("b"), user_msg("c")];
        let mut last = None;
        let usage = TokenUsage {
            input_tokens: 900,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert_eq!(messages.len(), 3);
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn no_compaction_within_cooldown() {
        let config = make_config(0.5, 1, Some(1000));
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, _rx) = mpsc::channel(16);
        let mut messages = vec![
            user_msg("a"),
            assistant_msg("b"),
            user_msg("c"),
            assistant_msg("d"),
        ];
        // Last compaction was at step 8, current step is 10 → only 2 steps apart (< 5)
        let mut last = Some(8);
        let usage = TokenUsage {
            input_tokens: 900,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert_eq!(messages.len(), 4);
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn empty_messages_no_crash() {
        let config = make_config(0.5, 2, Some(1000));
        let call_count = Arc::new(AtomicU32::new(0));
        let model: Arc<dyn ModelAdapter> = Arc::new(SummaryMock {
            call_count: call_count.clone(),
        });
        let (tx, _rx) = mpsc::channel(16);
        let mut messages: Vec<Message> = vec![];
        let mut last = None;
        let usage = TokenUsage {
            input_tokens: 900,
            output_tokens: 100,
            ..Default::default()
        };

        maybe_compact_context(
            &config,
            &model,
            &mut messages,
            &tx,
            &mut last,
            10,
            &usage,
            crate::run::RunId::new(),
        )
        .await;

        assert!(messages.is_empty());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }
}
