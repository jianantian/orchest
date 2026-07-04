# Briefing Desk — v0.10 Demo Validation Report

Demo A of the two-round pre-v1.0 validation strategy (see
[`docs/archive/iteration/v0_10/prd.md`](../archive/iteration/v0_10/prd.md)). Briefing Desk, a
local multimedia research-brief agent, was built against Orchest's public API
only (issues 001-005) to dogfood the runtime's orchestration surface
(model/tool/approval/event/session/sub-agent) and the ASR/TTS/multimodal
provider gateways before v1.0 freezes them. This report is the decision
record: what must change before v1.0 (release blockers), what's scoped to
this demo only (demo blockers, already fixed), and what can stay backlog.

**Bottom line**: the demo works end to end in `--fake` mode and validates
most of the target surface cleanly. It surfaced one hard blocker — **there is
currently no public API path to real multimodal image input at all** — plus
several smaller API-ergonomics and documentation gaps, all listed below with
file/function references. **No live provider run was performed** (see
"Live provider run" below); v1.0 cannot proceed on the live-path evidence
until one is completed or the release gate is consciously changed, per this
issue's own instructions.

## Runs

### Fake smoke run

```bash
cargo test -p briefing-desk-demo
```

- **Commit tested**: `f87574b514e3015a9ef8230db4d1e9f43e660db3` (issue 005,
  `iteration/v0_10` branch)
- **Date**: 2026-07-02
- **Result**: 19/19 tests pass (12 unit + 7 black-box CLI smoke tests).
  `cargo clippy -p briefing-desk-demo --all-targets -- -D warnings` and
  `cargo fmt --check` both clean; `cargo check --workspace` clean.

Manually re-run as a full end-to-end CLI session (not just the test binary)
on the same commit, from a clean directory:

```bash
cargo run -p briefing-desk-demo -- run \
  --materials examples/demo/briefing-desk/fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/bd-006/brief.md \
  --session v006-demo \
  --fake
# exit 0. search_fixtures -> read_fixture -> transcribe_audio ->
# describe_image -> review_report (real Agent-as-Tool sub-run, verdict
# visible in the event stream) -> write_report (approval granted) ->
# synthesize_brief (approval granted). brief.md contains both the
# ASR-transcribed interview quote and the image-derived chart numbers.

cargo run -p briefing-desk-demo -- resume \
  --session v006-demo \
  --question "Has anything changed about the retention numbers?" \
  --output /tmp/bd-006/followup.md \
  --fake
# exit 0, genuinely separate process, same run_id preserved. Follow-up
# answer quotes a snippet of the original brief pulled from the
# cross-process persisted session.
```

### Live provider run

**Not performed.** This environment has no LLM/ASR/TTS credentials and no
network egress. Per this issue's own instructions: this report says so
explicitly, and **v1.0 must not proceed on the strength of this report alone
until a live run is completed, or the project consciously decides to change
the release gate.**

The commands a maintainer with credentials should run, exactly as documented
in [`examples/demo/briefing-desk/README.md`](../../examples/demo/briefing-desk/README.md#live-provider-run):

```bash
export BRIEFING_DESK_ASR_PROVIDER=volcengine
export BRIEFING_DESK_ASR_MODEL=<model id>
export BRIEFING_DESK_ASR_API_KEY=<key>
export BRIEFING_DESK_TTS_PROVIDER=volcengine
export BRIEFING_DESK_TTS_MODEL=<model id>
export BRIEFING_DESK_TTS_API_KEY=<key>

cargo run -p briefing-desk-demo -- run \
  --materials examples/demo/briefing-desk/fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md \
  --fake   # still required — governs the chat model only, see below
```

Note even this live command still passes `--fake`: no v0.10 issue wires a
live chat/LLM provider into this CLI (see "LLM (text)" row in the freeze
coverage statement below) — `--fake` selects `FakeModel` for the chat step
regardless, while the ASR/TTS env vars independently select real provider
instances for those two steps only. Whoever runs this should record: exact
command, provider, model, date, and outcome (success / error, with the
`ProtocolError` if any) — either as an update to this report or a follow-up
entry in `docs/archive/iteration/v0_10/validation-notes.md`.

## Freeze Coverage Statement

Which public provider gateways this demo actually exercised before v1.0
freezes their API surface:

| Gateway | Exercised? | Fake | Live | Notes |
|---|---|---|---|---|
| LLM (text, chat) | Partially | Yes (`FakeModel`, all 5 issues) | **No** | No v0.10 issue scoped wiring a live chat provider into this CLI; the runtime's chat-adapter path itself is separately exercised by `examples/rust/basic_agent_run.rs` and friends, just not from *this* demo. Flagged as a coverage gap, not a blocker — the chat/model adapter surface is the most mature and most independently-tested part of the runtime. |
| ASR | **Yes** | Yes (`FakeAsr`, real `Asr` trait impl) | Wired, untested (no credentials) | `transcribe_audio` tool, real `orchest_protocol::Asr` + `orchest_provider::Registry` construction path. This is exactly the kind of newly-added satellite surface (v0.9.1/v0.9.6) the PRD wanted dogfooded. |
| TTS | **Yes** | Yes (`FakeTts`, real `Tts` trait impl) | Wired, untested (no credentials) | `synthesize_brief` tool, same pattern as ASR. Independently approval-gated from `write_report`. |
| Multimodal image input | **Yes (fixed)** | Yes (`DescribeImageFakeModel`, real `ContentBlock::Image` construction + real `ModelAdapter::complete()` call) | Not exercised (demo has no live chat adapter wiring at all — separate, pre-existing gap, not part of this fix) | Fixed by [#195](https://github.com/jianantian/orchest/issues/195): `AgentRun::start` now takes a `RunInput` (`RunInput::text(..).with_image(..)` or `.from_blocks(..)`), and `describe_image` builds a real `ContentBlock::Image` from the corpus file and drives it through a real `ModelAdapter::complete()` call. See `crates/orchest/src/run/config.rs` (`RunInput`) and `examples/demo/briefing-desk/src/media.rs` (`DescribeImageTool`). |
| AIGC image generation | **Not exercised — consciously skipped** | — | — | Optional stretch per the PRD ("lowest freeze risk... if skipped, record it as a conscious coverage gap"). Skipped because the demo's materials-ingestion flow (search/read/transcribe/describe) didn't produce a genuine need for a generated figure — forcing one in would have been a contrived, low-signal integration. Risk accepted consciously: AIGC (`orchest-provider-visual`) is the most mature of the four modality gateways (v0.6.1 + two hotfix passes, `docs/archive/iteration/v0_6_1/`), with its own crate-level test coverage, unlike ASR/TTS/multimodal-image which are all v0.9.x-era and had zero non-test-internal usage anywhere in the repo before this demo (see findings below). |

**Two gateways (ASR, TTS) go from zero real usage anywhere in the repository
to dogfooded and passing.** One gateway (multimodal image input) goes from
zero real usage to a confirmed, concrete blocker — which is exactly the kind
of finding this validation round exists to produce. AIGC's absence is a
conscious, argued choice, not an oversight.

## Findings

Full raw findings, including per-issue context, live in
[`docs/archive/iteration/v0_10/validation-notes.md`](../archive/iteration/v0_10/validation-notes.md).
This section is the consolidated, triaged version required by this issue.

### API friction

1. **`AgentRun::resume` has no input/new-message parameter** —
   `crates/orchest/src/run/mod.rs:79-104`. Continuing a session with a new
   follow-up question requires manually pushing a
   `Message { role: Role::User, content: vec![ContentBlock::Text(question)] }`
   onto `SessionSnapshot.messages` before calling `resume` — undiscoverable
   except by reading the source, since `AgentRun::start`'s sibling `input:
   String` parameter sets an expectation `resume` doesn't meet. No error is
   raised if skipped; the model is simply re-invoked against unchanged
   history. Demonstrated working around it in
   `examples/demo/briefing-desk/src/app.rs`'s `resume()`.
2. **Deserialized `AgentConfig` silently drops session persistence** —
   `crates/orchest/src/run/config.rs` (`session_store` field, `#[serde(skip)]`).
   After `SessionStore::load()`, `active_config.session_store` is `None`;
   persistence for the resumed run silently stops unless the caller calls
   `.with_session_store(store, id)` again. No error on omission.
3. **`SubAgentBuilder::build()` panics instead of returning `Result`** —
   `crates/orchest/src/tool/agent_as_tool.rs` (`SubAgentBuilder::build`).
   Inconsistent with `AgentConfigBuilder::build() -> Result<_, ConfigError>`
   and `ToolRegistry::register() -> Result<_, RegistryError>` elsewhere in
   the same runtime.
4. **`ContextMode` is not re-exported alongside its sibling types** —
   `crates/orchest/src/tool/agent_as_tool.rs`. Requires
   `orchest::tool::agent_as_tool::ContextMode` where `Approval`/`ApprovalMode`/
   `ToolMetadata` are all reachable one level up. Minor but real friction —
   discoverable only via the `agent_as_tool.rs` example.
5. **ASR/TTS construction has no chat-equivalent convenience function** —
   `crates/orchest-provider/src/lib.rs` has `create_adapter_from_config` for
   chat; ASR/TTS require the more general
   `Registry::asr()/.tts().provider(..).build(&ProviderConfig::new(..))`
   chain with a different config struct (`ProviderConfig` vs.
   `ProviderRuntimeConfig`). Works fine once found; just an extra idiom to
   learn per capability tier.

### Modality gateway friction

1. **No public API path to real vision-through-agent-loop exists at all**
   (the headline finding — see "Release blockers" below for full detail and
   triage). `crates/orchest/src/run/mod.rs` (`AgentRun::start`,
   `start_with_bus`); `crates/orchest-protocol/src/types.rs`
   (`ContentBlock::ToolResult.content: Value`).
2. **No reusable fake `Asr`/`Tts` existed anywhere in the workspace before
   this demo** — the pre-seeded finding from issue 005's own text,
   re-verified fresh rather than trusted: `grep -rn
   "FakeAsr\|FakeTts\|MockAsr\|MockTts"` across `crates/orchest-provider*`
   and `crates/orchest` found one private `FakeAsr` local to
   `crates/orchest-provider/tests/selection.rs` (not `pub`, not reusable —
   integration-test binaries never export to downstream crates regardless of
   visibility) and zero `FakeTts` anywhere. Both were written from scratch in
   `examples/demo/briefing-desk/src/media.rs`.
3. **TTS has zero registered entries under the `http` (REST) tier** —
   `crates/orchest-provider-http/src/lib.rs` (`tts_entries()` is a literal
   `Vec::new()`, with an internal comment "Filled in Issue 006" referring to
   a *different*, earlier iteration's issue numbering, not this one). All
   real TTS providers live in `orchest-provider-stream`. Harmless today
   because the `tts` feature alias on `orchest-provider` pulls both tiers,
   but a caller who enables only `http` for a TTS-only use case gets an
   empty registry with no compile-time signal.
4. **AIGC**: not exercised this iteration (see Freeze Coverage Statement) —
   no new finding to report; existing crate-level coverage stands.

### Documentation friction

1. **`docs/guide/quickstart.md`** (§8, "下一步") points to
   `session_persist_resume.rs` and `agent_as_tool.rs` with a one-line
   pointer each, but neither the guide nor the examples document the
   `AgentRun::resume` follow-up-message gotcha (API friction #1 above) or
   the `ContextMode` import path (API friction #4 above). Target: add a
   short prose paragraph under each bullet, not just the file pointer.
2. **`crates/orchest/src/run/mod.rs:79`** — `AgentRun::resume`'s doc comment
   is a single generic line ("Resume a previous run from a persisted
   snapshot.") that does not mention the missing-input-parameter gotcha
   (API friction #1). This is the single highest-value doc fix identified in
   this validation round — it is exactly the kind of thing a doc comment
   should carry, and its absence is what caused the finding to require
   reading the implementation instead of the API contract.
3. **`examples/demo/briefing-desk/README.md`** and
   `docs/archive/iteration/v0_10/prd.md` are, by contrast, in good shape — no gap
   found reading them against the delivered code; both were kept in sync
   issue-by-issue rather than written up front and left stale.

### Runtime bugs

None found. Every approval/deny path, session round-trip, and sub-agent
event-forwarding path behaved exactly as the public API's types and
signatures implied, across dozens of manual and automated runs.

### Product bugs

None found in `examples/demo/briefing-desk` itself at time of writing — all
19 automated tests pass and every manual run matched expected behavior.

## Triage

| # | Finding | Category | Triage | Tracking | Why |
|---|---|---|---|---|---|
| 1 | No public API path to real multimodal image input | Modality gateway friction | **Release blocker** | [#195](https://github.com/jianantian/orchest/issues/195) | The gateway is unusable from application code without a core-API change — directly meets the PRD's own triage rule ("release blocker if the gateway is unusable from application code without workarounds, because v1.0 freezes those public APIs"). |
| 2 | No reusable fake `Asr`/`Tts` in the workspace | Modality gateway friction | **Release blocker** | [#196](https://github.com/jianantian/orchest/issues/196) | Same PRD rule: "missing fake provider hook" is explicitly named as a release-blocker-eligible finding. Both gateways otherwise work; only the *fake* path was unreachable pre-demo. |
| 3 | `AgentRun::resume` has no input parameter, silent-wrong-behavior risk | API friction | **Release blocker** | [#197](https://github.com/jianantian/orchest/issues/197) | Silent-wrong-behavior (not silent no-op) footguns in a freezing public API are exactly what pre-1.0 validation exists to catch. |
| 4 | Deserialized `AgentConfig` silently drops session persistence | API friction | **Release blocker** | [#198](https://github.com/jianantian/orchest/issues/198) | Same class as #3: silent data-loss shape, not merely confusing. |
| 5 | `SubAgentBuilder::build()` panics instead of `Result` | API friction | **Release blocker** | [#199](https://github.com/jianantian/orchest/issues/199) | Inconsistent with the rest of the builder surface; panics from application-level misuse are a worse failure mode than the `Result` used everywhere else. |
| 6 | `ContextMode` re-export depth | API friction | Post-1.0 backlog | — | Cosmetic, one-line fix, does not affect correctness or safety. |
| 7 | ASR/TTS construction idiom differs from chat's convenience function | API friction | Post-1.0 backlog | — | Works correctly once found; ergonomics-only. |
| 8 | TTS empty `http`-tier registry, no compile-time signal | Modality gateway friction | Post-1.0 backlog | — | Matches the provider crate's own "filled in later" comment; not introduced by this iteration, and the `tts` feature alias already routes around it correctly. |
| 9 | `quickstart.md` pointers lack prose for resume/Agent-as-Tool gotchas | Documentation friction | Post-1.0 backlog | — | Nice-to-have; the example files themselves are correct, just under-narrated. |
| 10 | `AgentRun::resume` doc comment omits the input-parameter gotcha | Documentation friction | **Release blocker** (paired with #3) | [#197](https://github.com/jianantian/orchest/issues/197) | The doc fix *is* the fix for #3 in the cheapest case — if the API shape doesn't change before v1.0, the doc comment must, at minimum. |

Each release-blocker row has a tracking issue, labeled `release-blocker`, whose
acceptance criteria require re-running Briefing Desk (`cargo test -p
briefing-desk-demo` and/or a manual `--fake`/live run, as applicable) after
the fix and posting the output before the issue can close — so "the code
changed" and "the finding is verified fixed" stay distinct, checkable states
instead of collapsing into one merge. See
[#195](https://github.com/jianantian/orchest/issues/195),
[#196](https://github.com/jianantian/orchest/issues/196),
[#197](https://github.com/jianantian/orchest/issues/197),
[#198](https://github.com/jianantian/orchest/issues/198),
[#199](https://github.com/jianantian/orchest/issues/199).

Demo blockers (issues that would have prevented Briefing Desk itself from
working, fixed inline during issues 001-005 rather than carried forward):
none remain open. Every demo-blocking issue encountered during development
(e.g. the original `FakeModel` needing message-history inspection to drive a
real tool sequence, the session-path cross-process wiring) was resolved
within its own issue and is not re-litigated here.

## Release-blocker fixes required before v1.0

Grounded directly in the findings above, not inferred from unvalidated
backlog:

1. Add a small, deliberate public API surface for seeding a run with
   multimodal content — e.g. an `AgentRun::start`-equivalent accepting
   `Vec<ContentBlock>`/`Vec<Message>`, or narrowly publicizing the relevant
   slice of `start_with_bus`. (Finding #1 —
   [#195](https://github.com/jianantian/orchest/issues/195))
2. Either ship a reusable fake `Asr`/`Tts` (e.g. a `orchest-provider`
   `testing` feature or module) or explicitly document that downstream
   crates are expected to write their own, as this demo did. (Finding #2 —
   [#196](https://github.com/jianantian/orchest/issues/196))
3. Give `AgentRun::resume` either a way to append new input directly, or —
   at minimum — a prominent doc comment describing the manual-append
   requirement and the silent-no-persistence-if-forgotten gotcha. (Findings
   #3, #4, #10 — [#197](https://github.com/jianantian/orchest/issues/197),
   [#198](https://github.com/jianantian/orchest/issues/198))
4. Change `SubAgentBuilder::build()` to return `Result` for consistency with
   the rest of the builder-pattern surface. (Finding #5 —
   [#199](https://github.com/jianantian/orchest/issues/199))

None of these require new provider adapters or new runtime concepts — all
four are narrow, targeted fixes to existing public surface, consistent with
the PRD's framing of v0.10 as deciding "which API/documentation fixes are
release blockers," not opening new scope.

## Path to v0.11

Per the PRD, v0.11 (Demo B, Research Pipeline) begins by running against the
seam-gap list this report produces for the deeper supervised-delegation
surface (`LlmWatcher`, `Steering`, `ContextMode`, supervisor recovery,
multi-watcher FIFO). This round's reviewer path (Agent-as-Tool,
`ContextMode::Fresh`) worked cleanly with no friction beyond finding #4
(re-export depth) — there is no deep supervised-delegation finding to hand
off, because v0.10 deliberately only exercised the lightweight reviewer
path, not `LlmWatcher`/Steering/multi-watcher FIFO (those remain entirely
v0.11's to validate, per the PRD's Delegation Validation Boundary). v0.11
should treat this report's `ContextMode` and `SubAgentBuilder` findings
(#3-#5, #6) as its starting seam-gap list, since both surfaces sit directly
on the boundary between this round's lightweight validation and next
round's deep one.
