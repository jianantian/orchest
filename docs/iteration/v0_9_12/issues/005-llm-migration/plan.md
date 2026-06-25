# Issue 005 Plan: LLM migration + dependency inversion

## Files to Read

- `docs/iteration/v0_9_12/prd.md` (§From Here To There, Phase 3)
- `crates/agent-runtime-providers/src/providers/{anthropic,openai,deepseek,openrouter,volcengine,minimax}/`
- `crates/agent-runtime-providers/src/lib.rs` (`create_adapter_from_config`, `normalize_provider_model`)
- `crates/agent-runtime-core/src/model/mod.rs`, `crates/agent-runtime-core/Cargo.toml`
- `crates/agent-runtime-node/src/lib.rs:35,603`, `crates/agent-runtime-py/src/lib.rs:36,513`

## Files to Change

- `crates/orchest-provider-http/src/`: openai-compat (openai/deepseek/openrouter/volc-ark), anthropic, minimax-rest dialect modules as `ChatModel`
- `crates/agent-runtime-providers/`: reduce to deprecated re-export preserving the two free functions + `ModelAdapter` path
- `crates/agent-runtime-core/Cargo.toml` + `src/model/mod.rs`: depend on `orchest-protocol`, keep alias
- Wall registration (Issue 004)

## Steps

1. Create the openai-compat dialect module in `orchest-provider-http`; port openai/deepseek/openrouter/volc-ark onto it as `ChatModel`.
2. Port anthropic and minimax-rest dialects.
3. Register all LLM entries through the wall.
4. Reduce `agent-runtime-providers` to a deprecated re-export; keep `create_adapter_from_config`/`normalize_provider_model`/`ModelAdapter` resolving.
5. Flip `agent-runtime-core` to `orchest-protocol`; keep `core::model::ModelAdapter` alias.
6. Build node/py; run their tests unchanged.
7. Run the existing LLM provider test suites (characterization); fmt/clippy/test workspace.
