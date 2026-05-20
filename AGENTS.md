# Orchest — Agent Working Guide

## What This Is

Orchest is a **low-level Rust SDK** that provides the agent runtime core for building AI agent applications. It is not a complete agent product — it is the engine that other agent products run on: responsible for the agent loop, state management, event streaming, tool dispatch, and skill loading.

Current stage: **documentation only, no implementation code yet**. All work lives under `docs/`. Once code is introduced, `crates/`, `examples/`, and `skills/` will be organized per the Rust Project Conventions section below.

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
├── overview.md                # Product positioning, core concepts, design philosophy (external-facing)
├── spec.md                    # Original technical design reference (partially superseded — see authority rules)
├── polaris/
│   ├── concept-boundaries.md  # Tool/MCP/Skill boundary definitions (authoritative)
│   ├── design-principles.md   # Design principles and decision heuristics
│   └── non-goals.md           # Hard boundaries + minimum security guidance for sandboxless environments
├── iteration/
│   ├── v0_1/                  # Minimum viable: Rust core + dual-language SDK
│   ├── v0_2/                  # MCP integration + OpenAI adapter + context compaction
│   └── v0_3/                  # Production readiness: skill deps + code exec + sub-agent + sandbox architecture
└── research/
    └── claw-landscape.md      # Architecture research across 7 comparable products (SDK-layer takeaways)
```

### Authority Rules (Important)

**Iteration docs override spec.md.** `docs/spec.md` is the original design; parts of it have been superseded by iteration documents. When there is a conflict, the `docs/iteration/` files are authoritative. `spec.md` is kept as a historical reference — do not make authoritative changes there.

Each iteration has two layers:
- `prd.md` — iteration goals, success metrics, scope, and explicit out-of-scope items
- `issues/*.md` — implementation-ready units with acceptance criteria

---

## Iteration Status

| Iteration | Status | Core Scope |
|-----------|--------|-----------|
| **v0.1** | Docs complete, not yet implemented | Rust core run loop, skill loading, async jobs, budget guard, approval gate, Python/TS SDK |
| **v0.2** | Docs complete, not yet implemented | MCP stdio/HTTP, Tool Search Tool, OpenAI adapter, context compaction, webhook async tool |
| **v0.3** | Docs complete, not yet implemented | Skill dependency management, Code Execution MCP, sub-agent, ScriptExecutor abstraction + capability declaration |

---

## Locked Design Decisions (Do Not Re-litigate)

The following decisions are settled. Do not propose alternatives without a compelling new argument:

- **Rust core + PyO3/napi-rs** — cross-language SDKs require in-process embedding, not IPC; Rust is the only viable choice
- **Skill-first** — full alignment with the Anthropic Agent Skills open standard; SKILL.md format must remain compatible with the official spec
- **MCP is a transport protocol, not a tool type** — tools arriving via MCP are handled through the same `Tool` trait as in-process tools
- **Minimal core** — the runtime only handles "loop + state management + event stream"; all capabilities live in tools and skills
- **Streaming output is a v0.1 first-class concern** — not optional; `ModelAdapter::stream()` is the primary path
- **Sequential tool execution in v0.1** — keeps the approval gate simple; parallelism is a v0.2 optimization
- **No sandbox until v0.3+** — but v0.3 must complete the `ScriptExecutor` trait abstraction and `capabilities` declaration

---

## Documentation Change Conventions

### Adding an Issue

1. Place it under the relevant iteration's `issues/` directory; filename format: `NNN-slug.md` (three-digit numeric prefix)
2. Must include: Background, Goal, Acceptance Criteria (checkbox list), Notes (optional)
3. Acceptance criteria must be concrete and testable — write "when Y, Z holds" not just "implement X"

### Editing spec.md

The type definitions in spec.md (`ToolMetadata`, `ModelStreamChunk`, `RunStatus`, etc.) are the implementation contract for v0.1. When editing:
- Sync any affected issue acceptance criteria
- Append the rationale to the `## Design Decision Log` section at the bottom

### Editing Polaris Docs

Polaris documents record **constraints that do not change across iterations**. Edit with care — confirm this is a permanent boundary, not a current-iteration tradeoff, before modifying.

### Things to Avoid

- Do not make authoritative changes to spec.md without updating the corresponding issues
- Do not add implementation details to overview.md (it is external-facing)
- Do not bring multi-channel routing, user management, or Web UI concerns into SDK design
- Do not add a Non-Goal without grounding it in an existing polaris rationale

---

## Rust Project Conventions

### Workspace Structure

```
Cargo.toml                       # workspace root — no business logic here
crates/
  agent-runtime-core/            # pure Rust core, no FFI
    src/
      lib.rs
      run.rs                     # AgentRun, RunState, run loop
      tool/
        mod.rs                   # Tool trait, ToolRegistry, ToolOutput
        in_process.rs            # FFI callback tool
        skill_bundled.rs         # script tool + async job protocol parsing
        async_job.rs             # JobHandle, JobStatus, poll loop
        builtin.rs               # built-in read_file tool
        mcp.rs                   # MCP tool (added in v0.2)
      skill/
        mod.rs                   # SkillManifest, discovery, SKILL.md parsing
        executor.rs              # ScriptExecutor trait + BareSubprocessExecutor
      model/
        mod.rs                   # ModelAdapter trait
        anthropic.rs
        openai.rs                # added in v0.2
        streaming.rs             # ModelStreamChunk shared logic
      events.rs                  # RuntimeEvent enum
      budget.rs                  # BudgetGuard, BudgetConfig, BudgetUsage
  agent-runtime-py/              # PyO3 binding — no business logic
    src/lib.rs
  agent-runtime-node/            # napi-rs binding — no business logic
    src/lib.rs
examples/
skills/                          # example skills
```

**Rule:** All business logic lives in `agent-runtime-core`. Binding crates only do type conversion and FFI glue — no business decisions.

### Dependencies

**Locked core dependencies (do not replace):**

| Crate | Purpose | Features |
|-------|---------|---------|
| `tokio` | Async runtime | `full` |
| `serde` + `serde_json` | Serialization | `derive` |
| `async-trait` | Async trait objects | — |
| `uuid` | RunId | `v4`, `serde` |
| `thiserror` | Error types in library crates | — |
| `pyo3` | Python binding | `extension-module` |
| `napi` + `napi-derive` | Node.js binding | — |

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

# Verify no stale v1.0 references remain (should return nothing)
rg "v1\.0|v1_0" docs/
```
