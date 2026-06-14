# 001 · Draft/Commit metadata

## Background

High-risk actions often need a preview step before execution. The runtime currently only has per-call approval modes.

## Goal

Add metadata that links draft tools to commit tools.

## Acceptance Criteria

- [ ] `ToolMetadata` has `ToolExecutionMode::Normal`, `ToolExecutionMode::Draft { commit_tool }` and `ToolExecutionMode::Commit { draft_tool }`.
- [ ] Draft and commit links use canonical tool names and are validated during registration or registry finalization.
- [ ] Self-links, missing linked tools and ambiguous many-to-one links are rejected with structured errors.
- [ ] Metadata serialization remains stable across Rust/Python/Node boundaries.
- [ ] Rust, Python and Node tool registration paths can set the execution mode.
- [ ] Tests cover metadata defaults, valid links, invalid links and binding serialization.
