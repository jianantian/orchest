# Briefing Desk

Briefing Desk is a local, multimedia research-brief agent built on the Orchest
SDK. It is the v0.10 validation demo - see
[`docs/archive/iteration/v0_10/prd.md`](../../../docs/archive/iteration/v0_10/prd.md) for why it
exists: the goal is to dogfood the runtime's public API and the ASR/TTS/vision
provider gateways through one small, real product before v1.0 freezes those
surfaces.

## What it does

Given a directory of mixed research materials and a question, Briefing Desk:

1. Transcribes any recorded audio interviews in the corpus via ASR
   (`orchest_protocol::Asr`, real or fake).
2. Describes any chart/screenshot images in the corpus via a vision-capable
   chat model (`ModelAdapter::complete()` with `ContentBlock::Image`) - see
   "Vision" below.
3. Searches and reads the plain-text/Markdown materials.
4. Streams progress events to stdout as it works across all of the above.
5. Asks for approval before writing the final Markdown brief.
6. Optionally synthesizes an audio version of the brief (TTS), gated by a flag.
7. Persists the session so a follow-up question can resume it later.

The product is intentionally small in orchestration and broad in modality - see
the PRD for the reasoning. It must still work end-to-end and feel like a real
tool, not a snippet.

## Fixture corpus

`fixtures/research/` is a synthetic research corpus about a fictional internal
tool, "Loom," built around one question:

> "How is Loom's retention and competitive position looking this quarter - is it
> worth continuing to invest in for Q4?"

The corpus is deliberately inconsistent - conflicting numbers, one missing data
point, and one quote that only exists in audio - because a real research brief has
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

- **From `chart.png`** (`describe_image` tool - see "Vision" below): the
  three retention values by quarter - 38%, 40%, 42% - and that the trend is
  upward. This chart's Q3 number (42%) matches `001`, not the conflicting
  35% figure in `002`; the brief should use the chart to help identify which
  written source it corroborates. A vision-capable model (e.g. claude-sonnet)
  reads the actual pixels.
- **From `interview.wav`** (`transcribe_audio` tool, real `Asr` trait,
  fake or live): the interviewee's answer to "What would happen if Loom
  disappeared tomorrow?" - a specific line about the team reverting to
  spreadsheets and losing about two hours a day, described as "the real
  return on investment nobody puts in a slide deck." This line exists
  **only** in the audio; `005-interview-followup-notes.md` explicitly avoids
  repeating it verbatim so the ASR path is not optional for producing a
  correct brief.

## Regenerating media fixtures

`chart.png` and `interview.wav` are committed, generated outputs - regenerate them
only if the underlying data in `001-retention-dashboard-notes.md` or the interview
script changes.

```bash
# chart.png - pure Python stdlib, portable to any OS with Python 3
python3 fixtures/scripts/gen_chart.py

# interview.wav - macOS only (uses the built-in `say` TTS engine so the fixture
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
must not reach into private modules, test-only helpers, or crate-internal fakes -
if a fake provider isn't reachable as a normal dependency, that inaccessibility is
itself a finding (see `fixtures/expected/validation-rubric.md`, "modality gateway
friction"), not something to work around with a private import.

## Running the demo

### Basic run

```bash
# Set the chat model (required) and optionally an explicit API key
export BRIEFING_DESK_CHAT_MODEL=anthropic/claude-sonnet-4-6
export BRIEFING_DESK_CHAT_API_KEY=sk-ant-...
# Or omit the key - the provider's default env var is used (e.g. ANTHROPIC_API_KEY)

cargo run -p briefing-desk-demo -- run \
  --materials fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md
```

The agent drives the full pipeline: `search_fixtures` -> `read_fixture` ->
`transcribe_audio` -> `describe_image` -> `review_report` -> `write_report` ->
`synthesize_brief`. `transcribe_audio`/`describe_image` only run if the corpus
actually has audio/image sources (tool not registered otherwise);
`synthesize_brief` only runs unless `--no-tts`.

`review_report` is a lightweight reviewer sub-agent wired in through
`AgentConfig::as_tool` (Agent-as-Tool, `ContextMode::Fresh` - the reviewer
never sees the parent's conversation, only the draft it's asked to check).
Its verdict is forwarded to the parent's event stream (`[reviewer] ...` lines)
and gets appended into the written report.

`write_report` and `synthesize_brief` each independently require approval -
auto-approved by default. A denied `write_report` also skips
`synthesize_brief` entirely - there is nothing to synthesize.

Supported chat providers include `anthropic`, `openai`, `deepseek`,
`openrouter`, `volcengine`, `minimax`, and `aliyun`. See `.env.example` for
all `BRIEFING_DESK_CHAT_*` variables (API URL override, max tokens, etc.).

### Session persistence and resume

Pass `--session <id>` to `run` to persist the session to
`.briefing-desk-sessions/<id>.sqlite3` (relative to the current directory).
A later `resume` - a genuinely separate process - loads that file, appends
the follow-up question, and continues the same run:

```bash
cargo run -p briefing-desk-demo -- run \
  --materials fixtures/research \
  --question "Is Loom worth continued investment in Q4?" \
  --output /tmp/brief.md \
  --session demo-1

cargo run -p briefing-desk-demo -- resume \
  --session demo-1 \
  --question "Has anything changed about the retention numbers?" \
  --output /tmp/followup.md
```

`run` without `--session` is ephemeral (not resumable) - this is a deliberate
per-invocation choice, not a gap: `resume` requires `--session`, so a run
without one was never going to be resumable regardless. Resume re-registers
no materials tools (search/read/write/review); the follow-up is answered
directly from the persisted conversation history, which already contains
everything the original run read and wrote.

Under the hood this CLI's `resume` command calls the public
`AgentRun::resume_with_input(snapshot, RunInput::text(question), ..)`
(added by [#197](https://github.com/jianantian/orchest/issues/197)) rather
than hand-appending a message onto the loaded snapshot - see that function's
rustdoc for how it differs from the input-free `AgentRun::resume`. It also
re-attaches the session store before resuming; skipping that step now fails
loudly with `ConfigError::SessionStoreMissing` instead of silently dropping
persistence ([#198](https://github.com/jianantian/orchest/issues/198)).

### Live ASR and TTS

ASR and TTS each have an independent, env-var-gated live path - set all three
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
```

ASR/TTS liveness is independent of the chat model: you can mix live chat with
fake ASR, or any combination. This path is manual and untested by CI (no
credentials/network in this repo's test environment) - run it yourself and
record the outcome (provider, model, date, result) in the v0.10 validation
report.

### Vision

`describe_image` builds a real `ContentBlock::Image` (base64-encoded corpus
file) and drives a real `ModelAdapter::complete()` call - the public API path
added by [#195](https://github.com/jianantian/orchest/issues/195):
`AgentRun::start` now takes a `RunInput` (`RunInput::text(..).with_image(..)`
or `RunInput::from_blocks(..)`), and any tool that holds a model adapter can
do the same nested-call pattern `DescribeImageTool` uses in `media.rs`. The
same chat adapter constructed from `BRIEFING_DESK_CHAT_MODEL` is reused for
vision calls, so a genuinely live image description is exercised when the
configured model is vision-capable (e.g. claude-sonnet).

### Tests

`cargo test -p briefing-desk-demo` runs tool unit tests (no network needed)
plus integration smoke tests. Smoke tests that require a live LLM are skipped
when `BRIEFING_DESK_CHAT_MODEL` is not set; set it to run the full suite:

```bash
export BRIEFING_DESK_CHAT_MODEL=anthropic/claude-sonnet-4-6
cargo test -p briefing-desk-demo
```

ASR/TTS fakes (`orchest_provider::fakes::{FakeAsr, FakeTts}`) are used when
no live ASR/TTS env vars are set - see
[#196](https://github.com/jianantian/orchest/issues/196).

## Eval Lab (v0.16)

Briefing Desk hosts an application-local evaluation lab. Candidate experiments may
only edit harness surfaces in `src/harness.rs` (system prompts and Tool
descriptions). Tool schemas, execute bodies, runtime configuration, and fixtures
are not candidate surfaces.

The versioned corpus lives under `evals/`:

| Path | Role |
|------|------|
| `evals/cases.json` | 18 Loom-based cases (10 optimization / 4 validation / 4 scorecard) |
| `evals/session-seeds/*.json` | Synthetic read-only follow-up session seeds (messages/step/budget only) |

Corpus validation runs before any model call and enforces: unique case IDs, split
counts, scenario-family isolation across splits, validation coverage of all seven
behavior tags, fixture inventory membership, and session-seed content hashes.

All 18 cases are drawn from the existing Loom fixture corpus. Scorecards only
prove held-out performance within this pilot; they do not claim cross-domain
generalization.

### Sensitive run artifacts

Eval runs write under `evals/runs/<label>/` (manifest, harness + effective-config
snapshots, and per-attempt `trajectory.jsonl` / `output.md` / `attempt.json` /
`scores.json`). These artifacts are **sensitive by default**:

- Trajectories are allowlist-sanitized (stream chunks and nested thinking are
  dropped; secret-key fields are masked), but sanitized is **not** non-sensitive.
  Tool inputs/outputs, user questions, and generated report text still appear.
- Recording requires an explicit `--record-sensitive` confirmation. Without it,
  the eval runner refuses to start model calls.
- `evals/runs/` is gitignored. Do not commit, share, or publish run directories.
- Keep artifacts locally only as long as needed for compare/debug, then delete
  the label directory (`rm -rf evals/runs/<label>`). Prefer rotating labels over
  overwriting: an existing label is refused so baselines stay intact.
