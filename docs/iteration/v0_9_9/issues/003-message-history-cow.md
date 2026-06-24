# 003 · Message history CoW evaluation

## Background

The run loop clones message history per model call and retry. Copy-on-write may reduce allocations, but it changes ownership structure.

## Goal

Evaluate message-history clone cost and implement CoW only if justified.

## Acceptance Criteria

- [x] A focused benchmark or test captures message-history clone behavior.
- [x] The benchmark records message count, content size, clone count and allocation impact for model call, retry and handoff paths.
- [x] If implemented, CoW preserves message ordering and hook semantics.
- [x] If rejected, the issue notes include profiling evidence and rationale.
- [x] Any public API change updates examples and tests in the same issue.

## Notes

CoW is rejected for v0.9.9. The focused test
`message_history_clone_profile_records_model_retry_and_handoff_paths` records the current clone
shape without changing runtime behavior.

Representative profile from `cargo test -p agent-runtime-core
message_history_clone_profile_records_model_retry_and_handoff_paths -- --nocapture`:

| Path | Messages | Content bytes | Clone count | Estimated payload bytes |
|------|----------|---------------|-------------|-------------------------|
| Model call, no retry | 64 | 65,536 | 2 | 131,072 |
| Model call, one retry | 64 | 65,536 | 3 | 196,608 |
| Handoff, no input filter | 64 | 65,536 | 1 | 65,536 |
| Handoff, with input filter | 64 | 65,536 | 2 | 131,072 |

Rationale: the measurable allocation impact is bounded to a few history clones per call path, while
true CoW would require changing owned `Vec<Message>` surfaces in `ModelHookContext`,
`RunHookContext`, `ToolContext::parent_messages`, and `HandoffInputData`. That would be a public API
and hook-semantics change without enough profiling evidence in this iteration.
