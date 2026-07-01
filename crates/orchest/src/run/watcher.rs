//! Watcher trait: observe and optionally intervene in an agent run.

use async_trait::async_trait;

use crate::events::RuntimeEvent;

/// Action returned by a watcher after observing a runtime event.
pub enum WatcherAction {
    Continue,
    /// Inject a user-role message into the run's conversation at the next model call.
    Inject(String),
    /// Inject a system-role steering instruction into the run's conversation.
    Steer(String),
    /// Terminate the run; `reason` is surfaced in `RuntimeEvent::RunAborted`.
    Abort(String),
}

/// Observe and optionally intervene in an agent run's event stream.
#[async_trait]
pub trait Watcher: Send + Sync {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction;
}
