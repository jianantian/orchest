//! Application-layer authority policy guardrail.
//!
//! Run with: cargo run --example guardrail_authority_policy

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use orchest::events::RuntimeEvent;
use orchest::guardrail::{ToolInputGuardrail, ToolInputGuardrailAction};
use orchest::hook::ToolHookContext;
use orchest::model::{
    ContentBlock, Message, ModelAdapter, ModelCapabilities, ModelError, ModelResponse,
    RequestOptions, StopReason, StreamEvent, TokenUsage,
};
use orchest::run::{AgentConfig, AgentRun};
use orchest::tool::{
    registry::ToolRegistry, Approval, JsonSchema, Tool, ToolContext, ToolDef, ToolError,
    ToolExecutionMode, ToolMetadata, ToolOutput, ToolParallelism, ToolSource,
};
use serde_json::json;
use tokio::sync::mpsc;

struct PolicyModel {
    call: AtomicU32,
}

impl PolicyModel {
    fn new() -> Self {
        Self {
            call: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl ModelAdapter for PolicyModel {
    fn provider_name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        "mock"
    }

    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
        _options: &RequestOptions,
        tx: Option<mpsc::Sender<StreamEvent>>,
    ) -> Result<ModelResponse, ModelError> {
        let call = self.call.fetch_add(1, Ordering::SeqCst);
        let usage = TokenUsage {
            input_tokens: 12,
            output_tokens: 8,
            ..Default::default()
        };
        if let Some(tx) = tx {
            let _ = tx
                .send(StreamEvent::Done {
                    usage: usage.clone(),
                })
                .await;
        }

        let tool_results = messages
            .iter()
            .flat_map(|message| &message.content)
            .filter(|block| matches!(block, ContentBlock::ToolResult { .. }))
            .count();

        match (call, tool_results) {
            (0, _) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_high_risk".into(),
                    name: "change_account_limit".into(),
                    input: json!({
                        "actor_role": "support",
                        "risk": "high",
                        "account_id": "acct_1",
                        "new_limit_usd": 500_000
                    }),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            (_, 1) => Ok(ModelResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "call_manager_reviewed".into(),
                    name: "change_account_limit".into(),
                    input: json!({
                        "actor_role": "manager",
                        "risk": "medium",
                        "account_id": "acct_1",
                        "new_limit_usd": 25_000
                    }),
                }],
                usage,
                stop_reason: StopReason::ToolUse,
                option_adjustments: vec![],
            }),
            _ => Ok(ModelResponse {
                content: vec![ContentBlock::Text("Policy flow complete.".into())],
                usage,
                stop_reason: StopReason::EndTurn,
                option_adjustments: vec![],
            }),
        }
    }
}

struct AuthorityPolicyGuardrail {
    rejected_high_risk: Arc<AtomicBool>,
    allowed_manager_medium: Arc<AtomicBool>,
}

#[async_trait]
impl ToolInputGuardrail for AuthorityPolicyGuardrail {
    async fn check(&self, ctx: &ToolHookContext) -> ToolInputGuardrailAction {
        let actor_role = ctx
            .tool_input
            .get("actor_role")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let risk = ctx
            .tool_input
            .get("risk")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");

        match (actor_role, risk) {
            ("support", "low") | ("manager", "low" | "medium") => {
                if actor_role == "manager" && risk == "medium" {
                    self.allowed_manager_medium.store(true, Ordering::SeqCst);
                }
                println!(
                    "[guardrail] allow tool={} actor_role={} risk={}",
                    ctx.tool_name, actor_role, risk
                );
                ToolInputGuardrailAction::Allow
            }
            ("security_officer", "low" | "medium" | "high") => {
                println!(
                    "[guardrail] allow tool={} actor_role={} risk={}",
                    ctx.tool_name, actor_role, risk
                );
                ToolInputGuardrailAction::Allow
            }
            _ => {
                self.rejected_high_risk.store(true, Ordering::SeqCst);
                println!(
                    "[guardrail] reject tool={} actor_role={} risk={}",
                    ctx.tool_name, actor_role, risk
                );
                ToolInputGuardrailAction::Reject(format!(
                    "policy denied: actor_role={actor_role} cannot perform risk={risk}"
                ))
            }
        }
    }
}

struct ChangeAccountLimitTool {
    executed: Arc<AtomicBool>,
}

#[async_trait]
impl Tool for ChangeAccountLimitTool {
    fn name(&self) -> &str {
        "change_account_limit"
    }

    fn description(&self) -> &str {
        "Change a customer account limit."
    }

    fn input_schema(&self) -> &JsonSchema {
        &serde_json::Value::Null
    }

    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &ToolMetadata {
            side_effect: true,
            approval: Approval::WhenRisky,
            execution_mode: ToolExecutionMode::Normal,
            parallelism: ToolParallelism::Serial,
            cost_hint: None,
            timeout: None,
            max_output_tokens: None,
            source: ToolSource::InProcess,
        }
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError> {
        self.executed.store(true, Ordering::SeqCst);
        println!("[tool] change_account_limit executed with input={input}");
        Ok(ToolOutput::Immediate(json!({"changed": true})))
    }
}

#[tokio::main]
async fn main() {
    let rejected_high_risk = Arc::new(AtomicBool::new(false));
    let allowed_manager_medium = Arc::new(AtomicBool::new(false));
    let executed = Arc::new(AtomicBool::new(false));

    let config = AgentConfig::builder("mock/mock")
        .system_prompt("You are an account operations assistant.")
        .max_steps(5)
        .build()
        .unwrap()
        .with_tool_input_guardrail(Arc::new(AuthorityPolicyGuardrail {
            rejected_high_risk: Arc::clone(&rejected_high_risk),
            allowed_manager_medium: Arc::clone(&allowed_manager_medium),
        }));

    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::new(ChangeAccountLimitTool {
            executed: Arc::clone(&executed),
        }))
        .unwrap();

    let (handle, mut rx) = AgentRun::start(
        config,
        "Review account limit changes.".into(),
        Arc::new(PolicyModel::new()),
        registry,
    );

    let mut saw_approval = false;
    while let Some(event) = rx.recv().await {
        match &event {
            RuntimeEvent::ApprovalRequested { tool_call, context } => {
                println!(
                    "[approval] requested for tool={} context={context:?}",
                    tool_call.name
                );
                saw_approval = true;
                handle.respond_approval(handle.run_id, true).await.unwrap();
            }
            RuntimeEvent::RunCompleted { output, .. } => println!("[run] completed: {output}"),
            RuntimeEvent::RunFailed { error, .. } => println!("[run] failed: {error}"),
            _ => {}
        }
    }
    handle.wait().await;

    assert!(rejected_high_risk.load(Ordering::SeqCst));
    assert!(allowed_manager_medium.load(Ordering::SeqCst));
    assert!(saw_approval);
    assert!(executed.load(Ordering::SeqCst));
    println!("Done. App-layer guardrail handled policy; approval gated the allowed change.");
}
