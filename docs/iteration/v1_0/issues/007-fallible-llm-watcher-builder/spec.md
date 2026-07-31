# 007 · Fallible LLM watcher builder

GitHub issue: #255

## Background

RB-1 shows `LlmWatcherBuilder::build()` uses `expect()` when no model is
configured. The public library can panic on caller configuration error, and
changing the signature after v1.0 would be breaking.

## Goal

Return a typed configuration error for a missing watcher model before the
public API freezes.

## Acceptance Criteria

- [ ] `LlmWatcherBuilder::build()` returns `Result<LlmWatcher, ConfigError>`.
- [ ] A missing model returns a dedicated, asserted error variant without
  panic.
- [ ] Valid builder call sites use `?` or explicitly handle the result.
- [ ] Rust examples and bindings compile against the fallible signature.
- [ ] No new `unwrap()` or `expect()` is added to library code.

## Notes

This is a v1.0 release blocker independent of Multivac M2.
