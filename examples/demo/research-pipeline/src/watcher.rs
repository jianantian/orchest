//! Deterministic watcher used to attribute post-registration events/actions.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchest::{
    events::RuntimeEvent,
    run::{llm_watcher::LlmWatcher, Watcher, WatcherAction},
};
use tokio::sync::Notify;

pub struct RecordingActionWatcher {
    events: Arc<Mutex<Vec<RuntimeEvent>>>,
    action_tool: Option<String>,
    action: Mutex<Option<WatcherAction>>,
    activation_step: Option<u32>,
    activation_processed: Option<Arc<Notify>>,
    terminal_processed: Arc<Notify>,
}

impl RecordingActionWatcher {
    pub fn on_tool_until_terminal(
        events: Arc<Mutex<Vec<RuntimeEvent>>>,
        tool: impl Into<String>,
        action: WatcherAction,
        activation: (u32, Arc<Notify>),
        terminal_processed: Arc<Notify>,
    ) -> Self {
        Self {
            events,
            action_tool: Some(tool.into()),
            action: Mutex::new(Some(action)),
            activation_step: Some(activation.0),
            activation_processed: Some(activation.1),
            terminal_processed,
        }
    }

    pub fn observing_until_terminal(
        events: Arc<Mutex<Vec<RuntimeEvent>>>,
        terminal_processed: Arc<Notify>,
    ) -> Self {
        Self {
            events,
            action_tool: None,
            action: Mutex::new(None),
            activation_step: None,
            activation_processed: None,
            terminal_processed,
        }
    }
}

#[async_trait]
impl Watcher for RecordingActionWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        let action = match event {
            RuntimeEvent::ToolCallStarted { tool, .. }
                if self.action_tool.as_deref() == Some(tool.as_str()) =>
            {
                self.action.lock().ok().and_then(|mut action| action.take())
            }
            _ => None,
        };

        if let Ok(mut events) = self.events.lock() {
            events.push(event.clone());
        }

        if matches!(
            event,
            RuntimeEvent::ModelCallStarted { step, .. }
                if Some(*step) == self.activation_step
        ) {
            if let Some(activation_processed) = &self.activation_processed {
                activation_processed.notify_one();
            }
        }

        if matches!(
            event,
            RuntimeEvent::RunCompleted { .. }
                | RuntimeEvent::RunFailed { .. }
                | RuntimeEvent::RunAborted { .. }
        ) {
            self.terminal_processed.notify_one();
        }

        action.unwrap_or(WatcherAction::Continue)
    }
}

pub struct RecordingLlmWatcher {
    inner: LlmWatcher,
    completed_events: Arc<Mutex<Vec<RuntimeEvent>>>,
    activation: Option<(u32, Arc<Notify>)>,
    terminal_processed: Arc<Notify>,
}

impl RecordingLlmWatcher {
    pub fn until_terminal(
        inner: LlmWatcher,
        completed_events: Arc<Mutex<Vec<RuntimeEvent>>>,
        activation: Option<(u32, Arc<Notify>)>,
        terminal_processed: Arc<Notify>,
    ) -> Self {
        Self {
            inner,
            completed_events,
            activation,
            terminal_processed,
        }
    }
}

#[async_trait]
impl Watcher for RecordingLlmWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        let action = self.inner.on_event(event).await;

        if let Ok(mut events) = self.completed_events.lock() {
            events.push(event.clone());
        }

        if let (
            RuntimeEvent::ModelCallStarted { step, .. },
            Some((activation_step, activation_processed)),
        ) = (event, &self.activation)
        {
            if step == activation_step {
                activation_processed.notify_one();
            }
        }

        if matches!(
            event,
            RuntimeEvent::RunCompleted { .. }
                | RuntimeEvent::RunFailed { .. }
                | RuntimeEvent::RunAborted { .. }
        ) {
            self.terminal_processed.notify_one();
        }

        action
    }
}
