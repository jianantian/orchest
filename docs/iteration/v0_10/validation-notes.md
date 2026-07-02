# v0.10 Validation Notes (running log)

Findings recorded issue-by-issue while building Briefing Desk. Classified per
`examples/demo/briefing-desk/fixtures/expected/validation-rubric.md` (finding
category) and the PRD's Validation Triage Rule (demo blocker / release
blocker / post-1.0 backlog). Issue 006 consolidates this into the final
validation report — this file is the raw material, not the deliverable.

## Issue 004 — session resume and reviewer path

### `AgentRun::resume` has no input/new-message parameter

**Category**: API friction.
**Finding**: `AgentRun::resume(snapshot, model, registry)` takes no
`input: String` (unlike `AgentRun::start`). To continue a session with a new
follow-up question, the caller must manually push a `Message { role: Role::User,
content: vec![ContentBlock::Text(question)] }` onto `snapshot.messages` before
calling `resume` — there is no documented or type-guided way to learn this;
it only became clear from reading `crates/orchest/src/run/mod.rs` directly.
**Triage**: release blocker candidate. A resume API that silently does the
wrong thing (re-runs the model against unchanged history, producing a
duplicate/stale response) if you don't know to hand-mutate the snapshot is an
ergonomics gap worth closing or at least documenting prominently before v1.0.

### Deserialized `AgentConfig` silently drops session persistence unless re-attached

**Category**: API friction.
**Finding**: `AgentConfig.session_store` (and `hooks`, `retry_policy`,
`handoffs`) are `#[serde(skip)]`. After `SessionStore::load()` deserializes a
`SessionSnapshot`, `active_config.session_store` is `None` — persistence for
the resumed run silently stops unless the caller calls
`.with_session_store(store, id)` again before `AgentRun::resume`. No error is
raised if this step is skipped; a resumed-then-resumed-again session would
just quietly lose its updates.
**Triage**: release blocker candidate — silent data-loss-shaped footguns are
exactly the class of thing that should either error loudly or be automatic.

### `ContextMode` is not re-exported alongside its sibling types

**Category**: documentation/API friction (minor).
**Finding**: `Approval`, `ApprovalMode`, `ToolMetadata` etc. are reachable at
`orchest::tool::*` / `orchest::run::*`, but `ContextMode` requires the deeper
path `orchest::tool::agent_as_tool::ContextMode`. Discoverable only by reading
the `agent_as_tool.rs` example, not from the shallower public module surface.
**Triage**: post-1.0 backlog — cosmetic, one-line fix (re-export), not
blocking correctness.

### `SubAgentBuilder::build()` panics instead of returning `Result`

**Category**: API friction.
**Finding**: `SubAgentBuilder::build()` calls `.expect(...)` internally if
`.model()` or `.registry()` weren't called, rather than surfacing a typed
error. Every other fallible construction path in the runtime seen so far
(`AgentConfigBuilder::build() -> Result<_, ConfigError>`,
`ToolRegistry::register() -> Result<_, RegistryError>`) returns `Result`.
**Triage**: release blocker candidate — inconsistent with the rest of the
builder-pattern API surface; a misuse panic in application code is a worse
failure mode than the equivalent `Result` everywhere else in the runtime.

### Agent-as-Tool sub-agent event visibility — works well, no finding

For completeness: `RuntimeEvent::SubAgentStarted/SubAgentEvent/SubAgentCompleted`
forward every child event to the parent's stream automatically, with no extra
plumbing required. This worked exactly as hoped on the first try — recorded
here as a confirmed non-finding, not a problem.
