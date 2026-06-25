# Issue 004 Plan: Registry + umbrella wall

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§4 consumer API, Decisions 4 & 5)
- `crates/agent-runtime-providers/src/registry.rs` (`ProviderFactory`/`ProviderRegistry`)
- `crates/agent-runtime-asr-providers/src/routing.rs` (`AsrRouter`, `select_for_*`)
- `crates/agent-runtime-tts-providers/src/routing.rs` (parallel router)
- `crates/agent-runtime-node/src/lib.rs:35,603` and `crates/agent-runtime-py/src/lib.rs:36,513` (real call sites)
- `orchest-protocol` descriptor + capability traits (Issue 002 output)

## Files to Change

- New crate `orchest-providers` (+ workspace member)
- Registry (multi-capability, descriptor query), selection builder, vendor facade, feature graph
- Workspace `Cargo.toml`

## Steps

1. Create `orchest-providers` depending on `orchest-protocol` + `orchest-provider-core`.
2. Define the registry entry (capability, provider, model, static descriptor) and the registration mechanism (cfg-gated).
3. Port `AsrRouter.select_for_*` into the capability "pick"; fold `ProviderRegistry` factory-by-name.
4. Build the selection builder (capability filters + identity filter + pick-one/list) against the node/py call sites; record the surface.
5. Build the vendor-namespaced facade module.
6. Wire the feature graph (`volcengine`/`llm`/`asr` → impl-crate features); prove cfg subsets compile.
7. fmt/clippy/test; add registry selection unit tests over descriptor fixtures.
