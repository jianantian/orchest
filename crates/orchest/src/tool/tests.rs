use super::*;
use serde_json::json;
use std::sync::Arc;

/// A real (closure-backed) SDK tool that echoes its input and records the
/// context fields it was called with.
fn echo_tool() -> Arc<dyn Tool> {
    Arc::new(in_process::InProcessTool::new(
        "echo".to_string(),
        "echoes the input".to_string(),
        JsonSchema::Null,
        None,
        ToolMetadata::default(),
        Arc::new(|input: Value, ctx: ToolContext| {
            Box::pin(async move {
                Ok(ToolOutput::Immediate(json!({
                    "echo": input,
                    "run_depth": ctx.run_depth,
                    "tool_call_id": ctx.tool_call_id,
                })))
            })
        }),
    ))
}

#[tokio::test]
async fn oneshot_context_drives_real_tool_execute() {
    let ctx = ToolContext::oneshot();
    assert_eq!(ctx.run_depth, 0);
    assert!(ctx.event_tx.is_none());
    assert!(ctx.parent_messages.is_empty());
    assert!(ctx.tool_call_id.starts_with("oneshot-"));

    let output = echo_tool()
        .execute(json!({"msg": "hi"}), &ctx)
        .await
        .expect("execute should succeed");
    let ToolOutput::Immediate(value) = output else {
        panic!("expected Immediate output");
    };
    assert_eq!(value["echo"], json!({"msg": "hi"}));
    assert_eq!(value["run_depth"], json!(0));
    // The same synthesized id the caller saw is what the tool received.
    assert_eq!(value["tool_call_id"], json!(ctx.tool_call_id));
}

#[tokio::test]
async fn call_oneshot_constructs_context_internally() {
    let output = echo_tool()
        .call_oneshot(json!({"msg": "hi"}))
        .await
        .expect("call_oneshot should succeed");
    let ToolOutput::Immediate(value) = output else {
        panic!("expected Immediate output");
    };
    assert_eq!(value["echo"], json!({"msg": "hi"}));
    assert!(
        value["tool_call_id"]
            .as_str()
            .unwrap_or("")
            .starts_with("oneshot-"),
        "expected a synthesized tool_call_id, got: {}",
        value["tool_call_id"]
    );
}
