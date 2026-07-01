//! `after_tool` adapter for [`ToolOutputGuardrail`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{ToolOutputGuardrail, ToolOutputGuardrailAction};
use crate::hook::{Hook, HookAction, ToolHookContext};

pub(crate) struct ToolOutputGuardrailHook(pub Arc<dyn ToolOutputGuardrail>);

#[async_trait]
impl Hook for ToolOutputGuardrailHook {
    async fn after_tool(&self, ctx: &mut ToolHookContext) -> HookAction {
        match self.0.check(ctx).await {
            ToolOutputGuardrailAction::Allow => HookAction::Continue,
            ToolOutputGuardrailAction::Modify(output) => {
                // 001 reads ctx.tool_output back and replaces the tool result content.
                ctx.tool_output = Some(output);
                HookAction::Continue
            }
            ToolOutputGuardrailAction::Abort(reason) => HookAction::Abort(reason),
        }
    }
}
