//! Guardrail framework: a convenience layer over the Hook trait for the four
//! common review points (model input/output, tool input/output).
//!
//! A guardrail only *decides* (reads a read-only context, returns an action);
//! the adapter that bridges it to a [`crate::hook::Hook`] performs the actual
//! context mutation. This keeps the "review" and "apply" responsibilities apart.

use async_trait::async_trait;

use crate::hook::{ModelHookContext, ToolHookContext};
use crate::model::{ContentBlock, Message};

mod input;
mod output;
mod tool_input;
mod tool_output;

pub(crate) use input::InputGuardrailHook;
pub(crate) use output::OutputGuardrailHook;
pub(crate) use tool_input::ToolInputGuardrailHook;
pub(crate) use tool_output::ToolOutputGuardrailHook;

/// Decision for an [`InputGuardrail`] (runs at `before_model`).
#[non_exhaustive]
pub enum InputGuardrailAction {
    Allow,
    /// Replace the messages sent to the model.
    Replace(Vec<Message>),
    /// Abort the run with a reason.
    Abort(String),
}

/// Decision for an [`OutputGuardrail`] (runs at `after_model`).
#[non_exhaustive]
pub enum OutputGuardrailAction {
    Allow,
    /// Replace the model response content (flows into conversation history).
    Replace(Vec<ContentBlock>),
    Abort(String),
}

/// Decision for a [`ToolInputGuardrail`] (runs at `before_tool`).
#[non_exhaustive]
pub enum ToolInputGuardrailAction {
    Allow,
    /// Replace the tool input before execution.
    Modify(serde_json::Value),
    /// Reject the call, returning `reason` to the model as the tool result.
    Reject(String),
    Abort(String),
}

/// Decision for a [`ToolOutputGuardrail`] (runs at `after_tool`).
#[non_exhaustive]
pub enum ToolOutputGuardrailAction {
    Allow,
    /// Replace the tool output the model sees.
    Modify(serde_json::Value),
    Abort(String),
}

/// Reviews the messages about to be sent to the model.
#[async_trait]
pub trait InputGuardrail: Send + Sync {
    async fn check(&self, ctx: &ModelHookContext) -> InputGuardrailAction;
}

/// Reviews the model's response.
#[async_trait]
pub trait OutputGuardrail: Send + Sync {
    async fn check(&self, ctx: &ModelHookContext) -> OutputGuardrailAction;
}

/// Reviews a tool call's input before execution.
#[async_trait]
pub trait ToolInputGuardrail: Send + Sync {
    async fn check(&self, ctx: &ToolHookContext) -> ToolInputGuardrailAction;
}

/// Reviews a tool call's output after execution.
#[async_trait]
pub trait ToolOutputGuardrail: Send + Sync {
    async fn check(&self, ctx: &ToolHookContext) -> ToolOutputGuardrailAction;
}

#[cfg(test)]
mod tests;
