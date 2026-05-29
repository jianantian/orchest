# 002 · Enforce Tool Execution Metadata

## Background

`ToolMetadata.timeout` and `ToolMetadata.max_output_tokens` are public contract fields. The current run loop awaits `tool.execute()` directly and forwards full outputs without a shared enforcement layer. Some backends implement timeout locally, but in-process tools, SDK callbacks, and many custom tools can still block indefinitely or emit unbounded output.

## Goal

Make `ToolMetadata` behavior consistent across all tool sources.

## Acceptance Criteria

**Timeout enforcement:**
- [x] Runtime wraps tool execution in `tokio::time::timeout` when `ToolMetadata.timeout` is set
- [x] On timeout, runtime emits `ToolCallFailed { error: "tool execution timed out" }` or an equivalent stable error code
- [x] On timeout, runtime appends a `ToolResult` error so the model can continue the run
- [x] A test covers an in-process tool that sleeps longer than its metadata timeout

**Output truncation:**
- [x] Runtime applies `ToolMetadata.max_output_tokens` to `ToolOutput::Immediate`
- [x] Truncated output includes a clear marker such as `[output truncated]`
- [x] Truncation preserves valid JSON shape where possible; if exact shape cannot be preserved, the behavior is documented and tested
- [x] A test covers oversized string output and oversized structured JSON output

**Async job behavior:**
- [x] Async job submission timeout uses the tool metadata timeout
- [x] Async job wait timeout remains controlled by `JobHandle.timeout`
- [x] Timeout failures do not count as successful `ToolCallCompleted`

**Budget interaction:**
- [x] Tool call budget is checked before dispatch so `max_tool_calls = N` permits at most N executed tool calls
- [x] Denied-by-approval and denied-by-policy tool calls do not consume tool-call budget unless explicitly documented otherwise
- [x] Tests cover the exact boundary at `max_tool_calls`

## Notes

Keep backend-local timeouts if they provide stronger cleanup, but runtime-level enforcement is still required as the cross-source contract.

**Scope boundary with 007:** This issue owns the `tokio::time::timeout` wrapper at the runtime dispatch layer (in-process tools, SDK callbacks). Issue 007 owns subprocess/child-process kill-on-timeout and MCP process lifecycle cleanup. The two layers are complementary — runtime timeout fires first, then 007's cleanup ensures the underlying process is terminated.
