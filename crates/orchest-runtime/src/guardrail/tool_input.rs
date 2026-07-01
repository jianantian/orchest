//! `before_tool` adapter for [`ToolInputGuardrail`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{ToolInputGuardrail, ToolInputGuardrailAction};
use crate::hook::{Hook, HookAction, ToolHookContext};

pub(crate) struct ToolInputGuardrailHook(pub Arc<dyn ToolInputGuardrail>);

#[async_trait]
impl Hook for ToolInputGuardrailHook {
    async fn before_tool(&self, ctx: &mut ToolHookContext) -> HookAction {
        match self.0.check(ctx).await {
            ToolInputGuardrailAction::Allow => HookAction::Continue,
            ToolInputGuardrailAction::Modify(input) => {
                ctx.tool_input = input;
                HookAction::Continue
            }
            ToolInputGuardrailAction::Reject(reason) => HookAction::Reject(reason),
            ToolInputGuardrailAction::Abort(reason) => HookAction::Abort(reason),
        }
    }
}
