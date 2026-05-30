//! Handoff types: control-flow transfer from one agent to another.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::model::Message;
use crate::run::config::AgentConfig;

// ---------------------------------------------------------------------------
// Public traits
// ---------------------------------------------------------------------------

/// Resolves a handoff target dynamically based on the tool call input.
#[async_trait]
pub trait HandoffResolver: Send + Sync {
    async fn resolve(&self, input: Value) -> Result<AgentConfig, HandoffError>;
}

/// Filters the conversation history before passing it to the new agent.
#[async_trait]
pub trait HandoffInputFilter: Send + Sync {
    async fn filter(&self, data: HandoffInputData) -> HandoffInputData;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Input data available to a `HandoffInputFilter`.
pub struct HandoffInputData {
    /// Full conversation history (including system message) up to the handoff.
    pub history: Vec<Message>,
    /// The raw JSON input passed by the model to the handoff tool.
    pub handoff_input: Value,
}

/// The configured target of a handoff.
#[derive(Clone)]
pub enum HandoffTarget {
    /// A fixed agent config resolved at registration time.
    Static(Box<AgentConfig>),
    /// A dynamic resolver that chooses the target at invocation time.
    Dynamic(Arc<dyn HandoffResolver>),
}

/// A handoff descriptor registered on an `AgentConfig`.
///
/// At runtime this is exposed to the model as a regular tool named
/// `tool_name`; when the model calls it, the run loop switches to
/// `target` and the session continues under the new agent.
#[derive(Clone)]
pub struct Handoff {
    pub tool_name: String,
    pub tool_description: String,
    pub input_schema: Value,
    pub target: HandoffTarget,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    /// When `true` the history is folded into a single assistant message
    /// before being passed to the new agent.
    pub nest_history: bool,
}

/// The resolved result produced by `HandoffTool::execute`.
///
/// Carried inside `ToolOutput::Handoff`; the run loop consumes it.
pub struct HandoffResult {
    pub target_agent: AgentConfig,
    pub transfer_message: String,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub nest_history: bool,
}

impl std::fmt::Debug for HandoffResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandoffResult")
            .field("transfer_message", &self.transfer_message)
            .field("nest_history", &self.nest_history)
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error("handoff resolution failed: {0}")]
    Resolution(String),
}

// ---------------------------------------------------------------------------
// Message filtering helpers
// ---------------------------------------------------------------------------

impl HandoffResult {
    /// Produce the message list to pass to the new agent.
    ///
    /// The new agent's system prompt is prepended by the run loop separately;
    /// this method returns only the non-system turn messages.
    pub async fn apply_filter(&self, history: Vec<Message>, handoff_input: Value) -> Vec<Message> {
        if let Some(filter) = &self.input_filter {
            let data = filter
                .filter(HandoffInputData {
                    history: history.clone(),
                    handoff_input,
                })
                .await;
            data.history
        } else if self.nest_history {
            fold_history(history)
        } else {
            history
        }
    }
}

/// Fold all non-system messages into a single User message containing
/// the conversation history wrapped in `<CONVERSATION HISTORY>` tags.
fn fold_history(messages: Vec<Message>) -> Vec<Message> {
    use crate::model::{ContentBlock, Role};

    let system: Vec<Message> = messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .cloned()
        .collect();

    let non_system: Vec<Message> = messages
        .into_iter()
        .filter(|m| !matches!(m.role, Role::System))
        .collect();

    if non_system.is_empty() {
        return system;
    }

    let serialized = non_system
        .iter()
        .map(|m| serde_json::to_string(m).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    let folded = Message {
        role: Role::User,
        content: vec![ContentBlock::Text(format!(
            "<CONVERSATION HISTORY>\n{serialized}\n</CONVERSATION HISTORY>"
        ))],
    };

    let mut result = system;
    result.push(folded);
    result
}
