# Issue 003 Plan: orchest-provider-core extraction

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§2 layers L0/L1)
- `crates/agent-runtime-providers/src/{http.rs,sse/mod.rs,telemetry.rs}`
- `crates/agent-runtime-aigc-providers/src/{http.rs,telemetry.rs,storage/oss.rs}`
- `crates/agent-runtime-asr-providers/src/{http.rs,observability.rs}`
- `crates/agent-runtime-tts-providers/src/observability.rs`
- Auth: `crates/agent-runtime-realtime-providers/src/providers/volcengine/realtime/live.rs:250`, `crates/agent-runtime-asr-providers/src/providers/volcengine/mod.rs:207`
- All four provider `Cargo.toml` files (dependency/feature patterns)

## Files to Change

- New crate `orchest-provider-core` (+ workspace member)
- Move infra modules in; add `ws`/`oss`/`sse` feature gates
- Point the old crates' `http`/`observability`/`telemetry`/`sse` at core (re-export) so tests pass
- Workspace `Cargo.toml`

## Steps

1. Create `orchest-provider-core` depending on `orchest-protocol`.
2. Move the HTTP client builder (collapse the 4 `http.rs`).
3. Move the SSE helper (`providers/sse`).
4. Move the bidirectional WS scaffold + binary-frame codec behind a `ws` feature.
5. Unify and move telemetry/observability (collapse the two near-identical `observability.rs`).
6. Move OSS upload + gen-task poller behind an `oss` feature.
7. Add L1 header-auth strategies (`Bearer`, `X-Api-*`, HMAC, OSS-sign).
8. Re-export core from the old crates so existing tests compile and pass.
9. `cargo tree -e features --features <default>` to confirm no `tungstenite`/OSS by default; fmt/clippy/test.
