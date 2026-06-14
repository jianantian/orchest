# 001 · Draft/Commit metadata

## Background

High-risk actions often need a preview step before execution. The runtime currently only has per-call approval modes.

## Goal

Add metadata that links draft tools to commit tools.

## Acceptance Criteria

- [ ] Tool metadata can express draft mode or an associated commit tool.
- [ ] Metadata serialization remains stable across Rust/Python/Node boundaries.
- [ ] Tool registration validates obvious invalid draft/commit relationships.
- [ ] Tests cover metadata defaults and invalid links.
