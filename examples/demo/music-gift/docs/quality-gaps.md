# Quality Gaps vs bitwize-music

This documents the remaining quality pipeline differences between our single-call
music-gift agent and bitwize-music's multi-agent album workflow. Every item below
is a deliberate skip — not an oversight.

## Closed (Done in feat/music-gift-demo)

| Capability | bitwize-music | Our Implementation |
|---|---|---|
| Lyrics write | `lyric-writer` (SKILL.md + craft-reference + examples) | `prompts/lyrics.md` — tables, checklists, methodology adapted from source |
| Suno prompt | `suno-engineer` (genre-practices, v5-best-practices) | `prompts/music_prompt/suno.md` — Performance Cues, Exclude Styles, 8-dim JSON |
| Pronunciation | Advisory table in lyric-writer | **Mandatory** phonetic spelling in output, auto-fix in review pass |
| Performance Cues | Required per-section delivery tags | Enforced by both `suno.md` (writer) and `review.md` (reviewer) |
| Exclude Styles | Dedicated field in suno-engineer | Integrated into `EnrichedPrompt.exclude` + suno.md guidance |
| Artist names | `artist-blocklist.md` scanned by pre-generation-check | `lyrics_validator.rs` — 14-name blocklist checked before Suno submit |
| Independent review | `lyric-reviewer` (separate Claude agent, 14-point, auto-fix pronunciation) | `run_review_pass()` — second `AgentRun`, 10-point checklist, auto-fix cues + pronunciation |

## Open — Architecture-Level

These require multi-step orchestration the bitwize-music system does across
separate agent invocations with file-based state. Our single-turn gift context
doesn't need them, but they are the natural next-steps for a full album pipeline.

### 1. `pronunciation-specialist` (Dedicated Homograph Agent)

**What bitwize-music does:** A **separate Claude call** with its own system prompt
that scans the lyrics line-by-line and asks the user to resolve every ambiguous
homograph one at a time. Results go into a Pronunciation Notes table, then the
`lyric-reviewer` verifies they were applied.

**Why we skip it:** Our review pass auto-fixes unambiguous homographs and flags
ambiguous ones in a summary table. True interactive resolution requires a
multi-turn dialogue the gift flow doesn't have. The reviewer's auto-fix covers
~80% of cases (past-tense "read"→"red", adjective "live"→"lyve", etc.).

**Future:** If we build an album creation tool, this is the first agent to add.

### 2. `pre-generation-check` (6-Gate Blocker)

**What bitwize-music does:** A **haiku-model agent** (cheapest tier) that checks
6 gates before Suno generation:

| Gate | Check |
|---|---|
| Sources Verified | Only for documentary albums |
| Lyrics Reviewed | Lyrics box populated, no template placeholders |
| Pronunciation Resolved | All table entries applied, no unresolved homographs |
| Explicit Flag | Set to Yes/No for distribution metadata |
| Style Box Complete | Has vocals, section tags, not bloated (>12 descriptors) |
| Artist Names Cleared | No blocked names in style prompt |

Fails block generation. No fixes — report-only.

**Why we skip it:** We already cover 5 of 6 inline:
- Sources: N/A (no documentary mode)
- Lyrics: parse_lyrics checks has_lyrics
- Pronunciation: review pass auto-fixes
- Explicit: N/A (all our tracks are clean)
- Style Box: EnrichedPrompt has structural validation
- Artist Names: lyrics_validator checks before submit

**Future:** Worth extracting as a dedicated validation module if we ship a
public API where the submitter isn't the generator.

### 3. `lyric-refiner` (3-Pass Tighten → Cohesion → Unity)

**What bitwize-music does:** After the reviewer flags issues, the refiner
actually rewrites: Tighten (cut filler), Cohesion (cross-track consistency),
Unity (album-level arc). Each pass re-runs the 13-point check.

**Why we skip it:** The cohesion and unity passes are album-level concepts
(vocabulary drift across tracks, thematic progression, energy pacing). For a
single song gift, the Tighten pass is the only one that applies — and our
review pass already handles the most impactful fixes (pronunciation, cues).

**Future:** Add a single-pass Tighten agent if free-mode users consistently
report verbose/draft-quality lyrics. The album-level passes belong in a
separate album builder product.

### 4. Genre-Specific Defaults

**What bitwize-music does:** `suno-engineer/genre-practices.md` has per-genre
prompt patterns, instrument lists, and common gotchas (e.g., "for punk: keep
sections short and punchy, avoid reverb").

**Why we skip it:** Our style palette is curated to the gift use-case
(warm acoustic, gentle ballad, dreamy indie, etc.). Most map to Pop/Folk
defaults which are well-covered by the existing guidance.

**Future:** If we add user-selectable genre from the frontend, worth
incorporating per-genre instrument/mood defaults.

## Summary

For the single-song gift use-case, the pipeline is at parity with
bitwize-music on all **single-track quality levers**. The remaining gaps are
album-level orchestration features that don't apply to our scope.

The key architectural validation — **independent review agent** — is
already running as a second `AgentRun` and producing measurable quality
improvements (performance cues, pronunciation fixes, vocal gender correction).
