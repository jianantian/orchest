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

**Update (2026-07-02, hotfix issue 006 closeout)**: all 5 release-blocker
findings below (#195–#199) are fixed and merged — see the Triage table and
the Freeze Coverage Statement for what changed. The demo was fully re-run
after all five landed (`cargo test -p briefing-desk-demo`, plus a manual
`--fake run` + `--fake resume` end-to-end session); results below. **The live
provider run is still not performed** — this environment has no
LLM/ASR/TTS credentials — so the original gate stands unchanged: v1.0 must
not proceed on live-path evidence until a maintainer with credentials runs
the commands in "Live provider run" below, or the project consciously
changes the release gate.

**Update (2026-09-22, issue [#258](https://github.com/jianantian/orchest/issues/258))**:
the live provider run **is performed** — see "Live provider run" below. Chat,
vision, ASR, and TTS all ran against real providers in one end-to-end session
at revision `8a52610` plus three demo-side wiring fixes the run itself
surfaced, and the Freeze Coverage Statement's Live column is updated
accordingly. The paragraphs above are kept as the record of the state before
that run.

## Runs

### Fake smoke run

```bash
cargo test -p briefing-desk-demo
```

- **Commit tested (original v0.10 finding)**: `f87574b514e3015a9ef8230db4d1e9f43e660db3`
  (issue 005, `iteration/v0_10` branch) — 19/19 tests pass
- **Commit tested (hotfix 2026-07-02 closeout, issue 006)**: `8bb9a9b74f132f9e8db8464819d9364f25595fcd`
  (after #195–#199 all merged) — **20/20 tests pass** (13 unit + 7 black-box
  CLI smoke tests; +1 unit test vs. the original run, from issue 005's
  `orchest-provider::fakes` migration). `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo fmt --check`, `cargo test --workspace --features
  orchest/sqlite-session`, and `bash scripts/lint-check.sh` all clean (same
  two pre-existing, out-of-scope clippy findings as before: `result_large_err`
  in `orchest-provider/tests/selection.rs`, `too_many_arguments` in
  `orchest/src/run/tests.rs`).

Manually re-run as a full end-to-end CLI session (not just the test binary)
on the post-hotfix commit, from a clean directory:

```bash
cargo run -p briefing-desk-demo -- run \
  --materials examples/demo/briefing-desk/fixtures/research \
  --question "What would happen if Loom disappeared tomorrow, and what does the retention dashboard say?" \
  --output /tmp/bd-006/brief.md \
  --session demo-session-006 \
  --fake
# exit 0. search_fixtures -> read_fixture -> transcribe_audio ->
# describe_image -> review_report (real Agent-as-Tool sub-run, verdict
# visible in the event stream) -> write_report (approval granted) ->
# synthesize_brief (approval granted). brief.md contains both the
# ASR-transcribed interview quote and the image-derived chart numbers
# (describe_image's output references the same 42%/35% figures the
# fixture corpus disagrees on — the real ContentBlock::Image path, not
# a hard-coded string, since #195 landed).

cargo run -p briefing-desk-demo -- resume \
  --session demo-session-006 \
  --question "Follow up: which channel should we prioritize for Q4?" \
  --output /tmp/bd-006/followup.md \
  --fake
# exit 0, genuinely separate process, same run_id preserved (via
# resume_with_input landed in #197 + the session_store re-attach check
# from #198 — dropping the re-attach step now fails loudly instead of
# silently). Follow-up answer references the original brief pulled from
# the cross-process persisted session.
```

### Live provider run

**Performed 2026-09-22** (issue [#258](https://github.com/jianantian/orchest/issues/258))
against real providers, one end-to-end session covering chat, vision, ASR, and
TTS. Revision `8a52610` plus the demo-side fixes below; worktree otherwise
clean; environment key names only — no key material is recorded here.

- Chat (agent loop, `review_report` sub-agent): `openrouter`,
  `anthropic/claude-sonnet-4.6`
- Vision (`describe_image`, same chat adapter): `openrouter`,
  `anthropic/claude-sonnet-4.6`
- ASR (`transcribe_audio`): `aliyun` (DashScope), `fun-asr-flash-2026-06-15`
- TTS (`synthesize_brief`): `volcengine`, model/resource
  `volc.service_type.10029`, voice `zh_female_shuangkuaisisi_moon_bigtts`

Exact command (the CLI no longer has `--fake`; `run`/`resume` always build a
real chat adapter from `BRIEFING_DESK_CHAT_MODEL`):

```bash
set -a && . ./.env && set +a
export BRIEFING_DESK_CHAT_MODEL=openrouter/anthropic/claude-sonnet-4.6
export BRIEFING_DESK_CHAT_API_KEY=<openrouter key>
export BRIEFING_DESK_ASR_PROVIDER=aliyun
export BRIEFING_DESK_ASR_MODEL=fun-asr-flash-2026-06-15
export BRIEFING_DESK_ASR_API_KEY=<DashScope key>
export BRIEFING_DESK_TTS_PROVIDER=volcengine
export BRIEFING_DESK_TTS_MODEL=volc.service_type.10029
export BRIEFING_DESK_TTS_API_KEY=<volcengine key>
export BRIEFING_DESK_TTS_VOICE=zh_female_shuangkuaisisi_moon_bigtts

cargo run -p briefing-desk-demo -- run \
  --materials examples/demo/briefing-desk/fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/bd-live/brief.md
# exit 0; 6 model turns (1.6k-10.8k prompt tokens each)
# search_fixtures -> transcribe_audio -> describe_image -> read_fixture x5
# -> review_report (real Agent-as-Tool child run 1265e902, verdict FAIL on the
# first draft) -> write_report (auto-approved, 7290 bytes)
# -> synthesize_brief (auto-approved, 13269484 bytes)
# [done] final message; [report] and [synthesize] both written.
```

Per-layer outcome, all four rows live:

| Layer | Outcome | Evidence |
|---|---|---|
| Chat / agent loop | **Pass** | 6 turns, tool loop drove the whole pipeline, final answer returned |
| Vision (`describe_image`) | **Pass** | real `ContentBlock::Image` → `ModelAdapter::complete()`, child usage 21 in / 35 out tokens (the corpus chart is a ~1 KB PNG), description used in the draft |
| ASR (`transcribe_audio`) | **Pass** | transcript returned the fixture's audio-only line ("... lose about two hours a day ..."), i.e. not the offline `FAKE_TRANSCRIPT` string |
| TTS (`synthesize_brief`) | **Pass** | 13.3 MB audio written; RIFF/16-bit PCM/mono/24 kHz (volcengine streams a WAV with an unpatched RIFF length field and an `ISFT Lavf58.7` tag, so strict WAV parsers report an unknown length while the samples are valid PCM) |
| Reviewer sub-agent | **Pass** | genuine child run; verdict (`FAIL` on the first draft) forwarded to the parent event stream and appended to the brief |

Diagnostics kept out of the report for brevity: raw stdout for each run lives
in the session scratch logs (`live2.log`), not in the repository.

### Live-path defects this run found

Three defects were unreachable from the offline/scripted path and only
surfaced with a real model and real providers. All three are demo-side wiring
bugs — the public SDK surface behaved as documented — and all three are fixed
in this issue, with the code paths recorded in
[`examples/demo/briefing-desk/`](../../examples/demo/briefing-desk/):

1. **`review_report` advertised one parameter and required another.** The tool
   description and `input_mapper` both name `draft`, but `SubAgentBuilder`
   defaults to `{"input": "string"}` and the demo never called
   `.input_schema(..)`. A live model follows the schema, so every call failed
   with `missing required parameter 'draft'` (two attempts, then the model
   reasoned explicitly about the contradiction). Scripted models call the tool
   with `draft` directly, which is why `cargo test -p briefing-desk-demo`
   stayed green. Fixed by declaring the schema.
2. **`BRIEFING_DESK_ASR_MODEL` did not select anything.** `live_asr` filtered
   only by provider, and the registry's unique `default_for_provider` for
   `aliyun` is the *streaming* dialect, whose `transcribe` returns
   `UnsupportedOperation: aliyun ASR is streaming-only; use start_stream`.
   The README's documented `volcengine` example was likewise impossible —
   that dialect requires `api_url`, which the demo never sets. Fixed by
   filtering on provider **and** model, so the documented triplet picks a
   batch dialect (`aliyun/fun-asr-flash-2026-06-15`).
3. **Live TTS had no voice.** `SynthesizeBriefTool` always sent
   `voice: None`, and every live dialect forwards that as an empty voice:
   aliyun `Request voice is invalid!`, minimax `invalid params, empty field`,
   volcengine `403 Forbidden` (with the resource id defaulting to the model
   string) or `55000000 resource ID is mismatched with speaker related
   resource`. Fixed by adding the optional `BRIEFING_DESK_TTS_VOICE`; the
   offline `FakeTts` ignores it.

The live run above uses the fixed wiring. Earlier failure output and the
provider/model probes that identified the working configurations
(`aliyun/cosyvoice-v2` + `longxiaochun_v2`, `minimax/speech-2.8-hd` +
`English_Graceful_Lady`, `volcengine/volc.service_type.10029` +
`zh_female_shuangkuaisisi_moon_bigtts`) are recorded in the triage rows below.

Two SDK-side observations fall out of the same run and are **not** release
blockers (both fail loudly, both have workarounds): `SynthesizeRequest.voice`
is documented nowhere, so `None` silently becomes an empty provider field; and
`SubAgentBuilder` accepts an `input_mapper` without requiring a matching
`input_schema`, so a mismatch only fails at call time.

## Freeze Coverage Statement

Which public provider gateways this demo actually exercised before v1.0
freezes their API surface:

| Gateway | Exercised? | Fake | Live | Notes |
|---|---|---|---|---|
| LLM (text, chat) | **Yes** | Yes (`FakeModel`, v0.10 issues; `--scripted` model in the v0.16 Eval Lab) | **Yes** (2026-09-22, issue #258) | `run`/`resume` now always build a real adapter from `BRIEFING_DESK_CHAT_MODEL` (the `--fake` flag was removed after v0.10); the live session at revision `8a52610` drove the full pipeline, 6 model turns, plus a real `review_report` child run. |
| ASR | **Yes** | Yes (`orchest_provider::fakes::FakeAsr`, real `Asr` trait impl, since issue 005) | **Yes** (2026-09-22, issue #258) | `transcribe_audio` tool, real `orchest_protocol::Asr` + `orchest_provider::Registry` construction path. Live: `aliyun/fun-asr-flash-2026-06-15` returned the audio-only quote from `interview.wav`. The registry's provider-only default for `aliyun` is a streaming dialect that rejects `transcribe`, so the model filter is what makes this path usable — see "Live-path defects". |
| TTS | **Yes** | Yes (`orchest_provider::fakes::FakeTts`, real `Tts` trait impl, since issue 005) | **Yes** (2026-09-22, issue #258) | `synthesize_brief` tool, independently approval-gated from `write_report`. Live: volcengine `volc.service_type.10029` + `zh_female_shuangkuaisisi_moon_bigtts` produced 13.3 MB of RIFF/16-bit PCM audio. Every live dialect rejects an empty voice, so `BRIEFING_DESK_TTS_VOICE` is required for this path. |
| Multimodal image input | **Yes (fixed)** | Yes (`DescribeImageFakeModel`, real `ContentBlock::Image` construction + real `ModelAdapter::complete()` call) | **Yes** (2026-09-22, issue #258) | Fixed by [#195](https://github.com/jianantian/orchest/issues/195): `AgentRun::start` now takes a `RunInput` (`RunInput::text(..).with_image(..)` or `.from_blocks(..)`), and `describe_image` builds a real `ContentBlock::Image` from the corpus file and drives it through a real `ModelAdapter::complete()` call. See `crates/orchest/src/run/config.rs` (`RunInput`) and `examples/demo/briefing-desk/src/media.rs` (`DescribeImageTool`). Live: the corpus chart was described by the vision-capable chat adapter and the description fed the brief. |
| AIGC image generation | **Not exercised — consciously skipped** | — | — | Optional stretch per the PRD ("lowest freeze risk... if skipped, record it as a conscious coverage gap"). Skipped because the demo's materials-ingestion flow (search/read/transcribe/describe) didn't produce a genuine need for a generated figure — forcing one in would have been a contrived, low-signal integration. Risk accepted consciously: AIGC (`orchest-provider-visual`) is the most mature of the four modality gateways (v0.6.1 + two hotfix passes, `docs/archive/iteration/v0_6_1/`), with its own crate-level test coverage, unlike ASR/TTS/multimodal-image which are all v0.9.x-era and had zero non-test-internal usage anywhere in the repo before this demo (see findings below). |

**Two gateways (ASR, TTS) go from zero real usage anywhere in the repository
to dogfooded and passing.** One gateway (multimodal image input) goes from
zero real usage to a confirmed, concrete blocker — which is exactly the kind
of finding this validation round exists to produce. AIGC's absence is a
conscious, argued choice, not an oversight. As of the 2026-09-22 live run all
four required rows (LLM/chat, ASR, TTS, multimodal image input) have live
evidence; the live run's own three demo-side defects are triaged below.

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
| 1 | No public API path to real multimodal image input | Modality gateway friction | **Fixed** (hotfix 2026-07-02, issue 001, commit `5d912dc`) | [#195](https://github.com/jianantian/orchest/issues/195) | The gateway is unusable from application code without a core-API change — directly meets the PRD's own triage rule ("release blocker if the gateway is unusable from application code without workarounds, because v1.0 freezes those public APIs"). Resolved via the new `RunInput` type (`AgentRun::start(config, RunInput, ..)`); `describe_image` now builds a real `ContentBlock::Image` and drives a real `ModelAdapter::complete()` call. |
| 2 | No reusable fake `Asr`/`Tts` in the workspace | Modality gateway friction | **Fixed** (hotfix 2026-07-02, issue 005, commit `8bb9a9b`) | [#196](https://github.com/jianantian/orchest/issues/196) | `orchest-provider` now ships `fakes::{FakeAsr, FakeTts}` behind a `testing` Cargo feature (no impl crate, no network deps); the demo's own hand-written fakes were deleted in favor of it. |
| 3 | `AgentRun::resume` has no input parameter, silent-wrong-behavior risk | API friction | **Fixed** (hotfix 2026-07-02, issue 002, commit `39290e8`) | [#197](https://github.com/jianantian/orchest/issues/197) | Resolved via `AgentRun::resume_with_input(snapshot, RunInput, model, registry)` — a new entry point that appends the new input as a user turn onto the snapshot history before resuming; `resume` itself is unchanged (still replays history as-is, doc comment now says so explicitly). |
| 4 | Deserialized `AgentConfig` silently drops session persistence | API friction | **Fixed** (hotfix 2026-07-02, issue 003, commit `7ceb0a4`) | [#198](https://github.com/jianantian/orchest/issues/198) | `resume`/`resume_with_input` now return `Result<_, ConfigError>` and reject with `ConfigError::SessionStoreMissing { session_id }` when the snapshot's `session_id` is set but `session_store` wasn't re-attached — no more silent stop of persistence. |
| 5 | `SubAgentBuilder::build()` panics instead of `Result` | API friction | **Fixed** (hotfix 2026-07-02, issue 004, commit `256ae9e`) | [#199](https://github.com/jianantian/orchest/issues/199) | `build()` now returns `Result<Arc<dyn Tool>, ConfigError>` with two dedicated variants (`SubAgentMissingModel`/`SubAgentMissingRegistry`), matching the `Result`-returning convention used by `AgentConfigBuilder::build()` and `ToolRegistry::register()`. |
| 6 | `ContextMode` re-export depth | API friction | Post-1.0 backlog | — | Cosmetic, one-line fix, does not affect correctness or safety. |
| 7 | ASR/TTS construction idiom differs from chat's convenience function | API friction | Post-1.0 backlog | — | Works correctly once found; ergonomics-only. |
| 8 | TTS empty `http`-tier registry, no compile-time signal | Modality gateway friction | Post-1.0 backlog | — | Matches the provider crate's own "filled in later" comment; not introduced by this iteration, and the `tts` feature alias already routes around it correctly. |
| 9 | `quickstart.md` pointers lack prose for resume/Agent-as-Tool gotchas | Documentation friction | Post-1.0 backlog | — | Nice-to-have; the example files themselves are correct, just under-narrated. |
| 10 | `AgentRun::resume` doc comment omits the input-parameter gotcha | Documentation friction | **Fixed** (hotfix 2026-07-02, issue 002, commit `39290e8`) | [#197](https://github.com/jianantian/orchest/issues/197) | `resume`'s rustdoc now states explicitly that it does not append input and points to `resume_with_input` for the follow-up case; `docs/guide/quickstart.md` §8's `session_persist_resume.rs` pointer got the same note. |
| 11 | `review_report` advertised `{"input": "string"}` while its mapper, schema-less tool description, and scripted callers all used `draft` | Product bug (demo) | **Fixed** (2026-09-22, issue #258) | — | Live-only: a real model follows the advertised schema, so the tool could never be called; scripted models call it with `draft` directly, which is why the deterministic suite stayed green. Demo now declares `.input_schema(..)`; SDK-side note (12) is separate. |
| 12 | `SubAgentBuilder` accepts an `input_mapper` without requiring a matching `input_schema`, so the mismatch only fails at call time | API friction | Post-1.0 backlog | [#299](https://github.com/jianantian/orchest/issues/299) | Fails loudly with a clear message once reached, and the builder already exposes `.input_schema(..)`; a build-time consistency check is ergonomics, not correctness. Deriving a schema from the mapper is not generally possible. |
| 13 | `BRIEFING_DESK_ASR_MODEL` selected nothing: provider-only registry filtering resolved `aliyun` to the streaming dialect, whose `transcribe` returns `UnsupportedOperation`; the README's `volcengine` example was impossible too (that dialect requires `api_url`) | Modality gateway friction | **Fixed** (2026-09-22, issue #258) | — | The gateway is usable from application code — `Registry` exposes `.id("provider/model")` — but provider-only selection silently picks `default_for_provider`, which for ASR is streaming-only. Demo now filters on provider **and** model; README documents which dialects this batch tool can drive. |
| 14 | Live TTS sends `voice: None` as an empty voice, which every dialect's provider rejects | API friction | **Fixed** in the demo (2026-09-22, issue #258); SDK doc gap stays post-1.0 | [#300](https://github.com/jianantian/orchest/issues/300) | Observed: aliyun `Request voice is invalid!`, minimax `invalid params, empty field`, volcengine `403` / `resource ID is mismatched with speaker related resource`. Demo now passes `BRIEFING_DESK_TTS_VOICE`. SDK-side: `SynthesizeRequest.voice` has no documented meaning for `None`, and no dialect substitutes a default — a doc fix at minimum. |

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

**All four fixed as of hotfix 2026-07-02** (issues 001–005, closing
#195–#199). Kept below for the historical record of what was required and
why; see the Triage table above for the as-fixed detail and commit links.

1. ~~Add a small, deliberate public API surface for seeding a run with
   multimodal content~~ — **done**: `RunInput` (`AgentRun::start(config,
   RunInput, ..)`), `RunInput::text(..)`/`.with_image(..)`/`.from_blocks(..)`.
   (Finding #1 — [#195](https://github.com/jianantian/orchest/issues/195))
2. ~~Either ship a reusable fake `Asr`/`Tts`... or explicitly document that
   downstream crates are expected to write their own~~ — **done**:
   `orchest-provider::fakes` behind a `testing` feature. (Finding #2 —
   [#196](https://github.com/jianantian/orchest/issues/196))
3. ~~Give `AgentRun::resume` either a way to append new input directly, or —
   at minimum — a prominent doc comment...~~ — **done**: both, actually —
   `AgentRun::resume_with_input` for the follow-up-input path, plus
   `resume`'s rustdoc now states it doesn't append input, plus (finding #4)
   `resume`/`resume_with_input` now return `Result<_, ConfigError>` and
   reject with `SessionStoreMissing` instead of silently dropping
   persistence. (Findings #3, #4, #10 —
   [#197](https://github.com/jianantian/orchest/issues/197),
   [#198](https://github.com/jianantian/orchest/issues/198))
4. ~~Change `SubAgentBuilder::build()` to return `Result`~~ — **done**:
   `Result<Arc<dyn Tool>, ConfigError>` with `SubAgentMissingModel`/
   `SubAgentMissingRegistry`. (Finding #5 —
   [#199](https://github.com/jianantian/orchest/issues/199))

None of these required new provider adapters or new runtime concepts — all
four were narrow, targeted fixes to existing public surface, consistent with
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
