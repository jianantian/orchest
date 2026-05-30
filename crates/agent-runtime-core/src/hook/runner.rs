//! Chain-call runners for Hook trait methods with panic recovery.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt;
use tokio::sync::mpsc;

use crate::events::RuntimeEvent;
use crate::run::helpers::emit;

use super::{
    CompactHookContext, HandoffHookContext, Hook, HookAction, ModelHookAction, ModelHookContext,
    RunHookContext, ToolHookContext,
};

pub(crate) async fn run_on_run_start(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut RunHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.on_run_start(ctx))
            .catch_unwind()
            .await;
        if let Err(panic) = result {
            emit(
                tx,
                RuntimeEvent::HookPanicked {
                    hook_name: "on_run_start".to_string(),
                    message: format!("{panic:?}"),
                },
            )
            .await;
        }
    }
}

pub(crate) async fn run_on_run_end(
    hooks: &[Arc<dyn Hook>],
    ctx: &RunHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.on_run_end(ctx)).catch_unwind().await;
        if let Err(panic) = result {
            emit(
                tx,
                RuntimeEvent::HookPanicked {
                    hook_name: "on_run_end".to_string(),
                    message: format!("{panic:?}"),
                },
            )
            .await;
        }
    }
}

pub(crate) async fn run_on_run_error(
    hooks: &[Arc<dyn Hook>],
    ctx: &RunHookContext,
    error: &str,
    tx: &mpsc::Sender<RuntimeEvent>,
) {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.on_run_error(ctx, error))
            .catch_unwind()
            .await;
        if let Err(panic) = result {
            emit(
                tx,
                RuntimeEvent::HookPanicked {
                    hook_name: "on_run_error".to_string(),
                    message: format!("{panic:?}"),
                },
            )
            .await;
        }
    }
}

pub(crate) async fn run_before_model(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ModelHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> ModelHookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.before_model(ctx))
            .catch_unwind()
            .await;
        match result {
            Ok(ModelHookAction::Abort(reason)) => return ModelHookAction::Abort(reason),
            Ok(ModelHookAction::Continue) => {}
            Err(panic) => {
                emit(
                    tx,
                    RuntimeEvent::HookPanicked {
                        hook_name: "before_model".to_string(),
                        message: format!("{panic:?}"),
                    },
                )
                .await;
            }
        }
    }
    ModelHookAction::Continue
}

pub(crate) async fn run_after_model(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ModelHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.after_model(ctx)).catch_unwind().await;
        match result {
            Ok(HookAction::Abort(reason)) => return HookAction::Abort(reason),
            Ok(HookAction::Skip | HookAction::Continue) => {}
            Err(panic) => {
                emit(
                    tx,
                    RuntimeEvent::HookPanicked {
                        hook_name: "after_model".to_string(),
                        message: format!("{panic:?}"),
                    },
                )
                .await;
            }
        }
    }
    HookAction::Continue
}

pub(crate) async fn run_before_tool(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ToolHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.before_tool(ctx)).catch_unwind().await;
        match result {
            Ok(HookAction::Abort(reason)) => return HookAction::Abort(reason),
            Ok(HookAction::Skip) => return HookAction::Skip,
            Ok(HookAction::Continue) => {}
            Err(panic) => {
                emit(
                    tx,
                    RuntimeEvent::HookPanicked {
                        hook_name: "before_tool".to_string(),
                        message: format!("{panic:?}"),
                    },
                )
                .await;
            }
        }
    }
    HookAction::Continue
}

pub(crate) async fn run_after_tool(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut ToolHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.after_tool(ctx)).catch_unwind().await;
        match result {
            Ok(HookAction::Abort(reason)) => return HookAction::Abort(reason),
            Ok(HookAction::Skip | HookAction::Continue) => {}
            Err(panic) => {
                emit(
                    tx,
                    RuntimeEvent::HookPanicked {
                        hook_name: "after_tool".to_string(),
                        message: format!("{panic:?}"),
                    },
                )
                .await;
            }
        }
    }
    HookAction::Continue
}

pub(crate) async fn run_on_handoff(
    hooks: &[Arc<dyn Hook>],
    ctx: &HandoffHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.on_handoff(ctx)).catch_unwind().await;
        if let Err(panic) = result {
            emit(
                tx,
                RuntimeEvent::HookPanicked {
                    hook_name: "on_handoff".to_string(),
                    message: format!("{panic:?}"),
                },
            )
            .await;
        }
    }
}

pub(crate) async fn run_before_compact(
    hooks: &[Arc<dyn Hook>],
    ctx: &mut CompactHookContext,
    tx: &mpsc::Sender<RuntimeEvent>,
) -> HookAction {
    for hook in hooks {
        let result = AssertUnwindSafe(hook.before_compact(ctx))
            .catch_unwind()
            .await;
        match result {
            Ok(HookAction::Abort(reason)) => return HookAction::Abort(reason),
            Ok(HookAction::Skip) => return HookAction::Skip,
            Ok(HookAction::Continue) => {}
            Err(panic) => {
                emit(
                    tx,
                    RuntimeEvent::HookPanicked {
                        hook_name: "before_compact".to_string(),
                        message: format!("{panic:?}"),
                    },
                )
                .await;
            }
        }
    }
    HookAction::Continue
}
