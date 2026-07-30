//! Controlled worker fault and the repeated-failure abort policy.

use async_trait::async_trait;
use orchest::{
    hook::{Hook, HookAction, RepeatedFailureHookContext},
    tool::{Approval, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource},
};
use serde_json::{json, Value};

pub const CONTROLLED_FAULT_CODE: &str = "RESEARCH_PIPELINE_CONTROLLED_FAULT";
pub const CONTROLLED_FAULT_ABORT_REASON: &str =
    "controlled worker fault reached repeated-failure threshold";

pub struct FaultTriggerTool {
    metadata: ToolMetadata,
    input_schema: Value,
}

impl FaultTriggerTool {
    pub fn new() -> Self {
        Self {
            metadata: ToolMetadata {
                side_effect: false,
                approval: Approval::Never,
                source: ToolSource::InProcess,
                ..ToolMetadata::default()
            },
            input_schema: json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "optional diagnostic reason for the controlled fault"
                    }
                }
            }),
        }
    }
}

impl Default for FaultTriggerTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FaultTriggerTool {
    fn name(&self) -> &str {
        "fault_trigger"
    }

    fn description(&self) -> &str {
        "Trigger the Research Pipeline's deterministic fatal worker fault."
    }

    fn input_schema(&self) -> &Value {
        &self.input_schema
    }

    fn output_schema(&self) -> Option<&Value> {
        None
    }

    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }

    async fn execute(&self, input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let reason = input
            .get("reason")
            .and_then(Value::as_str)
            .filter(|reason| !reason.trim().is_empty())
            .unwrap_or("requested by deterministic fault scenario");
        Err(
            ToolError::fatal(format!("controlled worker fault: {reason}"))
                .with_code(CONTROLLED_FAULT_CODE),
        )
    }
}

pub struct ControlledFaultAbortHook;

#[async_trait]
impl Hook for ControlledFaultAbortHook {
    async fn on_repeated_failure(&self, context: &RepeatedFailureHookContext) -> HookAction {
        let is_controlled_fault = context.tool_name == "fault_trigger"
            && context.error_kind == orchest::tool::ErrorKind::Fatal
            && context
                .error_history
                .last()
                .and_then(|error| error.code.as_deref())
                == Some(CONTROLLED_FAULT_CODE);
        if is_controlled_fault {
            HookAction::Abort(CONTROLLED_FAULT_ABORT_REASON.to_string())
        } else {
            HookAction::Continue
        }
    }
}
