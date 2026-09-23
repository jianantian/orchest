//! Deterministic watcher used to attribute post-registration events/actions.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchest::{
    events::RuntimeEvent,
    run::{LlmWatcher, Watcher, WatcherAction},
};
use tokio::sync::Notify;

/// Maps runtime events to deterministic milestone keys while deliberately
/// omitting volatile payloads such as run ids, durations, and model output.
pub fn stable_event_key(event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::RunStarted { .. } => "run.started".to_string(),
        RuntimeEvent::ModelCallStarted { step } => format!("model.started:{step}"),
        RuntimeEvent::ModelStreamChunk { .. } => "model.stream".to_string(),
        RuntimeEvent::ModelCallCompleted { .. } => "model.completed".to_string(),
        RuntimeEvent::ModelRetry { attempt, .. } => format!("model.retry:{attempt}"),
        RuntimeEvent::ToolCallStarted { tool, .. } => format!("tool.started:{tool}"),
        RuntimeEvent::ToolCallUpdate { tool, .. } => format!("tool.updated:{tool}"),
        RuntimeEvent::ToolCallCompleted { tool, .. } => format!("tool.completed:{tool}"),
        RuntimeEvent::ToolCallFailed { tool, .. } => format!("tool.failed:{tool}"),
        RuntimeEvent::ToolCallRetry { tool, attempt, .. } => {
            format!("tool.retry:{tool}:{attempt}")
        }
        RuntimeEvent::ToolCallBatchStarted { tool_count, .. } => {
            format!("tool.batch.started:{tool_count}")
        }
        RuntimeEvent::ToolCallBatchItemStarted {
            tool,
            requested_order,
            ..
        } => format!("tool.batch.item.started:{tool}:{requested_order}"),
        RuntimeEvent::ToolCallBatchItemCompleted {
            tool,
            requested_order,
            completion_order,
            ..
        } => format!("tool.batch.item.completed:{tool}:{requested_order}:{completion_order}"),
        RuntimeEvent::AsyncToolStarted { tool, .. } => format!("async.started:{tool}"),
        RuntimeEvent::AsyncToolProgress { tool, .. } => format!("async.progress:{tool}"),
        RuntimeEvent::AsyncToolCompleted { tool, .. } => format!("async.completed:{tool}"),
        RuntimeEvent::SkillContentRead { skill_name, .. } => {
            format!("skill.content:{skill_name}")
        }
        RuntimeEvent::ApprovalRequested { tool_call, .. } => {
            format!("approval.requested:{}", tool_call.name)
        }
        RuntimeEvent::ApprovalGranted { tool_call, .. } => {
            format!("approval.granted:{}", tool_call.name)
        }
        RuntimeEvent::ApprovalDenied { tool_call, .. } => {
            format!("approval.denied:{}", tool_call.name)
        }
        RuntimeEvent::BudgetWarning { .. } => "budget.warning".to_string(),
        RuntimeEvent::RuntimeWarning { .. } => "runtime.warning".to_string(),
        RuntimeEvent::SkillMissingCapabilities { skill_name } => {
            format!("skill.missing-capabilities:{skill_name}")
        }
        RuntimeEvent::SkillLoadWarning { .. } => "skill.load-warning".to_string(),
        RuntimeEvent::ContextCompacted { .. } => "context.compacted".to_string(),
        RuntimeEvent::SubAgentStarted { .. } => "sub-agent.started".to_string(),
        RuntimeEvent::SubAgentCompleted { .. } => "sub-agent.completed".to_string(),
        RuntimeEvent::SubAgentFailed { .. } => "sub-agent.failed".to_string(),
        RuntimeEvent::ChildRunEvent { event, .. } => {
            format!("child.event:{}", stable_event_key(event))
        }
        RuntimeEvent::SubAgentEvent { event, .. } => {
            format!("sub-agent.event:{}", stable_event_key(event))
        }
        RuntimeEvent::HookPanicked { hook_name, .. } => format!("hook.panicked:{hook_name}"),
        RuntimeEvent::AgentUpdated { .. } => "agent.updated".to_string(),
        RuntimeEvent::EventsDropped {
            subscriber_id,
            count,
            from_seq,
            to_seq,
        } => format!("events.dropped:{subscriber_id}:{count}:{from_seq}:{to_seq}"),
        RuntimeEvent::RunRestarted { attempt } => format!("run.restarted:{attempt}"),
        RuntimeEvent::RunCompleted { stop_reason, .. } => {
            format!("run.completed:{stop_reason:?}")
        }
        RuntimeEvent::RunFailed { .. } => "run.failed".to_string(),
        RuntimeEvent::RunAborted { .. } => "run.aborted".to_string(),
        _ => "unknown".to_string(),
    }
}

/// Deterministic observer for FIFO and no-drop equivalence evidence.
///
/// It never returns an action, so matching sequences cannot be interpreted as
/// evidence for cross-watcher action application order.
pub struct CountingWatcher {
    sequence: Arc<Mutex<Vec<String>>>,
    activation: Option<(u32, Arc<Notify>)>,
    terminal_processed: Arc<Notify>,
}

impl CountingWatcher {
    pub fn until_terminal(
        sequence: Arc<Mutex<Vec<String>>>,
        activation: Option<(u32, Arc<Notify>)>,
        terminal_processed: Arc<Notify>,
    ) -> Self {
        Self {
            sequence,
            activation,
            terminal_processed,
        }
    }
}

#[async_trait]
impl Watcher for CountingWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        if let Ok(mut sequence) = self.sequence.lock() {
            sequence.push(stable_event_key(event));
        }

        if let (
            RuntimeEvent::ModelCallStarted { step },
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

        WatcherAction::Continue
    }
}

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
