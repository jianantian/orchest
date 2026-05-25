// Sub-agent execution: both __sub_agent_request and AgentDelegate paths.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};

use crate::budget::{BudgetGuard, BudgetUsage};
use crate::events::RuntimeEvent;
use crate::model::ModelAdapter;
use crate::tool::registry::ToolRegistry;
use crate::tool::AgentDelegate;

use super::config::{AgentConfig, AgentRun, RunId, SubAgentRuntime};
use super::handle::ApprovalSlot;
use super::helpers::{emit, narrow_permission_list, parse_budget_config};

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_sub_agent_request(
    parent_run_id: RunId,
    parent_config: &AgentConfig,
    model: &Arc<dyn ModelAdapter>,
    registry: &ToolRegistry,
    tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
    request: &Value,
    active_children: &Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
) -> Value {
    let child_run_id = RunId::new();
    if parent_config.run_depth >= 3 {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: "max_run_depth_exceeded".into(),
            },
        )
        .await;
        return json!({"error": "max_run_depth_exceeded"});
    }
    let remaining = parent_budget.remaining_config();
    if remaining.max_tokens == Some(0)
        || remaining.max_tool_calls == Some(0)
        || remaining.max_duration == Some(Duration::ZERO)
        || remaining.max_cost_usd == Some(0.0)
    {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id,
                error: "parent_budget_exhausted".into(),
            },
        )
        .await;
        return json!({"error": "parent_budget_exhausted"});
    }

    let requested_budget = request
        .get("config")
        .and_then(|config| config.get("budget"))
        .map(parse_budget_config)
        .unwrap_or_else(|| remaining.clone());
    let mut child_config = parent_config.clone();
    child_config.budget = SubAgentRuntime::cap_budget(&requested_budget, &remaining);
    child_config.run_depth = parent_config.run_depth + 1;
    if let Some(requested_tools) = request
        .get("config")
        .and_then(|config| config.get("allowed_tools"))
        .and_then(Value::as_array)
    {
        let requested: Vec<String> = requested_tools
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        child_config.allowed_tools = Some(narrow_permission_list(
            &parent_config.allowed_tools,
            &requested,
        ));
    }
    if let Some(requested_skills) = request
        .get("config")
        .and_then(|config| config.get("allowed_skills"))
        .and_then(Value::as_array)
    {
        let requested: Vec<String> = requested_skills
            .iter()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        child_config.allowed_skills = Some(narrow_permission_list(
            &parent_config.allowed_skills,
            &requested,
        ));
    }
    let input = request
        .get("input")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    let child_registry = registry.filter_by_allowed(&child_config.allowed_tools);
    let (handle, mut child_rx) =
        AgentRun::start(child_config, input, Arc::clone(model), child_registry);
    let actual_child_run_id = handle.run_id;

    // Register child approval slot so parent RunHandle can route
    // approval responses to the child run.
    {
        let mut children = active_children.lock().await;
        children.insert(actual_child_run_id, Arc::clone(&handle.pending_approval));
    }

    emit(
        tx,
        RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id: actual_child_run_id,
            config_summary: json!({
                "run_depth": parent_config.run_depth + 1,
                "budget": request.get("config").and_then(|config| config.get("budget")).cloned().unwrap_or(Value::Null),
            }),
        },
    )
    .await;
    let mut child_usage = BudgetUsage::default();
    let mut output = Value::Null;
    let mut failed = None;

    let child_depth = parent_config.run_depth + 1;
    while let Some(event) = child_rx.recv().await {
        match &event {
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                child_usage.tokens_used += tokens.input_tokens + tokens.output_tokens;
                // Propagate to parent budget immediately so the parent
                // guard reflects child consumption in real time.
                let incremental = BudgetUsage {
                    tokens_used: tokens.input_tokens + tokens.output_tokens,
                    tool_calls_used: 0,
                    cost_usd: 0.0,
                };
                parent_budget.record_external_usage(&incremental);
            }
            RuntimeEvent::ToolCallCompleted { .. } => {
                child_usage.tool_calls_used += 1;
                let incremental = BudgetUsage {
                    tokens_used: 0,
                    tool_calls_used: 1,
                    cost_usd: 0.0,
                };
                parent_budget.record_external_usage(&incremental);
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
        // Wrap child events with identity metadata so consumers can
        // distinguish root vs child run events
        emit(
            tx,
            RuntimeEvent::ChildRunEvent {
                child_run_id: actual_child_run_id,
                run_depth: child_depth,
                event: Box::new(event),
            },
        )
        .await;
    }
    handle.wait().await;

    // Deregister child from active children map
    {
        let mut children = active_children.lock().await;
        children.remove(&actual_child_run_id);
    }

    // Note: parent_budget was already updated incrementally above.
    // child_usage is kept for the SubAgentCompleted event payload.

    if let Some(error) = failed {
        emit(
            tx,
            RuntimeEvent::SubAgentFailed {
                child_run_id: actual_child_run_id,
                error: error.clone(),
            },
        )
        .await;
        json!({"error": error})
    } else {
        emit(
            tx,
            RuntimeEvent::SubAgentCompleted {
                child_run_id: actual_child_run_id,
                output: output.clone(),
                budget_used: child_usage,
            },
        )
        .await;
        output
    }
}

pub(crate) async fn execute_agent_delegate(
    parent_run_id: RunId,
    parent_config: &AgentConfig,
    tx: &mpsc::Sender<RuntimeEvent>,
    parent_budget: &mut BudgetGuard,
    delegate: AgentDelegate,
    active_children: &Arc<Mutex<HashMap<RunId, ApprovalSlot>>>,
) -> (Value, Value) {
    let mut child_config = delegate.config.clone();
    let remaining = parent_budget.remaining_config();
    child_config.budget = SubAgentRuntime::cap_budget(&child_config.budget, &remaining);
    child_config.run_depth = parent_config.run_depth + 1;

    let (handle, mut child_rx) = AgentRun::start(
        child_config,
        delegate.input.clone(),
        Arc::clone(&delegate.model),
        delegate.registry.clone(),
    );
    let child_run_id = handle.run_id;

    {
        let mut children = active_children.lock().await;
        children.insert(child_run_id, Arc::clone(&handle.pending_approval));
    }

    emit(
        tx,
        RuntimeEvent::SubAgentStarted {
            parent_run_id,
            child_run_id,
            config_summary: json!({
                "run_depth": parent_config.run_depth + 1,
                "input": delegate.input,
            }),
        },
    )
    .await;

    let mut child_usage = BudgetUsage::default();
    let mut output = Value::Null;
    let mut failed = None;
    let child_depth = parent_config.run_depth + 1;

    while let Some(event) = child_rx.recv().await {
        match &event {
            RuntimeEvent::ModelCallCompleted { tokens, .. } => {
                let tokens_used = tokens.input_tokens + tokens.output_tokens;
                child_usage.tokens_used += tokens_used;
                parent_budget.record_external_usage(&BudgetUsage {
                    tokens_used,
                    tool_calls_used: 0,
                    cost_usd: 0.0,
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
            RuntimeEvent::ChildRunEvent {
                child_run_id,
                run_depth: child_depth,
                event: Box::new(event),
            },
        )
        .await;
    }
    handle.wait().await;

    {
        let mut children = active_children.lock().await;
        children.remove(&child_run_id);
    }

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
