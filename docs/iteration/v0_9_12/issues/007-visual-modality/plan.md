# Issue 007 Plan: Visual + remaining modality migration

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (Chameleon ruler, pricing risk)
- `crates/agent-runtime-aigc-providers/src/gateway/` (image/video), `src/music/`, `src/storage/`, `src/types/`
- `crates/agent-runtime-aigc-providers/src/providers/{volcengine,aliyun,crazyrouter,renderful,openrouter}/`
- `crates/agent-runtime-model/src/options.rs:110` (`ModelPricing`), `crates/agent-runtime-asr-providers/src/types.rs` (`AsrUsage`)
- `orchest-provider-core` gen-task poller + OSS upload + asset storage (Issue 003 output)

## Files to Change

- New crate `orchest-provider-visual` (+ workspace member, `oss` feature)
- `GenTask` impls for volc-visual/aliyun/crazyrouter/renderful; minimax music → `orchest-provider-http`
- Pricing reconciliation in `orchest-provider-core`
- Delete `crates/agent-runtime-aigc-providers`; remove from workspace
- Wall registration (Issue 004)

## Steps

1. Create `orchest-provider-visual` (signed/poll tier) depending on protocol + core.
2. Abstract `GenTask` from `ImageGateway` + video gateway; implement for volc-visual/aliyun/crazyrouter/renderful.
3. Route minimax music to `orchest-provider-http` (REST/Bearer).
4. Move asset storage/OSS use onto core; wire gen-task poller.
5. Reconcile pricing (token/duration/asset) into one core accounting surface; add tests.
6. Delete `agent-runtime-aigc-providers`; register visual entries through the wall.
7. Confirm the Chameleon ruler (turn emits `Image`, no `GenTask`); fmt/clippy/test.
