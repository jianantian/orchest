# Briefing Desk

Briefing Desk is a local, multimedia research-brief agent built on the Orchest
SDK. It is the v0.10 validation demo — see
[`docs/iteration/v0_10/prd.md`](../../../docs/iteration/v0_10/prd.md) for why it
exists: the goal is to dogfood the runtime's public API and the ASR/TTS/vision
provider gateways through one small, real product before v1.0 freezes those
surfaces.

> **Status**: this issue (001) only defines the product spec and fixture corpus.
> No runtime code exists yet — the CLI, tools, and provider wiring land in
> issues 002-005. Running `cargo run` here does nothing until then.

## What it does

Given a directory of mixed research materials and a question, Briefing Desk:

1. Transcribes any recorded audio interviews in the corpus (ASR).
2. Reads any chart/screenshot images in the corpus through a vision-capable model
   (multimodal image input).
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

- **From `chart.png`** (vision path): the three retention values by quarter —
  38%, 40%, 42% — and that the trend is upward. This chart's Q3 number (42%)
  matches `001`, not the conflicting 35% figure in `002`; the brief should use the
  chart to help identify which written source it corroborates.
- **From `interview.wav`** (ASR path): the interviewee's answer to "What would
  happen if Loom disappeared tomorrow?" — a specific line about the team reverting
  to spreadsheets and losing about two hours a day, described as "the real return
  on investment nobody puts in a slide deck." This line exists **only** in the
  audio; `005-interview-followup-notes.md` explicitly avoids repeating it verbatim
  so the ASR path is not optional for producing a correct brief.

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

This exercises the full pipeline offline: materials discovery, a fake ASR
transcript per audio source, a fake vision description per image, a canned
model response, writing the brief, and a fake TTS audio file (add `--no-tts`
to skip that last step). `cargo test -p briefing-desk-demo` runs this same
path as an automated smoke test.

### Live provider run and resume flow

Not yet available. Live model/ASR/TTS wiring lands in issue 005
([#192](https://github.com/jianantian/orchest/issues/192)); the `resume`
subcommand currently parses its arguments but returns a stub error — real
session persistence and resume land in issue 004
([#191](https://github.com/jianantian/orchest/issues/191)). This section will
be filled in as those land.
