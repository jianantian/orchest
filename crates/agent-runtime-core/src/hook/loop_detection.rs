//! Loop detection hook: detects repeated tool call patterns and warns or aborts.

use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;

use crate::model::{ContentBlock, Message, Role};

use super::{Hook, HookAction, ModelHookAction, ModelHookContext, ToolHookContext};

/// Configuration for the loop detection hook.
#[derive(Debug, Clone)]
pub struct LoopDetectionConfig {
    /// Sliding window size: number of recent tool calls to examine (default: 10).
    pub window_size: usize,
    /// Number of repeated calls before injecting a warning (default: 3).
    pub warn_threshold: usize,
    /// Number of repeated calls before aborting the run (default: 5).
    pub stop_threshold: usize,
}

impl Default for LoopDetectionConfig {
    fn default() -> Self {
        Self {
            window_size: 10,
            warn_threshold: 3,
            stop_threshold: 5,
        }
    }
}

#[derive(Debug)]
struct ToolCallPattern {
    tool_name: String,
    /// Canonical (key-sorted) JSON string used for both identity and hash.
    input_canonical: String,
    input_hash: u64,
}

impl ToolCallPattern {
    fn new(tool_name: String, input: &Value) -> Self {
        let input_canonical = canonical_json(input);
        let input_hash = hash_str(&input_canonical);
        Self {
            tool_name,
            input_canonical,
            input_hash,
        }
    }
}

struct LoopState {
    window: VecDeque<ToolCallPattern>,
    pending_warning: Option<String>,
}

/// Two-phase loop detection hook.
///
/// `after_tool`: records patterns, sets warning flag or aborts.
/// `before_model`: injects any pending warning into the message list.
pub struct LoopDetectionHook {
    config: LoopDetectionConfig,
    state: Mutex<LoopState>,
}

impl LoopDetectionHook {
    pub fn new(config: LoopDetectionConfig) -> Self {
        Self {
            state: Mutex::new(LoopState {
                window: VecDeque::new(),
                pending_warning: None,
            }),
            config,
        }
    }
}

impl Default for LoopDetectionHook {
    fn default() -> Self {
        Self::new(LoopDetectionConfig::default())
    }
}

impl From<LoopDetectionConfig> for LoopDetectionHook {
    fn from(config: LoopDetectionConfig) -> Self {
        Self::new(config)
    }
}

#[async_trait]
impl Hook for LoopDetectionHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        let warning = {
            let mut state = self.state.lock().unwrap();
            state.pending_warning.take()
        };
        if let Some(msg) = warning {
            ctx.messages.push(Message {
                role: Role::User,
                content: vec![ContentBlock::Text(msg)],
            });
        }
        ModelHookAction::Continue
    }

    async fn after_tool(&self, ctx: &mut ToolHookContext) -> HookAction {
        let pattern = ToolCallPattern::new(ctx.tool_name.clone(), &ctx.tool_input);

        let mut state = self.state.lock().unwrap();

        // Slide the window: evict oldest entry when at capacity.
        if state.window.len() >= self.config.window_size {
            state.window.pop_front();
        }
        state.window.push_back(pattern);

        // Count matching patterns in window (hash pre-filter + full compare).
        let last = state.window.back().unwrap();
        let count = state
            .window
            .iter()
            .filter(|p| {
                p.tool_name == last.tool_name
                    && p.input_hash == last.input_hash
                    && p.input_canonical == last.input_canonical
            })
            .count();

        if count >= self.config.stop_threshold {
            return HookAction::Abort("loop detected".into());
        }
        if count >= self.config.warn_threshold {
            let tool_name = last.tool_name.clone();
            let window_size = self.config.window_size;
            state.pending_warning = Some(format!(
                "[Warning] You have called the tool '{tool_name}' with similar arguments \
                 {count} times in the last {window_size} calls. This suggests a loop. \
                 Please try a different approach or use a different tool."
            ));
        }
        HookAction::Continue
    }
}

/// Produce a canonical (key-sorted) JSON string for stable hashing.
fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut pairs: Vec<(&str, String)> = map
                .iter()
                .map(|(k, v)| (k.as_str(), canonical_json(v)))
                .collect();
            pairs.sort_by_key(|(k, _)| *k);
            let inner = pairs
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", k, v))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{}}}", inner)
        }
        Value::Array(arr) => {
            let inner = arr.iter().map(canonical_json).collect::<Vec<_>>().join(",");
            format!("[{}]", inner)
        }
        _ => value.to_string(),
    }
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolMetadata;
    use serde_json::json;

    fn make_ctx(tool_name: &str, input: Value) -> ToolHookContext {
        ToolHookContext {
            run_id: crate::run::RunId::new(),
            tool_name: tool_name.to_string(),
            tool_input: input,
            tool_metadata: ToolMetadata {
                side_effect: false,
                approval: crate::tool::Approval::Never,
                cost_hint: None,
                timeout: None,
                max_output_tokens: None,
                source: crate::tool::ToolSource::Builtin,
            },
            tool_output: None,
        }
    }

    fn make_model_ctx() -> ModelHookContext {
        ModelHookContext {
            run_id: crate::run::RunId::new(),
            messages: vec![],
            model_spec: crate::model::ModelSpec {
                provider: "mock".into(),
                model: "mock".into(),
                api_key_env: None,
                api_url: None,
                max_tokens: None,
                context_window_size: None,
            },
            response: None,
        }
    }

    #[tokio::test]
    async fn warning_injected_at_warn_threshold() {
        let hook = LoopDetectionHook::new(LoopDetectionConfig {
            window_size: 10,
            warn_threshold: 3,
            stop_threshold: 5,
        });

        let input = json!({"q": "same"});
        for _ in 0..3 {
            let action = hook
                .after_tool(&mut make_ctx("search", input.clone()))
                .await;
            assert!(matches!(action, HookAction::Continue));
        }

        // Warning should now be pending.
        let mut model_ctx = make_model_ctx();
        hook.before_model(&mut model_ctx).await;
        assert_eq!(model_ctx.messages.len(), 1);
        let msg_text = match &model_ctx.messages[0].content[0] {
            ContentBlock::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(msg_text.contains("search"), "warning mentions tool name");
        assert!(msg_text.contains("loop"), "warning mentions loop");

        // Second before_model call should not inject again.
        let mut model_ctx2 = make_model_ctx();
        hook.before_model(&mut model_ctx2).await;
        assert!(
            model_ctx2.messages.is_empty(),
            "warning consumed after first inject"
        );
    }

    #[tokio::test]
    async fn abort_at_stop_threshold() {
        let hook = LoopDetectionHook::new(LoopDetectionConfig {
            window_size: 10,
            warn_threshold: 3,
            stop_threshold: 5,
        });

        let input = json!({"q": "same"});
        for i in 0..5 {
            let action = hook
                .after_tool(&mut make_ctx("search", input.clone()))
                .await;
            if i < 4 {
                assert!(matches!(action, HookAction::Continue));
            } else {
                assert!(
                    matches!(action, HookAction::Abort(_)),
                    "5th call should abort"
                );
            }
        }
    }

    #[tokio::test]
    async fn different_inputs_do_not_trigger() {
        let hook = LoopDetectionHook::new(LoopDetectionConfig {
            window_size: 10,
            warn_threshold: 3,
            stop_threshold: 5,
        });

        for i in 0..10 {
            let input = json!({"q": i});
            let action = hook.after_tool(&mut make_ctx("search", input)).await;
            assert!(
                matches!(action, HookAction::Continue),
                "different inputs should never trigger"
            );
        }

        // No pending warning.
        let mut model_ctx = make_model_ctx();
        hook.before_model(&mut model_ctx).await;
        assert!(model_ctx.messages.is_empty());
    }

    #[tokio::test]
    async fn window_sliding_evicts_old_patterns() {
        let hook = LoopDetectionHook::new(LoopDetectionConfig {
            window_size: 4,
            warn_threshold: 3,
            stop_threshold: 5,
        });

        let repeat = json!({"q": "same"});

        // 2 repeated calls — below warn_threshold.
        hook.after_tool(&mut make_ctx("s", repeat.clone())).await;
        hook.after_tool(&mut make_ctx("s", repeat.clone())).await;

        // 4 distinct calls that fully evict the repeated ones from the window.
        for i in 0..4_u32 {
            hook.after_tool(&mut make_ctx("s", json!({"q": i}))).await;
        }

        // Window is now [distinct×4]; one more "same" should count as 1, below threshold.
        let action = hook.after_tool(&mut make_ctx("s", repeat.clone())).await;
        assert!(
            matches!(action, HookAction::Continue),
            "evicted patterns should not count"
        );

        // No pending warning (the pending warning from any intermediate distinct pattern is
        // not set because all four distinct inputs are different from each other).
        let mut model_ctx = make_model_ctx();
        hook.before_model(&mut model_ctx).await;
        assert!(model_ctx.messages.is_empty());
    }

    #[tokio::test]
    async fn canonical_json_is_key_order_independent() {
        let v1 = json!({"b": 1, "a": 2});
        let v2 = json!({"a": 2, "b": 1});
        assert_eq!(canonical_json(&v1), canonical_json(&v2));
    }
}
