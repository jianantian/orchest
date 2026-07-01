//! `before_model` adapter for [`InputGuardrail`].

use std::sync::Arc;

use async_trait::async_trait;

use super::{InputGuardrail, InputGuardrailAction};
use crate::hook::{Hook, ModelHookAction, ModelHookContext};

pub(crate) struct InputGuardrailHook(pub Arc<dyn InputGuardrail>);

#[async_trait]
impl Hook for InputGuardrailHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        match self.0.check(ctx).await {
            InputGuardrailAction::Allow => ModelHookAction::Continue,
            InputGuardrailAction::Replace(messages) => {
                ctx.messages = messages;
                ModelHookAction::Continue
            }
            InputGuardrailAction::Abort(reason) => ModelHookAction::Abort(reason),
        }
    }
}
