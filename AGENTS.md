# Orchest — Agent Working Guide

## What This Is

Orchest is a **low-level Rust SDK** that provides the agent runtime core for building AI agent applications. It is not a complete agent product — it is the engine that other agent products run on: responsible for the agent loop, state management, event streaming, tool dispatch, and skill loading.

Current stage: **pre-1.0, actively implemented**. The Rust core (`agent-runtime-core`) and the shared model crate (`agent-runtime-model`) are built out, alongside satellite provider crates for LLM, image/video (AIGC), ASR, and TTS. Design docs under `docs/` remain the authoritative implementation contract; code lives in `crates/`, `examples/`, and `skills/` per the Rust Project Conventions below. The living roadmap is [`docs/iteration/roadmap.md`](./docs/iteration/roadmap.md).

---

## Terminology Boundaries (Most Important — Never Conflate)

These three concepts are frequently confused in the Anthropic ecosystem. Orchest enforces strict separation:

| Concept | Definition | Layer |
|---------|-----------|-------|
| **Tool** | The smallest atomic capability unit the model can invoke — contains name, schema, and an `execute` implementation | Capability layer |
| **MCP** | The transport protocol for tool providers — not a special tool type or replacement for tools | Protocol layer |
| **Skill** | A filesystem-organized procedural knowledge package (SKILL.md + optional scripts), exposed to the agent via progressive disclosure | Knowledge layer |

**MCP is not a replacement for tools.** It is the protocol layer that decouples tool discovery and execution from application code. Tools arriving via MCP and tools registered directly as in-process tools are treated identically inside the runtime through the same `Tool` trait.

**A Skill's core is not a collection of tools.** It is how-to knowledge. Many skills consist entirely of Markdown and call tools that already exist in the current session.

For any question about these boundaries, defer to `docs/polaris/concept-boundaries.md`.

---

## Document Map

```
docs/
├── polaris/                    # Cross-iteration constraints (authoritative): overview, concept-boundaries,
│                               #   design-principles, observability, non-goals
├── iteration/
│   ├── roadmap.md             # Living iteration roadmap + dependency graph + capability-gap map (START HERE)
│   ├── v0_9_6/ … v0_9_10/     # Active/planned iterations — each: prd.md + issues/NNN-slug/{spec,plan}.md
│   └── v0_10/                 # Demo Product Validation (Briefing Desk)
├── hotfix/                     # Refactor/repair iterations (e.g. 2026_06_17)
├── archive/                    # Completed iterations + hotfixes (v0_1 … v0_9), moved here after closeout
├── todo/                       # Forward-looking direction notes not yet scheduled (e.g. provider-unification.md)
├── external/                   # Upstream vendor API docs (anthropic, minimax, volceengine, aliyun, …)
├── research/ · analysis/       # Architecture research and competitive analysis
└── guide/                      # User-facing quickstart + Python/TS SDK guides
```

### Authority Rules (Important)

**Iteration, hotfix, and polaris docs are authoritative.** The old root technical design document was removed because it was superseded and no longer represented the current implementation contract. When there is a conflict, the `docs/iteration/`, `docs/hotfix/`, and `docs/polaris/` files are authoritative.

Each iteration has two layers:
- `prd.md` — iteration goals, success metrics, scope, and explicit out-of-scope items
- `issues/NNN-slug/` — one directory per issue, holding `spec.md` (acceptance criteria) and `plan.md` (implementation steps). Hotfix issues may embed the plan in `spec.md`. See [WORKFLOW.md](./WORKFLOW.md).

The living iteration index is [`docs/iteration/roadmap.md`](./docs/iteration/roadmap.md) — consult it for current status rather than this file.

---

## Iteration Status

The authoritative, up-to-date status lives in [`docs/iteration/roadmap.md`](./docs/iteration/roadmap.md) (已完成 / 规划中 tables + dependency graph). Do not duplicate it here.

Snapshot (2026-06): v0.1–v0.9.5 shipped; satellite provider crates landed (ASR v0.9.1, TTS v0.9.3, Image/Video AIGC v0.6.1); v0.9.6–v0.9.9 runtime/satellite iterations planned; **v0.9.10 Minimax multimodal provider integration** in progress; then v0.10 Demo Product Validation → v1.0 first public release.

---

## Locked Design Decisions (Do Not Re-litigate)

The following decisions are settled. Do not propose alternatives without a compelling new argument:

- **Rust core + PyO3/napi-rs** — cross-language SDKs require in-process embedding, not IPC; Rust is the only viable choice
- **Skill-first** — full alignment with the Anthropic Agent Skills open standard; SKILL.md format must remain compatible with the official spec
- **MCP is a transport protocol, not a tool type** — tools arriving via MCP are handled through the same `Tool` trait as in-process tools
- **Minimal core** — the runtime only handles "loop + state management + event stream"; all capabilities live in tools and skills
- **Streaming output is a first-class concern** — not optional; model adapters use the unified `ModelAdapter::complete()` contract with streaming events delivered through the optional event channel, and `stream_chat()` is the convenience helper
- **Sequential tool execution in v0.1** — keeps the approval gate simple; parallelism is a v0.2 optimization
- **No sandbox until v0.3+** — but v0.3 must complete the `ScriptExecutor` trait abstraction and `capabilities` declaration
- **Provider crates are independent satellites** — each modality (LLM, AIGC image/video, ASR, TTS) is its own crate depending on `agent-runtime-model`, not on `agent-runtime-core`. Cross-modality consolidation (driven by omni / end-to-end speech models, and Chameleon-style image-out LLMs) is a **known future direction, not yet decided** — tracked in [`docs/todo/provider-unification.md`](./docs/todo/provider-unification.md). Do not merge provider crates ahead of that refactor, and do not assume the current single-modality split is permanent.

---

## Documentation Change Conventions

### Adding an Issue

1. Create a directory `issues/NNN-slug/` under the relevant iteration (three-digit numeric prefix), holding `spec.md` + `plan.md` (hotfix issues may embed the plan in `spec.md`). See [WORKFLOW.md](./WORKFLOW.md).
2. `spec.md` must include: Background, Goal/scope, Acceptance Criteria (checkbox list), Notes (optional). `plan.md` lists files to read, files to change, and numbered implementation steps.
3. Acceptance criteria must be concrete and testable — write "when Y, Z holds" not just "implement X"

### Editing Issue Specs

When changing behavior, API shape, or acceptance criteria:
- Update the corresponding iteration or hotfix issue spec / PRD first
- Keep issue specs concrete and testable
- Do not introduce a new root-level replacement for the removed technical design document

### Editing Polaris Docs

Polaris documents record **constraints that do not change across iterations**. Edit with care — confirm this is a permanent boundary, not a current-iteration tradeoff, before modifying.

### Things to Avoid

- Do not make authoritative changes to spec.md without updating the corresponding issues
- Do not add implementation details to `docs/polaris/overview.md` (it is a high-level reference, not an implementation contract)
- Do not bring multi-channel routing, user management, or Web UI concerns into SDK design
- Do not add a Non-Goal without grounding it in an existing polaris rationale

---

## Rust Project Conventions

### Workspace Structure

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

### Dependencies

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

### Error Handling

- **Each module defines its own `XxxError`** using `thiserror` derive: `ToolError`, `ModelError`, `SkillError`, `BudgetError`
- **`unwrap()` and `expect()` are banned in library code** except inside `#[cfg(test)]` blocks or where an invariant is explicitly documented in a comment
- At FFI boundaries (PyO3/napi), convert internal errors to the target language's exception/Error type — do not leak Rust error types

### Traits and Visibility

- `pub trait` is only for the public API surface (`Tool`, `ModelAdapter`, `ScriptExecutor`); internal extension points use `pub(crate) trait`
- Implementation types default to `pub(crate)`; only types that need to be constructed in binding crates are `pub`
- Do not blanket re-export with `pub use *` — explicitly name what is exported

### Async Conventions

- The run loop runs on a `tokio::spawn` task; the event channel uses `tokio::sync::mpsc`; the approval gate uses `tokio::sync::oneshot`
- **Use `async-trait` for trait methods** — do not use `-> impl Future` (incompatible with PyO3/napi FFI)
- Wrap blocking operations (file I/O, subprocess spawning) in `tokio::task::spawn_blocking`; do not block inside an async context

### Serialization

- Types that cross the FFI boundary must implement `Serialize + Deserialize`
- `JobHandle.poll` is a closure and **cannot be serialized** — skip it with `#[serde(skip)]` and document in a comment that async job state is lost on cross-process restore
- `JsonSchema` is a type alias for `serde_json::Value` in v0.1; do not introduce a jsonschema crate yet

### Unsafe Policy

- **`agent-runtime-core` must contain no `unsafe` code**
- Binding crates (`agent-runtime-py`, `agent-runtime-node`) may use `unsafe` for FFI, but every `unsafe` block must:
  - Have a comment explaining the safety invariant
  - Contain only type conversion — no business logic inside `unsafe`

### Testing

- **Unit tests**: `#[cfg(test)]` module at the bottom of the relevant file; use plain structs implementing the trait for fakes (no mockall or similar frameworks)
- **Integration tests**: `tests/` at the workspace root, one file per scenario, named after the scenario (`tool_async_job.rs`, `skill_loading.rs`)
- **Test helpers** are named with a `Fake` prefix: `FakeModelAdapter`, `FakeScriptExecutor`; place them in a `#[cfg(test)]` module or `tests/helpers/`
- CI must pass: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`

### Python / PyO3 Build Verification

- `agent-runtime-py` is a PyO3 `extension-module` crate. On macOS, `cargo build -p agent-runtime-py` may fail at link time with missing Python symbols; do **not** treat that command as the authoritative Python binding build check.
- Use `maturin develop` or `maturin build` from the workspace root to verify the Python extension package. If `maturin` is not installed globally, `uvx maturin develop` is the preferred local command.
- After `maturin develop`, verify Python package behavior with the project virtualenv, for example: `.venv/bin/python -m pytest python/tests/test_run_sync.py -v`.
- Rust workspace checks still use `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check`; those commands exercise the PyO3 crate in test/check mode without replacing the `maturin` packaging verification.

### Naming Conventions

| Context | Convention | Examples |
|---------|-----------|---------|
| Types / traits | `PascalCase` | `ToolMetadata`, `ModelAdapter` |
| Methods / variables | `snake_case` | `execute()`, `run_id` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_POLL_RETRIES` |
| Module files | `snake_case` | `async_job.rs`, `skill_bundled.rs` |
| Error types | `XxxError` suffix | `ToolError`, `ModelError` |
| Test fakes | `FakeXxx` prefix | `FakeModelAdapter` |
| Feature flags | `kebab-case` | `mcp`, `openai` |

### Code Organization

- Each file focuses on one primary type or trait; consider splitting if a file exceeds ~400 lines
- `mod.rs` only re-exports and declares submodules — keep logic in the subfiles
- The core state machine of the run loop belongs in `run.rs`; do not scatter loop logic across tool/model modules

---

## Development Workflow

See [WORKFLOW.md](./WORKFLOW.md) for the full development workflow: one branch + worktree per iteration, GitHub issues first, one commit per issue (`closes #N`), pre-merge checks, and per-iteration dependency order.

---

## Commit Conventions

Prefix: `docs:` (documentation), `feat:` (feature, during implementation), `fix:` (bug fix), `refactor:` (refactoring)

Subject examples:
- `docs: add v0.2 issue for webhook async tool`
- `docs: clarify ScriptExecutor trait in spec`
- `feat: implement Tool trait and ToolRegistry`

Keep subjects under 72 characters. Use the body to explain why and what downstream files were also updated.

---

## Useful Search Commands

```bash
# Search the design corpus for a term or type name
rg "term" docs/

# List all documentation files
find docs -maxdepth 4 -name "*.md" | sort

# Show all unchecked acceptance criteria across iterations
rg "\- \[ \]" docs/iteration/

# Current iteration status (authoritative)
sed -n '/## 已完成/,/## 能力缺口/p' docs/iteration/roadmap.md
```
