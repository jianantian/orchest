# Orchest — Rust Project Conventions

> Detailed engineering conventions for the Orchest Rust workspace. `AGENTS.md` points here and keeps only the non-negotiable invariants inline. Read this before writing core/provider/binding code.

## Workspace Structure

```
Cargo.toml                       # workspace root — no business logic here
crates/
  agent-runtime-core/            # pure Rust core: run loop, tools, skills, sessions, guardrails, hooks — no FFI
  agent-runtime-model/           # shared model-layer types: Message, ContentBlock, Role, RequestOptions, ModelAdapter
  agent-runtime-providers/       # LLM provider adapters: anthropic, openai, deepseek, openrouter, volcengine (+ minimax in v0.9.10)
  agent-runtime-aigc-providers/  # image + video generation gateway + asset persistence (+ music submodule in v0.9.10)
  agent-runtime-asr-providers/   # speech-to-text providers (volcengine, aliyun) — duplex streaming
  agent-runtime-tts-providers/   # text-to-speech + voice management providers (volcengine, aliyun; + minimax in v0.9.10)
  agent-runtime-py/              # PyO3 binding — no business logic
  agent-runtime-node/            # napi-rs binding — no business logic
examples/
skills/                          # example skills
```

Each satellite crate keeps its own `src/` layout (e.g. `providers/<vendor>/`, `catalog`, `gateway`, `storage`); see the crate's `lib.rs` for its module map.

**Rule:** Runtime business logic lives in `agent-runtime-core`; shared model types live in `agent-runtime-model`; provider adapters live in their respective satellite crates (each depends on `agent-runtime-model`, not on `core`). Binding crates only do type conversion and FFI glue — no business decisions.

## Dependencies

**Locked core dependencies (do not replace):**

| Crate | Purpose | Features |
|-------|---------|---------|
| `tokio` | Async runtime | core crate uses only required features (`rt`, `rt-multi-thread`, `sync`, `time`, `macros`, `io-util`, `process`, `fs`, `net`); application/example crates may use `full` |
| `serde` + `serde_json` | Serialization | `derive` |
| `async-trait` | Async trait objects | — |
| `uuid` | RunId | `v4`, `serde` |
| `thiserror` | Error types in library crates | — |
| `pyo3` | Python binding | `extension-module` |
| `napi` + `napi-derive` | Node.js binding | — |

The table above is the **core** crate's locked dependency set. Satellite provider crates carry their own provider-specific deps (`reqwest`, `tokio-tungstenite`, `futures-util`, and crypto/`base64`/`hex` for signing and audio decoding), gated behind per-provider feature flags where optional (`agent-runtime-core` stays dependency-light).

**Policy for adding new dependencies:**
- Prefer std + tokio; do not introduce actor frameworks (locked decision)
- Use `thiserror` in library crates; `anyhow` is for application binaries, not SDKs
- Justify every new dependency in the PR or commit body: what it does, what alternatives were considered

## Error Handling

- **Each module defines its own `XxxError`** using `thiserror` derive: `ToolError`, `ModelError`, `SkillError`, `BudgetError`
- **`unwrap()` and `expect()` are banned in library code** except inside `#[cfg(test)]` blocks or where an invariant is explicitly documented in a comment
- At FFI boundaries (PyO3/napi), convert internal errors to the target language's exception/Error type — do not leak Rust error types

## Traits and Visibility

- `pub trait` is only for the public API surface (`Tool`, `ModelAdapter`, `ScriptExecutor`); internal extension points use `pub(crate) trait`
- Implementation types default to `pub(crate)`; only types that need to be constructed in binding crates are `pub`
- Do not blanket re-export with `pub use *` — explicitly name what is exported

## Async Conventions

- The run loop runs on a `tokio::spawn` task; the event channel uses `tokio::sync::mpsc`; the approval gate uses `tokio::sync::oneshot`
- **Use `async-trait` for trait methods** — do not use `-> impl Future` (incompatible with PyO3/napi FFI)
- Wrap blocking operations (file I/O, subprocess spawning) in `tokio::task::spawn_blocking`; do not block inside an async context

## Serialization

- Types that cross the FFI boundary must implement `Serialize + Deserialize`
- `JobHandle.poll` is a closure and **cannot be serialized** — skip it with `#[serde(skip)]` and document in a comment that async job state is lost on cross-process restore
- `JsonSchema` is a type alias for `serde_json::Value` in v0.1; do not introduce a jsonschema crate yet

## Unsafe Policy

- **`agent-runtime-core` must contain no `unsafe` code**
- Binding crates (`agent-runtime-py`, `agent-runtime-node`) may use `unsafe` for FFI, but every `unsafe` block must:
  - Have a comment explaining the safety invariant
  - Contain only type conversion — no business logic inside `unsafe`

## Testing

- **Unit tests**: `#[cfg(test)]` module at the bottom of the relevant file; use plain structs implementing the trait for fakes (no mockall or similar frameworks)
- **Integration tests**: `tests/` at the workspace root, one file per scenario, named after the scenario (`tool_async_job.rs`, `skill_loading.rs`)
- **Test helpers** are named with a `Fake` prefix: `FakeModelAdapter`, `FakeScriptExecutor`; place them in a `#[cfg(test)]` module or `tests/helpers/`
- CI must pass: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`

## Python / PyO3 Build Verification

- `agent-runtime-py` is a PyO3 `extension-module` crate. On macOS, `cargo build -p agent-runtime-py` may fail at link time with missing Python symbols; do **not** treat that command as the authoritative Python binding build check.
- Use `maturin develop` or `maturin build` from the workspace root to verify the Python extension package. If `maturin` is not installed globally, `uvx maturin develop` is the preferred local command.
- After `maturin develop`, verify Python package behavior with the project virtualenv, for example: `.venv/bin/python -m pytest python/tests/test_run_sync.py -v`.
- Rust workspace checks still use `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check`; those commands exercise the PyO3 crate in test/check mode without replacing the `maturin` packaging verification.

## Naming Conventions

| Context | Convention | Examples |
|---------|-----------|---------|
| Types / traits | `PascalCase` | `ToolMetadata`, `ModelAdapter` |
| Methods / variables | `snake_case` | `execute()`, `run_id` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_POLL_RETRIES` |
| Module files | `snake_case` | `async_job.rs`, `skill_bundled.rs` |
| Error types | `XxxError` suffix | `ToolError`, `ModelError` |
| Test fakes | `FakeXxx` prefix | `FakeModelAdapter` |
| Feature flags | `kebab-case` | `mcp`, `openai` |

## Code Organization

- Each file focuses on one primary type or trait; consider splitting if a file exceeds ~400 lines
- `mod.rs` only re-exports and declares submodules — keep logic in the subfiles
- The core state machine of the run loop belongs in `run.rs`; do not scatter loop logic across tool/model modules
