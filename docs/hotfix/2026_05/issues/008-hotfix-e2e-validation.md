# 008 · Hotfix Regression Validation

## Background

The current test suite passes but misses several contract gaps because many tests exercise core internals directly or only check that events are printed. Hotfix completion needs a validation layer that proves public entrypoints and negative boundaries.

## Goal

Add regression tests and validation commands that prevent the hotfix issues from reappearing silently.

## Acceptance Criteria

**Core tests:**
- [ ] Test root `allowed_tools` visibility and execution denial
- [ ] Test `allowed_skills` filtering during skill registration
- [ ] Test tool execution timeout for an in-process tool
- [ ] Test `max_output_tokens` truncation
- [ ] Test exact `max_tool_calls` boundary
- [ ] Test `SkillMissingCapabilities` emission from normal skill loading
- [ ] Test malformed Anthropic SSE JSON fails
- [ ] Test OpenAI full tool loop request mapping
- [ ] Test sub-agent approval grant and denial routing
- [ ] Test MCP HTTP `tools/call` is not retried by default

**SDK tests or demos:**
- [ ] Python SDK test proves `agent.run()` streams events before run completion
- [ ] Python SDK test proves a registered handler return value appears in `ToolCallCompleted`
- [ ] TypeScript SDK test proves a JS handler return value appears in `tool_call_completed`
- [ ] TypeScript SDK test proves `for await` event streaming works
- [ ] SDK approval tests cover active run handle behavior

**Validation commands:**
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] `./scripts/check-ts-event-wire-naming.sh`
- [ ] Python demo smoke test with local mock provider
- [ ] TypeScript demo smoke test with local mock provider

**Documentation:**
- [ ] Update the relevant iteration issue checklists or add a hotfix completion note linking back to these issues
- [ ] Document any intentionally deferred behavior in this hotfix PRD before closing the hotfix
- [ ] If an existing public API changes, update examples and type stubs in the same change

## Notes

Treat this issue as the final gate. It should be closed only after the previous hotfix issues are implemented and verified through public entrypoints.
