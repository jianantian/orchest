# 005 · Repair Provider Tool Protocol Mappings

## Background

The provider adapters normalize provider responses into core `ContentBlock`s. That direction is only half of the contract: adapters must also serialize core message history back into provider-specific request formats.

The OpenAI adapter currently serializes `ToolResult` as ordinary text and drops assistant `ToolUse` content, which breaks Chat Completions tool-call continuation. The Anthropic adapter silently converts malformed SSE JSON into empty objects, which can produce successful but incorrect empty responses.

## Goal

Make Anthropic and OpenAI adapters fail clearly and preserve tool-call history across at least one full tool loop.

## Acceptance Criteria

**OpenAI request mapping:**
- [ ] Assistant messages containing `ContentBlock::ToolUse` serialize to OpenAI assistant messages with `tool_calls`
- [ ] Tool results serialize to OpenAI `role: "tool"` messages with the matching `tool_call_id`
- [ ] Mixed assistant text + tool calls are serialized according to OpenAI Chat Completions expectations
- [ ] System/user text messages continue to serialize correctly

**OpenAI response mapping:**
- [ ] Streaming tool call chunks continue to normalize into `ContentBlock::ToolUse`
- [ ] Invalid function argument JSON returns `ModelError { code: Some("invalid_tool_arguments") }` or an equivalent stable error code, and no tool call is executed for those arguments
- [ ] Usage mapping remains covered

**Anthropic robustness:**
- [ ] Malformed SSE JSON returns `ModelError { code: Some("invalid_json") }` or equivalent, not an empty successful response
- [ ] Tests cover malformed `data:` payloads
- [ ] Existing thinking-boundary and tool-use parsing tests continue to pass

**Cross-provider integration:**
- [ ] A shared adapter smoke test covers: model requests tool -> runtime executes tool -> adapter sends tool result -> model returns final text
- [ ] The smoke test runs for both Anthropic-compatible mock and OpenAI-compatible mock

## Notes

Keep provider-specific branching inside adapters. The run loop should continue to operate only on `Message`, `ContentBlock`, `ModelResponse`, and `ToolDef`.
