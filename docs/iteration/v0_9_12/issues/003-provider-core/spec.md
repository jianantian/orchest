# Issue 003: orchest-provider-core extraction

## Background

Four provider crates each carry their own `http.rs`, `observability`/`telemetry`, SSE handling, and
scattered header-auth. This issue extracts one shared stack into `orchest-provider-core` — L0 building
blocks + L1 header-auth strategies + cross-cutting services — feature-gated by dependency weight so light
consumers stay light. Behavior-preserving.

## Goal / Scope

Stand up `orchest-provider-core` and move the duplicated infra into it.

In scope:

- **L0:** HTTP client builder, SSE helper, bidirectional WS scaffold, binary-frame codec, OSS upload,
  gen-task poller, retry, telemetry, catalog storage.
- **L1:** header-injection auth strategies (`Bearer`, `X-Api-*` header sets, `AK/SK-HMAC`, `OSS-signature`).
- Weight feature gates (`ws`, `oss`, `sse`); depends on `orchest-protocol`.

Out of scope:

- No provider/dialect impls moved yet (Issues 005–007).
- No registry (Issue 004).

## Acceptance Criteria

- [ ] `orchest-provider-core` exists, depends only on `orchest-protocol` + infra crates.
- [ ] The duplicated `http.rs` / `observability.rs` (asr 259 / tts 239 LOC) / `sse` / `telemetry` are
      collapsed into one implementation; old per-crate copies are removed or re-export core.
- [ ] L1 header-auth strategies cover `Bearer` + the Volcengine `X-Api-*` sets + HMAC/OSS.
- [ ] Default features compile **without** `tokio-tungstenite`/OSS; `ws`/`oss` gate the heavy deps.
- [ ] Existing provider tests stay green (via core or a temporary re-export).

## Notes

Behavior-preserving extraction. Verify with `cargo tree` that the default feature set is light. Depends on Issue 002.
