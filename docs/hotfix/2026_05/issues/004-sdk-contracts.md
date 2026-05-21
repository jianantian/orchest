# 004 · Repair Python and TypeScript SDK Contracts

## Background

The Python and TypeScript SDKs expose APIs that diverge from the iteration docs:

- Python `run()` returns a synchronous list instead of an async event stream
- Python async job polling calls the returned poll object synchronously and does not await coroutine results
- Python approval response is unsupported in sync mode
- TypeScript `registerTool()` accepts only schema metadata, not a handler
- TypeScript `runSync()` hides the lack of a true event stream

This lets demos print event-shaped output while not validating real SDK behavior.

## Goal

Align SDK APIs with the documented runtime contract and make demos prove real handler execution.

## Acceptance Criteria

**Python SDK:**
- [ ] `agent.run(input)` supports async iteration over runtime events
- [ ] Existing sync collection behavior, if retained, is named distinctly such as `run_sync` and documented as a compatibility helper
- [ ] `@agent.tool` supports plain sync functions
- [ ] `@agent.tool` supports async functions and awaits their result
- [ ] `@agent.tool(requires_approval=True, side_effect=True)` or equivalent metadata registration is supported
- [ ] Python async job poll supports async callables by awaiting coroutine results
- [ ] `respond_approval(run_id, approved)` works for active runs

**TypeScript SDK:**
- [ ] SDK exposes `agent.tool({ name, description, inputSchema/input, handler, ...metadata })` or an equivalent executable handler API
- [ ] The handler receives parsed tool input and can return JSON-serializable output
- [ ] Handler errors become `ToolCallFailed` and tool result errors
- [ ] Async handler results are awaited
- [ ] TypeScript async job return shape is recognized and converted to `JobHandle`
- [ ] `agent.run(input)` supports async iteration over runtime events
- [ ] `respondApproval(runId, approved)` works for active runs

**Demos and types:**
- [ ] Python basic demo output includes the real return value from `get_weather`
- [ ] Python async tool demo output includes at least one progress event from the tool poll function
- [ ] TypeScript basic demo output includes the real return value from a JS handler
- [ ] TypeScript streaming demo uses event streaming rather than collecting only after completion
- [ ] `js/index.ts` and `js/native.d.ts` reflect the implemented API

## Notes

If napi-rs callback plumbing is too large for one issue, split implementation internally, but do not mark this issue complete until TypeScript can execute user-provided tool handlers.
