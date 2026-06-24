# 003 · Python GIL behavior

## Background

The Python binding's GIL behavior must be accurate in docs and safe for users running long tool calls.

## Goal

Audit, document and improve Python GIL behavior where needed.

## Acceptance Criteria

- [x] Current `py.detach()` usage is documented accurately.
- [x] Python tool execution behavior is documented separately from run-loop execution.
- [x] Tests or examples demonstrate that long runs do not unnecessarily hold the GIL.
- [x] Any real GIL contention found in run-loop or Python callback execution is fixed in this iteration unless it is caused by user-provided Python code intentionally holding the GIL.
- [x] Python examples/tests are updated when behavior or guidance changes.
