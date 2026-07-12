# 003 · Public re-export cleanup + v1.0 migration note + binding sync

Parent: [ADR-0002](../../../adr/0002-protocol-provider-decoupling.md) Phase 3 · [v0.12 PRD](../prd.md)

## Background

Issues 001–002 remove the legacy factory layer and the redundant adapter structs. The
`orchest-provider-http` `lib.rs` still re-exports the now-deleted types (`AnthropicAdapter`,
`OpenAiConfig`, `MinimaxConfig`, `OpenRouterConfig`, `VolcengineConfig`, etc.). The public
surface and any downstream that named these types need to be reconciled, and a v1.0-oriented
migration note written, since v1.0 freezes whatever surface v0.12 leaves.

## Goal

Clean up the public re-export surface to reflect the post-Phase-3 model, confirm the wall and
consumer crates name only protocol + wall, and ship a migration note for anyone who referenced
the removed types.

## Acceptance Criteria

- [ ] `lib.rs` re-exports of removed adapter/config/factory types deleted; remaining exports
      reflect the entry + protocol-factory surface.
- [ ] The wall (`orchest-provider`) public surface and consumers (`orchest`, `orchest-node`,
      `orchest-py`) name only protocol + wall — no impl types leak (grep/audit).
- [ ] `orchest-node` / `orchest-py` build and test green; any reference to a removed type
      updated (expected: none, since bindings go through the wall, but verified).
- [ ] A v1.0 migration note (CHANGELOG or `docs/`) documents: what was removed, and how a
      downstream that constructed via `ProviderFactory` / a `*Adapter` now constructs via the
      free functions / entry surface.
- [ ] `cargo test --workspace` / clippy `-D warnings` / fmt / `scripts/lint-check.sh` pass.

## Notes

This is the issue that makes the Phase 3 surface the one v1.0 will freeze, so the migration
note matters even if the current downstream set is just this workspace. Keep it factual:
removed symbols and their replacement construction path. No behavior changes here — purely
surface and docs.
