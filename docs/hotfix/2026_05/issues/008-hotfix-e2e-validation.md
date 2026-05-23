# 008 · Hotfix Regression Validation

## Background

The current test suite passes but misses several contract gaps because many tests exercise core internals directly or only check that events are printed. Hotfix completion needs a validation layer that proves public entrypoints and negative boundaries.

## Goal

Add regression tests and validation commands that prevent the hotfix issues from reappearing silently.

## Acceptance Criteria

**Core tests:**
- [x] Test root `allowed_tools` visibility and execution denial
- [x] Test `allowed_skills` filtering during skill registration
- [x] Test tool execution timeout for an in-process tool
- [x] Test `max_output_tokens` truncation
- [x] Test exact `max_tool_calls` boundary
- [x] Test `SkillMissingCapabilities` emission from normal skill loading
- [x] Test malformed Anthropic SSE JSON fails
- [x] Test OpenAI full tool loop request mapping
- [x] Test sub-agent approval grant and denial routing
- [x] Test MCP HTTP `tools/call` is not retried by default

**SDK tests or demos:**
- [x] Python SDK test proves `agent.run()` streams events before run completion
- [x] Python SDK test proves a registered handler return value appears in `ToolCallCompleted`
- [x] TypeScript SDK test proves a JS handler return value appears in `tool_call_completed`
- [x] TypeScript SDK test proves `for await` event streaming works
- [x] SDK approval tests cover active run handle behavior

**Validation commands:**
- [x] `cargo test --workspace`
- [x] `cargo clippy --workspace -- -D warnings`
- [x] `cargo fmt --check`
- [x] `./scripts/check-ts-event-wire-naming.sh`
- [x] Python demo smoke test with local mock provider
- [x] TypeScript demo smoke test with local mock provider

**CI integration:**
- [x] All validation commands above run in CI (GitHub Actions or equivalent), not only locally
- [x] CI failure blocks merge for hotfix branches

**Documentation:**
- [x] Update the relevant iteration issue checklists or add a hotfix completion note linking back to these issues
- [x] Document any intentionally deferred behavior in this hotfix PRD before closing the hotfix
- [x] If an existing public API changes, update examples and type stubs in the same change

## Notes

Treat this issue as the final gate. It should be closed only after the previous hotfix issues are implemented and verified through public entrypoints.
