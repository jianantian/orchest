//! `after_model` adapter for [`OutputGuardrail`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{OutputGuardrail, OutputGuardrailAction};
use crate::hook::{Hook, HookAction, ModelHookContext};

pub(crate) struct OutputGuardrailHook(pub Arc<dyn OutputGuardrail>);

#[async_trait]
impl Hook for OutputGuardrailHook {
    async fn after_model(&self, ctx: &mut ModelHookContext) -> HookAction {
        match self.0.check(ctx).await {
            OutputGuardrailAction::Allow => HookAction::Continue,
            OutputGuardrailAction::Replace(content) => {
                // 001 reads ctx.response back into the response that flows to history.
                ctx.response = Some(content);
                HookAction::Continue
            }
            OutputGuardrailAction::Abort(reason) => HookAction::Abort(reason),
        }
    }
}
