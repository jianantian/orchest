# 004 · Static error payload cleanup

## Background

Code review noted repeated static `json!` payloads in error/skip paths. The impact is small but the cleanup is straightforward.

## Goal

Remove simple repeated allocations for static tool-result payloads where it does not obscure code.

## Acceptance Criteria

- [x] Static error/skip payload helpers are introduced only where they reduce duplication.
- [x] Model-facing payload shape preserves the post-v0.9.4 structured error contract.
- [x] Tests that assert payload shape continue to pass.
- [x] No unrelated run-loop refactor is included.
- [x] Cleanup does not reintroduce string-only error payloads.
