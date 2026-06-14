# 004 · Static error payload cleanup

## Background

Code review noted repeated static `json!` payloads in error/skip paths. The impact is small but the cleanup is straightforward.

## Goal

Remove simple repeated allocations for static tool-result payloads where it does not obscure code.

## Acceptance Criteria

- [ ] Static error/skip payload helpers are introduced only where they reduce duplication.
- [ ] Model-facing payload shape is unchanged.
- [ ] Tests that assert payload shape continue to pass.
- [ ] No unrelated run-loop refactor is included.
