//! Provider-independent rendering of public runtime events.

use orchest::events::RuntimeEvent;

pub fn render_event(event: &RuntimeEvent) -> String {
    render_scoped_event("run", event)
}

fn render_scoped_event(scope: &str, event: &RuntimeEvent) -> String {
    match event {
        RuntimeEvent::SubAgentEvent { event, .. } | RuntimeEvent::ChildRunEvent { event, .. } => {
            render_scoped_event("worker", event)
        }
        RuntimeEvent::ModelCallStarted { step } => {
            format!("[{scope}] model turn started: step {step}")
        }
        RuntimeEvent::ModelCallCompleted { tokens, .. } => format!(
            "[{scope}] model turn completed: {} input tokens, {} output tokens",
            tokens.input_tokens, tokens.output_tokens
        ),
        RuntimeEvent::ModelRetry { attempt, error, .. } => {
            format!("[{scope}] model turn retry {attempt}: {error}")
        }
        RuntimeEvent::ToolCallStarted { tool, .. } => {
            format!("[{scope}] tool call started: {tool}")
        }
        RuntimeEvent::ToolCallCompleted { tool, duration, .. } => {
            format!("[{scope}] tool result: {tool} ({}ms)", duration.as_millis())
        }
        RuntimeEvent::ToolCallFailed { tool, error } => {
            format!("[{scope}] tool result: {tool} failed ({error})")
        }
        RuntimeEvent::RunCompleted { stop_reason, .. } => {
            format!("[{scope}] terminal status: completed ({stop_reason:?})")
        }
        RuntimeEvent::RunFailed { error, .. } => {
            format!("[{scope}] terminal status: failed ({error})")
        }
        RuntimeEvent::RunAborted { reason } => match reason {
            Some(reason) => format!("[{scope}] terminal status: aborted ({reason})"),
            None => format!("[{scope}] terminal status: aborted"),
        },
        RuntimeEvent::SubAgentStarted { child_run_id, .. } => {
            format!("[worker] started: {child_run_id}")
        }
        RuntimeEvent::SubAgentCompleted { child_run_id, .. } => {
            format!("[worker] terminal status: completed ({child_run_id})")
        }
        RuntimeEvent::SubAgentFailed {
            child_run_id,
            error,
        } => format!("[worker] terminal status: failed ({child_run_id}: {error})"),
        other => format!("[{scope}] event: {other:?}"),
    }
}
