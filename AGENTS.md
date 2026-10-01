# Orchest — Agent Working Guide

## What This Is

Orchest is a **low-level Rust SDK** that provides the agent runtime core for building AI agent applications. It is not a complete agent product — it is the engine that other agent products run on: responsible for the agent loop, state management, event streaming, tool dispatch, and skill loading.

Current stage: **1.x, released** — `1.0.0` of the eight lockstep crates is on crates.io (2026-10-01; versioning and SemVer policy in [ADR-0003](./docs/adr/0003-release-policy.md)). The Python/TypeScript binding packages are not published yet. The Rust core (`orchest`) and the shared protocol spine (`orchest-protocol`) are built out, alongside weight-tier provider crates (`orchest-provider-http`/`-stream`/`-visual`, covering LLM, ASR, TTS, and image/video AIGC) behind the `orchest-provider` registry wall. Design docs under `docs/` remain the authoritative implementation contract; code lives in `crates/`, `examples/`, and `skills/` per the Rust Project Conventions below. The living roadmap is [`docs/iteration/roadmap.md`](./docs/iteration/roadmap.md).

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
│   └── <version>/             # Active/planned iterations — each: prd.md + issues/NNN-slug/{spec,plan}.md
├── hotfix/<date>/              # Active refactor/repair iterations (created when needed)
├── archive/                    # Completed iterations + hotfixes (v0_1 … v1_0, hotfix/<date>), moved here after closeout
├── adr/                        # Architecture decision records (0001 provider unification … 0003 release policy)
├── review/                     # Validation reports and reviews (e.g. v1_0_public_api.md + inventories)
├── todo/                       # Forward-looking direction notes not yet scheduled (e.g. provider-unification.md)
├── external/                   # Upstream vendor API docs (anthropic, minimax, volceengine, aliyun, …)
├── research/                   # Architecture research
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

Snapshot (2026-10): v0.1–v0.17 and v1.0 shipped; the repository is public. `1.1.0` (hotfix 2026-10-01: DeepSeek model list and `deepseek-flash` image input) is the latest crates.io release. Nothing else is scheduled yet; candidate directions live in `docs/todo/`.

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
- **Provider crates are organized by wire-dialect weight, not by modality** (v0.9.12 Provider Unification, see [`docs/adr/0001-provider-unification.md`](./docs/adr/0001-provider-unification.md)) — `orchest-provider-http` (REST/SSE), `orchest-provider-stream` (WebSocket), `orchest-provider-visual` (signed/polled gen), each depending on `orchest-protocol` + `orchest-provider-core`, never on `orchest`. Consumers select providers only through the `orchest-provider` umbrella wall by capability query or identity pick — impl crates and wire dialects are never named outside it. Do not reintroduce a per-modality crate split (LLM/ASR/TTS/AIGC each as their own crate) — that was the pre-v0.9.12 architecture and was deliberately dissolved.

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

Full conventions — workspace structure, dependency policy, error handling, traits/visibility, async, serialization, unsafe policy, testing, PyO3 build verification, naming, and code organization — live in [CONVENTIONS.md](./CONVENTIONS.md). Read it before writing core/provider/binding code.

Non-negotiable invariants (the rest is in `CONVENTIONS.md`):

- Runtime business logic lives in `orchest`; shared protocol/capability contract in `orchest-protocol`; provider adapters in their weight-tier crates (`orchest-provider-http`/`-stream`/`-visual`, each depending on `orchest-protocol` + `orchest-provider-core`, never on `orchest`); binding crates (`orchest-py`, `orchest-node`) do FFI glue only - no business decisions.
- `orchest` contains no `unsafe`.
- `unwrap()` / `expect()` are banned in library code outside `#[cfg(test)]` (or a documented invariant).
- `thiserror` in library crates; `anyhow` only in application binaries; do not introduce actor frameworks.
- Use `async-trait` for trait methods - not `-> impl Future` (PyO3/napi FFI incompatibility).
- CI must pass: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`.

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
