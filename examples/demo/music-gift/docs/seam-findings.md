# Seam Findings — Orchest SDK gaps surfaced by music-gift

This demo exists to exercise the Orchest SDK's public surface and record where
that surface is missing or mis-shaped. Each finding below names the API, the
concrete need that hit the gap, a classification, and — where we fix it — the
design. Findings are for the SDK maintainers, not the demo.

---

## Finding 1 · `GenTask` results can't express structured timed text (aligned lyrics)

**API surface:** `orchest_protocol::capability::GenResult` / `GenTask`
**Classification:** API redesign (seam blocker for any timed-text consumer)
**Status:** fix designed, this document

### The need

The gift's scrolling-lyrics view needs per-line timing: which lyric line is sung
at each moment of the audio. Suno exposes exactly this via
`POST /api/v1/generate/get-timestamped-lyrics`, returning `alignedWords` — each
segment with `startS` / `endS` in seconds (real forced alignment against the
rendered audio).

### The gap

`GenResult` offers only one place to carry lyrics — a single field:

```rust
pub struct GenResult {
    pub assets: Vec<GenAsset>,          // Url | Bytes
    pub diagnostic_metadata: Value,     // untyped escape hatch
    pub lrc: Option<String>,            // ← the problem
}
```

`lrc: Option<String>` is leaky on two axes at once:

1. **Modality leak.** `GenResult` is the shared result for image / video / music
   generation (the `GenTask` trait doc even says "image/video"). `lrc` is
   music-only, yet every image and video result carries it.
2. **Format leak.** LRC is one *subtitle file format* (a presentation concern).
   Hard-coding `lrc: String` decides, at the protocol layer, that timed text may
   only ever be LRC — and pre-serializing to a string throws away the structure
   (`[(text, start, end)]`) that a consumer would need to render SRT, VTT, or a
   custom karaoke highlight.

**Dead-field evidence.** Across the whole workspace there are **11 construction
sites** of `GenResult`, and **every one sets `lrc: None`** — the four music
providers (mureka, minimax_music, aliyun_music, suno) *and* all five visual
providers (volcengine, renderful, crazyrouter, aliyun, volcengine_video) *and*
provider-core. No provider has ever populated this field. It is a speculative
field that shipped, polluted the shared type for every modality, and was never
honored. This is the "patch-shaped design" the demo is meant to catch: a field
was bolted on for a one-off need and left unfulfilled.

**Escape-hatch abuse (related, not fixed here).** Because the typed surface has
no room for them, Suno's `duration_secs` and `cover_url` are stuffed into
`diagnostic_metadata: Value` — structured, first-class product metadata living
in an untyped bag named "diagnostic". Recorded as **Finding 2** below; out of
scope for this change by decision.

### Why it matters — measured

Same 31.84 s song, our text-only estimate (divide duration evenly by line,
weighted by section) vs. Suno's real alignment:

| lyric line | text estimate | Suno real alignment |
|---|---|---|
| 晨光爬上窗台 | **0.00 s** | **11.01 s** |
| 你还在睡 | 7.96 s | 16.84 s |
| 新的一天来了 | 15.92 s | 21.65 s |
| 带着微笑醒来 | 23.88 s | 26.01 s |

The estimate pins line 1 at 0.00 s, but the vocal doesn't start until an 11 s
intro — the whole track is misaligned from the first line, and intervals are
uneven besides. The structured provider data fixes this; the flattened `String`
field could not carry it in a usable form.

### The fix — a modality-neutral, structured timed-text type

Replace the leaky field with a generic, structured one. Timed text is a
cross-modality concept: ASR transcripts, TTS word timings, and sung-lyric
alignment all produce it.

```rust
/// A sequence of text segments each carrying a time span — the structured
/// form behind aligned lyrics, ASR transcripts, and TTS word timings.
/// Generic across modalities; consumers render it to LRC / SRT / VTT / a
/// custom UI. Granularity (word vs line) is whatever the provider supplies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimedText {
    pub segments: Vec<TimedSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimedSegment {
    /// Provider-verbatim text. May carry the provider's own markup (e.g. Suno
    /// returns `[Verse 1 — tender]\n晨光…`). The SDK does not clean it — the
    /// consumer owns presentation cleanup, so the protocol stays faithful.
    pub text: String,
    pub start: f64,       // seconds
    /// Not all sources supply an end (some ASR word timings give only start,
    /// and LRC rendering needs only start). `None` = unknown; a consumer that
    /// needs a span (SRT/VTT) can infer it from the next segment's start.
    pub end: Option<f64>,
}

pub struct GenResult {
    pub assets: Vec<GenAsset>,
    pub diagnostic_metadata: Value,
    pub timed_text: Option<TimedText>, // replaces `lrc`
}
```

Design commitments:

- **Structured, not pre-formatted.** The protocol carries `[(text, start, end?)]`;
  the consumer owns the render-to-format decision. LRC becomes one rendering,
  chosen in the demo, not in the SDK.
- **Granularity-neutral.** `segments` holds whatever the provider gives — Suno
  returned line-level chunks here; a word-level provider stores words. The demo
  aggregates/renders.
- **Faithful text, consumer cleans.** `TimedSegment.text` is provider-verbatim,
  markup and all; the SDK does no business cleanup. The demo's line-based parser
  (already strips `[Verse]` headers and inline `[Whispered]` tags) does it at
  render time.
- **Provider fills, consumer judges quality.** The provider maps whatever
  `alignedWords` it gets into `TimedText` and does **not** apply a quality gate.
  Overall confidence (`hootCer`) and per-word `success` go into
  `diagnostic_metadata`; the consumer reads them and decides whether to trust the
  alignment or fall back. Keeps the quality threshold out of the provider (no
  business decision baked into the SDK layer).
- **Per-segment `success` intentionally dropped.** Suno's per-word `success`
  bool is not modeled on `TimedSegment` (YAGNI for a render); the aggregate
  `hootCer` in `diagnostic_metadata` is enough for the consumer's trust decision.
- **Corresponds to the primary track.** `timed_text` aligns the primary product
  (`assets[0]`). Suno returns multiple track variants; grouping per-variant
  assets+timing is a separate gap (not this change), so the single `Option`
  tracks the primary audio.
- **Diagnostics stay diagnostic.** Alignment confidence (`hootCer`) belongs in
  `diagnostic_metadata`, not in the product type — kept separate on purpose.

### Layering — where each concern lives

- **Protocol** (`capability.rs`): defines `TimedText`; owns the structured shape.
- **Provider** (`suno.rs`): `fetch` calls `get-timestamped-lyrics` and maps
  `alignedWords` → `TimedText` (and puts `hootCer` into `diagnostic_metadata`).
  The secondary HTTP call is an implementation detail of `fetch`; it does **not**
  leak into the `GenTask` submit→poll→fetch lifecycle (deliberately not extending
  the trait — that would be over-design for one provider's secondary asset).
  Cost note: `fetch` now makes one extra read-only request per call; it's
  idempotent, and `fetch` is normally invoked once, so this is acceptable.
- **Consumer** (music-gift `handle_done` + `lrc.rs`): if `timed_text` is present
  (and `hootCer` in `diagnostic_metadata` is acceptable), renders it → LRC text;
  otherwise — instrumental, endpoint failure, `None`, or confidence below the
  consumer's threshold — falls back to the existing text-estimate `generate_lrc`.
  The threshold lives here, not in the provider.

### Blast radius

| Layer | File(s) | Change |
|---|---|---|
| Protocol | `crates/orchest-protocol/src/capability.rs` | add `TimedText`/`TimedSegment`; `GenResult.lrc` → `timed_text` |
| Providers (10 dead `None`) | http mureka/minimax_music/aliyun_music; visual volcengine/renderful/crazyrouter/aliyun/volcengine_video; provider-core `gen.rs` | `lrc: None` → `timed_text: None` (mechanical — the unavoidable reach of renaming a field on a shared struct) |
| Provider (real fill) | `crates/orchest-provider-http/src/gen/suno.rs` | `fetch`: call `get-timestamped-lyrics`, map `alignedWords` → `TimedText` |
| Consumer | `examples/demo/music-gift/src/tools/music_gen.rs`, `src/lrc.rs` | render `timed_text` → LRC; keep text-estimate fallback |

### Verification plan

- Protocol + demo unit tests compile; existing lrc-render tests stay green.
- One real Suno generation end-to-end: assert `timed_text` is populated, the
  rendered LRC's first line starts at the true vocal onset (≈11 s in the sample,
  not 0), and the frontend LRCViewer highlights the correct active line during
  playback.
- Instrumental / no-alignment path falls back to the text estimate without error.

---

## Finding 2 · `diagnostic_metadata: Value` is a dumping ground for typed product data

**API surface:** `GenResult.diagnostic_metadata`
**Classification:** API redesign (post-fix backlog)
**Status:** recorded, not fixed (scope decision)

Suno's `duration_secs` and `cover_url` are first-class product outputs but ride
in the untyped `diagnostic_metadata` bag because the typed surface has no home
for them. A future change should give `GenAsset` semantic roles (audio / cover /
timed-text) and lift duration into typed metadata, at which point
`diagnostic_metadata` returns to holding only genuinely diagnostic data. Left out
of the Finding 1 change to keep that change focused and its blast radius minimal.

---

## Finding 3 · Calling a tool once requires hand-constructing `ToolContext`

**API surface:** `orchest::tool::Tool::execute` / `orchest::tool::ToolContext`
**Classification:** missing SDK helper ("call a tool once")
**Status:** recorded, not fixed

The countdown subagent is invoked outside any agent run — a plain "call this
tool with this input, once" (`src/tools/countdown.rs:153-163`). But `execute`
takes a `&ToolContext`, so the caller must mint a throwaway context by hand:
`RunId::new()`, `ApprovalBus::default()`, `run_depth: 0`, a made-up
`tool_call_id`, `event_tx: None`, default budget, empty `parent_messages`.
None of these are meaningful for a one-shot call; all of them are forced.
A `ToolContext::oneshot()` (or a `Tool::call(input)` convenience that builds
the trivial context internally) would remove the boilerplate and, more
importantly, keep callers from having to know which fields are safe to fake.

---

## Finding 4 · `ToolContext` is too hard to construct in tests

**API surface:** `orchest::tool::ToolContext`
**Classification:** missing SDK helper (test ergonomics)
**Status:** recorded, not fixed

Because there is no easy way to build a `ToolContext` in a unit test, the
`collect_info` test (`src/tools/collect_info.rs:108`) gives up on executing
the tool and instead **copies the validation body into the test module**,
testing the copy instead of the real callback. The two implementations can
now drift silently — the test would keep passing if the tool's actual logic
changed. Finding 3's helper would fix this too: any supported way to execute
a tool outside a run lets tests exercise the real code path.

---

## Finding 5 · `ChatModel` vs `ModelAdapter`: one trait, two names, a manual cast

**API surface:** `orchest::model::ModelAdapter` / `orchest_protocol::ChatModel`
**Classification:** API ergonomics (redundant alias leaks into call sites)
**Status:** recorded, not fixed

`ModelAdapter` is an alias for `ChatModel` (see the note at
`src/config.rs:70-72`), yet the boundary is not transparent to callers:
`AgentRun::start` takes `Arc<dyn ModelAdapter>`, so a held
`Arc<dyn ChatModel>` needs an explicit `as Arc<dyn ModelAdapter>` cast
(`src/agent.rs:216`). An alias that requires a cast at use sites is an alias
in name only. Either the runtime should accept `Arc<dyn ChatModel>` directly
(or a generic `Into`/`From`), or the alias should be collapsed to a single
canonical name so no conversion is ever needed.
