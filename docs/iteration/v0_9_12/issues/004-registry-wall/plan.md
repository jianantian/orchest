# Issue 004 Plan: Registry + umbrella wall

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§4 consumer API, Decisions 4 & 5)
- `crates/agent-runtime-providers/src/registry.rs` (`ProviderFactory`/`ProviderRegistry`)
- `crates/agent-runtime-asr-providers/src/routing.rs` (`AsrRouter`, `select_for_*`)
- `crates/agent-runtime-tts-providers/src/routing.rs` (parallel router)
- node/py call sites — `rg "create_adapter_from_config|normalize_provider_model" crates/agent-runtime-node/src crates/agent-runtime-py/src` (real surface; not fixed line numbers)
- `orchest-protocol` descriptor + capability traits (Issue 002 output)

## Files to Change

- New crate `orchest-providers` (+ workspace member)
- Empty impl-crate skeletons `orchest-provider-{http,stream,visual}` (Cargo.toml + lib stub + feature stubs)
- Registry (multi-capability, descriptor query), selection builder, vendor facade, feature graph
- Workspace `Cargo.toml`

## Steps

1. Create `orchest-providers` (depending on `orchest-protocol` + `orchest-provider-core`) and the empty `orchest-provider-{http,stream,visual}` skeletons (workspace members + feature stubs) for the impl issues to fill.
2. Define the registry entry (capability, provider, model, static descriptor) and the registration mechanism (cfg-gated).
3. Port `AsrRouter.select_for_*` into the capability "pick"; fold `ProviderRegistry` factory-by-name.
4. Build the selection builder (capability filters + identity filter + pick-one/list) against the node/py call sites; record the surface.
5. Build the vendor-namespaced facade module.
6. Wire the feature-graph **scaffold** (empty feature stubs); prove the mechanism-only build + cfg subsets compile. Concrete `volcengine`/`llm`/`asr` rows land in 005/006/007.
7. fmt/clippy/test; add registry selection unit tests over descriptor fixtures.
