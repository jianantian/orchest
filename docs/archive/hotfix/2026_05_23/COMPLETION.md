# Hotfix 2026-05 Completion Summary

All 8 hotfix issues have been implemented, tested, and merged to main.

## Issues Resolved

| Issue | Title | PR/Commit |
|-------|-------|-----------|
| #36 (001) | Enforce permission boundaries | `closes #36` |
| #37 (002) | Enforce tool execution metadata | `closes #37` |
| #38 (003) | Wire skill loading into SDK/runtime entrypoints | `closes #38` |
| #39 (004) | Repair Python and TypeScript SDK contracts | `closes #39` |
| #40 (005) | Repair provider tool protocol mappings | `closes #40` |
| #41 (006) | Repair sub-agent routing, events, and budget | `closes #41` |
| #42 (007) | Fix MCP retry and child process timeout | `closes #42` |
| #43 (008) | Hotfix regression validation | `closes #43` |

## Key Changes

### Permission & Safety
- `allowed_tools` / `allowed_skills` properly filter tool visibility and block execution of disallowed tools
- Sub-agent permission inheritance uses intersection (`narrow_permission_list`) so children cannot expand parent restrictions
- MCP HTTP `tools/call` is never retried to prevent duplicate side effects

### Runtime Reliability
- Child processes killed on timeout (BareSubprocessExecutor, JS code execution)
- MCP stdio child killed on client Drop
- Budget guards enforce token, tool call, duration, and cost limits
- `max_output_tokens` truncation prevents oversized tool results

### SDK Contracts
- Python: coroutine detection for async tool handlers, `@agent.tool` decorator, approval routing
- TypeScript: `registerToolWithHandler` with ThreadsafeFunction, approval routing, snake_case event wire format

### Observability
- `ChildRunEvent` wrapper carries `child_run_id` + `run_depth` for all sub-agent events
- Approval routing to child runs via `RunHandle::respond_approval(run_id, ...)`
- `SkillMissingCapabilities` / `SkillDependencyError` / `RuntimeWarning` events

### CI
- GitHub Actions workflow: `cargo test`, `cargo clippy`, `cargo fmt --check`, TS event wire naming check
- All validation commands block merge on failure

## Intentionally Deferred

- **SDK injection criteria**: orchest_sdk availability for skill scripts deferred to skill loading infrastructure (Issue 003 notes)
- **Python/TypeScript demo smoke tests**: require mock provider setup; manual validation performed, automated smoke tests deferred to iteration v0.4
- **SDK streaming tests** (Python `run()` event streaming, TS `for await`): covered by integration patterns in SDK crates; full e2e smoke tests deferred
