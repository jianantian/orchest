# Briefing Desk

Briefing Desk is a local, multimedia research-brief agent built on the Orchest
SDK. It is the v0.10 validation demo — see
[`docs/archive/iteration/v0_10/prd.md`](../../../docs/archive/iteration/v0_10/prd.md) for why it
exists: the goal is to dogfood the runtime's public API and the ASR/TTS/vision
provider gateways through one small, real product before v1.0 freezes those
surfaces.

## What it does

Given a directory of mixed research materials and a question, Briefing Desk:

1. Transcribes any recorded audio interviews in the corpus via ASR
   (`orchest_protocol::Asr`, real or fake).
2. Describes any chart/screenshot images in the corpus. This is a fixed
   placeholder, not real vision-model input — see "Vision is not real" below
   for why, and `docs/archive/iteration/v0_10/validation-notes.md` for the finding.
3. Searches and reads the plain-text/Markdown materials.
4. Streams progress events to stdout as it works across all of the above.
5. Asks for approval before writing the final Markdown brief.
6. Optionally synthesizes an audio version of the brief (TTS), gated by a flag.
7. Persists the session so a follow-up question can resume it later.

The product is intentionally small in orchestration and broad in modality — see
the PRD for the reasoning. It must still work end-to-end and feel like a real
tool, not a snippet.

## Fixture corpus

`fixtures/research/` is a synthetic research corpus about a fictional internal
tool, "Loom," built around one question:

> "How is Loom's retention and competitive position looking this quarter — is it
> worth continuing to invest in for Q4?"

The corpus is deliberately inconsistent — conflicting numbers, one missing data
point, and one quote that only exists in audio — because a real research brief has
to reconcile exactly this kind of mess. See
[`fixtures/expected/report-shape.md`](fixtures/expected/report-shape.md) for the
exact brief structure a correct run should produce, and
[`fixtures/expected/validation-rubric.md`](fixtures/expected/validation-rubric.md)
for how findings during implementation get classified.

| File | Type | Role |
|------|------|------|
| `001-retention-dashboard-notes.md` | text | Analyst's finance-reconciled retention numbers by channel/quarter |
| `002-support-ticket-summary.md` | text | Support themes; cites a **conflicting** referral-retention number from an unreconciled pull |
| `003-competitor-scan.md` | text | Competitor notes; one competitor's pricing is explicitly **missing/TBD** |
| `004-roadmap-decision-log.md` | text | Prior planning decisions and an unsized budget item |
| `005-interview-followup-notes.md` | text | Notes from a customer interview; deliberately does **not** repeat the interview's exact quote |
| `chart.png` | image | Bar chart of referral-channel 30-day retention: 38% (Q1), 40% (Q2), 42% (Q3) |
| `interview.wav` | audio | ~12s recorded interview with the customer named in `005` |

### What the agent must extract (checkable)

- **From `chart.png`** (`describe_image` tool — see "Vision is not real"
  below): the three retention values by quarter — 38%, 40%, 42% — and that
  the trend is upward. This chart's Q3 number (42%) matches `001`, not the
  conflicting 35% figure in `002`; the brief should use the chart to help
  identify which written source it corroborates. Today this is a hardcoded
  description, not a real read of the pixels, because no live counterpart is
  possible yet (see below) — but the fixed text still asserts these specific
  numbers, so the "does the brief cite the chart's numbers" check is real.
- **From `interview.wav`** (`transcribe_audio` tool, real `Asr` trait,
  fake or live): the interviewee's answer to "What would happen if Loom
  disappeared tomorrow?" — a specific line about the team reverting to
  spreadsheets and losing about two hours a day, described as "the real
  return on investment nobody puts in a slide deck." This line exists
  **only** in the audio; `005-interview-followup-notes.md` explicitly avoids
  repeating it verbatim so the ASR path is not optional for producing a
  correct brief.

## Regenerating media fixtures

`chart.png` and `interview.wav` are committed, generated outputs — regenerate them
only if the underlying data in `001-retention-dashboard-notes.md` or the interview
script changes.

```bash
# chart.png — pure Python stdlib, portable to any OS with Python 3
python3 fixtures/scripts/gen_chart.py

# interview.wav — macOS only (uses the built-in `say` TTS engine so the fixture
# contains real, checkable speech). On other platforms, record or synthesize a
# replacement by hand using the same script text (see the file for details).
./fixtures/scripts/gen_interview_audio.sh
```

Both scripts write directly into `fixtures/research/`, overwriting the existing
file. Commit the regenerated binary alongside whatever fixture-content change
prompted it.

## Constraints

Briefing Desk is built exclusively against Orchest's public API surface: the core
runtime crate plus the public ASR, TTS, and AIGC/multimodal provider crates. It
must not reach into private modules, test-only helpers, or crate-internal fakes —
if a fake provider isn't reachable as a normal dependency, that inaccessibility is
itself a finding (see `fixtures/expected/validation-rubric.md`, "modality gateway
friction"), not something to work around with a private import.

## Running the demo

### Fake smoke run (no network credentials)

```bash
cargo run -p briefing-desk-demo -- run \
  --materials fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md \
  --fake
```

This exercises the full pipeline offline, in order: `search_fixtures` ->
`read_fixture` -> `transcribe_audio` -> `describe_image` -> `review_report`
-> `write_report` -> `synthesize_brief`. `transcribe_audio`/`describe_image`
only run if the corpus actually has audio/image sources (tool not registered
otherwise); `synthesize_brief` only runs unless `--no-tts`. Each step's
input is built from the real output of the steps before it — e.g. the ASR
transcript and the image description both get folded into the draft that
`review_report` and `write_report` see, so "the transcript feeds the brief"
is literally true and checked in `tests/smoke.rs`, not just claimed.

`review_report` is a lightweight reviewer sub-agent wired in through
`AgentConfig::as_tool` (Agent-as-Tool, `ContextMode::Fresh` — the reviewer
never sees the parent's conversation, only the draft it's asked to check).
Its verdict is forwarded to the parent's event stream (`[reviewer] ...` lines)
and gets appended into the written report.

`write_report` and `synthesize_brief` each independently require approval —
auto-approved unless `BRIEFING_DESK_FAKE_DENY_APPROVAL` (for the write) or
`BRIEFING_DESK_FAKE_DENY_TTS_APPROVAL` (for the synthesis) is set, so
"write approved, TTS denied" and "write denied" are both independently
testable. A denied `write_report` also skips `synthesize_brief` entirely —
there is nothing to synthesize.

`cargo test -p briefing-desk-demo` runs all of the above as automated smoke
tests, using `FakeAsr`/`FakeTts` (real `orchest_protocol::{Asr, Tts}` impls,
deterministic, no network).

### Session persistence and resume

Pass `--session <id>` to `run` to persist the session to
`.briefing-desk-sessions/<id>.sqlite3` (relative to the current directory).
A later `resume` — a genuinely separate process — loads that file, appends
the follow-up question, and continues the same run:

```bash
cargo run -p briefing-desk-demo -- run \
  --materials fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md \
  --session demo-1 \
  --fake

cargo run -p briefing-desk-demo -- resume \
  --session demo-1 \
  --question "Has anything changed about the retention numbers?" \
  --output /tmp/followup.md \
  --fake
```

`run` without `--session` is ephemeral (not resumable) — this is a deliberate
per-invocation choice, not a gap: `resume` requires `--session`, so a run
without one was never going to be resumable regardless. Resume re-registers
no materials tools (search/read/write/review); the follow-up is answered
directly from the persisted conversation history, which already contains
everything the original run read and wrote — the fake follow-up answer
literally quotes a snippet of the original brief pulled out of that history
to make the context-preservation checkable.

### Live provider run

ASR and TTS each have an independent, env-var-gated live path — set all three
of a modality's variables to use a real provider from the
`orchest_provider::Registry` instead of the fake:

```bash
# ASR: real transcription instead of FakeAsr
export BRIEFING_DESK_ASR_PROVIDER=volcengine
export BRIEFING_DESK_ASR_MODEL=<model id>
export BRIEFING_DESK_ASR_API_KEY=<key>

# TTS: real synthesis instead of FakeTts
export BRIEFING_DESK_TTS_PROVIDER=volcengine
export BRIEFING_DESK_TTS_MODEL=<model id>
export BRIEFING_DESK_TTS_API_KEY=<key>

cargo run -p briefing-desk-demo -- run \
  --materials fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md \
  --fake
```

`--fake` is still required — it governs the chat model only (still
`FakeModel`; no v0.10 issue wires a live chat provider into this CLI). ASR
and TTS liveness is controlled purely by the env vars above, independent of
`--fake`. This path is manual and untested by CI (no credentials/network in
this repo's test environment) — run it yourself and record the outcome
(provider, model, date, result) in the v0.10 validation report.

### Vision is not real

`describe_image` always returns a fixed, hardcoded description, in both
`--fake` and live mode — there is no live counterpart to flip on. Real
`ContentBlock::Image` input needs a way to seed a run's message history with
an image, but the only public entry point, `AgentRun::start`, takes a plain
`String`, and the method that does accept `Vec<Message>`
(`AgentRun::start_with_bus`) is `pub(crate)`; a tool can't inject an image
into the next model turn either, since `ToolResult.content` is hard-typed
`serde_json::Value`. There is currently no public Orchest API path to real
vision-through-agent-loop at all. Recorded as a release-blocker finding in
[`docs/archive/iteration/v0_10/validation-notes.md`](../../../docs/archive/iteration/v0_10/validation-notes.md)
rather than worked around by adding new surface to `orchest` itself. See also
the tracked issue: [#195](https://github.com/jianantian/orchest/issues/195).
