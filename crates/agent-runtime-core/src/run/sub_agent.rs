//! Sub-agent execution through AgentDelegate.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::budget::{BudgetGuard, BudgetUsage};
use crate::events::RuntimeEvent;
use crate::tool::AgentDelegate;

use super::config::{AgentConfig, AgentRun, RunId, SubAgentRuntime};
use super::handle::ApprovalBus;
use super::helpers::emit;
use tokio_util::sync::CancellationToken;

#[allow(clippy::too_many_arguments)] // justified: sub-agent delegation requires parent context, budget, and cancellation; planned struct in v0.7
pub(crate) async fn execute_agent_delegate(
    parent_run_id: RunId,
    parent_config: &AgentConfig,
    tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
    delegate: AgentDelegate,
    approval_bus: ApprovalBus,
    cancel_token: CancellationToken,
) -> (Value, Value) {
    let placeholder_child_run_id = RunId::new();
    if parent_config.runtime.run_depth >= 3 {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id: placeholder_child_run_id,
                error: "max_run_depth_exceeded".into(),
            },
        )
        .await;
        let details = json!({
            "child_run_id": placeholder_child_run_id,
            "error": "max_run_depth_exceeded",
            "budget_used": BudgetUsage::default(),
        });
        let model_output = (delegate.output_mapper)(details.clone());
        return (model_output, details);
    }

    let mut child_config = delegate.config.clone();
    let remaining = parent_budget.remaining_config();
    child_config.budget = SubAgentRuntime::cap_budget(&child_config.budget, &remaining);
    child_config.runtime.run_depth = parent_config.runtime.run_depth + 1;

    let (handle, mut child_rx) = AgentRun::start_with_bus_and_token(
        child_config,
        delegate.input.clone(),
        Arc::clone(&delegate.model),
        delegate.registry.clone(),
        approval_bus,
        cancel_token.child_token(),
    );
    let child_run_id = handle.run_id;

    emit(
        tx,
        RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id,
            config_summary: json!({
                "run_depth": parent_config.runtime.run_depth + 1,
                "input": delegate.input,
            }),
        },
    )
    .await;

    let mut child_usage = BudgetUsage::default();
    let mut output = Value::Null;
    let mut failed = None;

    while let Some(event) = child_rx.recv().await {
        match &event {
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                let tokens_used = tokens.input_tokens + tokens.output_tokens;
                let cost_usd = tokens.cost_usd.unwrap_or(0.0);
                child_usage.tokens_used += tokens_used;
                child_usage.cost_usd += cost_usd;
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used,
                    tool_calls_used: 0,
                    cost_usd,
                });
            }
            RuntimeEvent::ToolCallCompleted { .. } => {
                child_usage.tool_calls_used += 1;
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used: 0,
                    tool_calls_used: 1,
                    cost_usd: 0.0,
                });
            }
            RuntimeEvent::RunCompleted {
                output: child_output,
            } => {
                output = child_output.clone();
            }
            RuntimeEvent::RunFailed { error } => {
                failed = Some(error.clone());
            }
            _ => {}
        }
        emit(
            tx,
            RuntimeEvent::SubAgentEvent {
                parent_run_id,
                child_run_id,
                event: Box::new(event),
            },
        )
        .await;
    }
    handle.wait().await;

    let details = if let Some(error) = failed {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: error.clone(),
            },
        )
        .await;
        json!({
            "child_run_id": child_run_id,
            "error": error,
            "budget_used": child_usage,
        })
    } else {
        emit(
            tx,
            RuntimeEvent::SubAgentCompleted {
                child_run_id,
                output: output.clone(),
                budget_used: child_usage.clone(),
            },
        )
        .await;
        json!({
            "child_run_id": child_run_id,
            "output": output,
            "budget_used": child_usage,
        })
    };

    let model_output = (delegate.output_mapper)(details.clone());
    (model_output, details)
}
