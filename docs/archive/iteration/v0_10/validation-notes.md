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

## Issue 005 — multimedia ingestion and audio output

### No public API path to real vision-through-agent-loop at all

**Category**: modality gateway friction (the most significant single finding
in v0.10 so far).
**Finding**: real image input requires `ContentBlock::Image` inside the
model's message history, but every path to get it there is blocked:
`AgentRun::start(config, input: String, ...)` — the only public entry
point — takes a plain `String`, not `Vec<ContentBlock>`/`Vec<Message>`. The
method that does accept `initial_messages: Vec<Message>`
(`AgentRun::start_with_bus`) is `pub(crate)`, reachable only from inside
`crates/orchest` itself (its own `start()` wrapper and the internal
Agent-as-Tool spawn path). A tool can't inject an image into the next model
turn either: `ToolResult.content` (what a `Tool::execute` result becomes for
the model) is hard-typed `serde_json::Value` — structurally incapable of
carrying a `ContentBlock`. Confirmed via exhaustive search: zero working
examples of `ContentBlock::Image` reaching a real `ModelAdapter::complete()`
call exist anywhere in the repo, in any crate or test.
**Resolution for this issue**: per explicit user decision (not a unilateral
call), `orchest`'s core API was left untouched. `describe_image` is a
fixed-text placeholder tool in both `--fake` and (hypothetical) live mode —
see `examples/demo/briefing-desk/src/media.rs`'s `DescribeImageTool` doc
comment and the README's "Vision is not real" section.
**Triage**: release blocker. Multimodal image input is one of the three
required modalities in the v0.10 PRD specifically because it's newly
un-validated surface heading into the v1.0 API freeze — this finding means
it is not merely awkward, it is **currently unusable** from application
code. Needs a small, deliberate public API addition before v1.0 (e.g. an
`AgentRun::start`-equivalent that accepts a `Vec<ContentBlock>` or
`Vec<Message>`, or making a narrow slice of `start_with_bus` public) —
proposed here, not decided unilaterally.

### No reusable fake `Asr`/`Tts` anywhere in the workspace (re-confirmed)

**Category**: modality gateway friction (this is the exact scenario the
validation rubric's own worked example describes).
**Finding**: re-verified fresh (not trusting the pre-seeded note in the issue
doc): `grep -rn "FakeAsr\|FakeTts\|MockAsr\|MockTts"` across
`crates/orchest-provider*` and `crates/orchest` turns up one private
`FakeAsr` struct local to `crates/orchest-provider/tests/selection.rs`
(not `pub`, and integration-test binaries are never part of a crate's
importable surface regardless of visibility) and zero `FakeTts` anywhere.
**Resolution**: this issue writes both from scratch
(`media::FakeAsr`/`media::FakeTts`, real trait impls of
`orchest_protocol::{Asr, Tts}`) since there was nothing to reuse.
**Triage**: release blocker — before v1.0 freezes the ASR/TTS gateway APIs,
either a shared fake-provider crate/module should exist for downstream
testing, or the gap should be a documented, intentional non-goal.

### ASR/TTS construction has no chat-equivalent convenience function

**Category**: API friction (minor).
**Finding**: the chat path has a one-call convenience function,
`orchest_provider::create_adapter_from_config(ProviderRuntimeConfig{..})`.
No equivalent exists for ASR/TTS — construction goes through the more
general `Registry::asr()/.tts().provider(..).build(&ProviderConfig::new(..))`
chain, a different config struct (`ProviderConfig`, not
`ProviderRuntimeConfig`) with a different shape. Not wrong, just
inconsistent: two capability tiers, two different "get me a working handle"
idioms.
**Triage**: post-1.0 backlog — works fine once you find the right pattern
(the registry chain is well-designed), just an extra thing to learn per
capability instead of one consistent shape.

### TTS has zero registered entries under the `http` (REST) tier

**Category**: modality gateway friction (minor, informational).
**Finding**: `orchest-provider-http::tts_entries()` is a literal
`Vec::new()` (comment: "Filled in Issue 006" — an internal note from the
provider crate's own earlier iteration, not this v0.10 iteration). All real
TTS providers live in the `stream` tier. Enabling `orchest-provider`'s `tts`
feature alias pulls both `http` and `stream` regardless, which is why this
didn't block the demo — but a caller who only enables `http` for a
TTS-only use case would get an empty registry with no obvious error until
`.select()` fails at runtime with `NoMatchingProvider`.
**Triage**: post-1.0 backlog — matches the code's own "filled in later"
comment; not a v0.10-introduced gap.
