# Issue 006 — Handoff

_Last updated: 2026-06-29. Branch: `claude/pensive-curie-zz0t4v`._

This picks up from `spec.md` / `plan.md`. It records exactly what is **done**, the
**pattern** every remaining piece should follow, and the **three** remaining
slices with concrete file pointers and decisions. Read this, then `git log
--oneline` on the branch to see the per-slice commits.

---

## 1. Status at a glance

**Live through the wall** (selectable via `Registry::with_builtin()` →
`reg.asr().provider(..)` / `reg.tts().provider(..)`), each a full vertical
(codec → `run_*_stream` loop → `Asr`/`Tts` impl → wall entry), unit-tested
in-memory:

| Capability | Providers |
|---|---|
| **ASR** | volcengine · deepgram · soniox · aliyun · elevenlabs |
| **TTS** | volcengine · minimax · aliyun |

`orchest-provider-stream/src/lib.rs` registers **8 entries** (`asr_entries()` = 5,
`tts_entries()` = 3). `realtime_entries()` still returns `Vec::new()` — omni is
**not wall-registered yet** (see §4.1).

**Omni full-duplex** — the whole session is built and tested, just not yet wired
to the wall:
- `map_server_event` + `OmniSession` (`RealtimeSession` impl) + the **omni ruler
  test** (audio in / audio+text out / mid-stream tool use, audio never blocks). ✓
- **`run_omni_session<T: ByteDuplex>`** — the network-independent live driver:
  openspeech handshake + full-duplex loop, unit-tested in-memory over a crossed
  `ByteDuplex` peer pair. ✓

**Gate status:** every commit on this branch is green on all four gates
(`fmt --check`, `clippy --workspace -D warnings`, `cargo test --workspace`,
`scripts/lint-check.sh`). Stream crate: 63 unit tests. Wall selection tests:
`crates/orchest-providers/tests/selection.rs` (one per live dialect).

---

## 2. Acceptance criteria — current state

- [x] The openspeech binary protocol exists **once**, shared by asr/tts/omni.
      → `crates/orchest-provider-stream/src/openspeech.rs` (header + field
      constants + gzip). The **event-frame body codec** (event i32 + length-prefixed
      session_id + payload/audio) lives in `tts/volcengine.rs`
      (`build_connect_frame` / `build_meta_frame` / `build_audio_frame` /
      `parse_frame` / `VolcengineFrame` / `EVENT_*`) and is **reused by omni** —
      omni is the same dialect, so it adds no second copy.
- [x] Omni runs as `RealtimeSession`; the omni ruler passes
      (`omni::tests::omni_ruler_audio_never_blocks_across_tool_use`).
- [ ] **`agent-runtime-realtime-providers` is deleted** — _remaining (§4.2)._
- [x] ASR/TTS streaming behavior preserved (existing tests green; the new
      in-memory loop tests cover each dialect).
- [ ] **Every ASR/TTS path has a home** — _remaining: speechmatics + assemblyai
      (batch REST → `orchest-provider-http`), §4.3._

---

## 3. The pattern (follow this for every remaining provider)

Each WS dialect is one module `src/asr/<p>.rs` or `src/tts/<p>.rs` with:

1. **Wire structs + a pure `parse_*` / `map_*`** (`ServerEvent` → `Vec<StreamEvent>`):
   provisional vs committed transcript via the dialect's "is_final" flag; errors
   → `StreamEvent::Error { fatal: true }`. Pure, fully unit-testable.
2. **Frame builders** (`build_*`) producing the client frames.
3. **`run_<p>_stream<T: ByteDuplex>(transport, …, input, events)`** — the
   transport-agnostic loop. **This is the keystone**: it is generic over
   `crate::transport::ByteDuplex` (the `WsFrame` binary+text duplex), so the whole
   protocol is testable in-memory with a channel-backed `ByteDuplex` — **no
   network**. The live `tokio-tungstenite` socket is the same `ByteDuplex` via
   `WsDuplex`.
4. **`struct <P>Asr/Tts` + `impl Asr/Tts`** — `start_stream` / `synthesize` does
   the auth handshake (header strategy), connects (`WsDuplex::new`), spawns the
   loop into a `RealtimeHandle` / `EventStream`. The non-streaming method that the
   dialect can't do is `ErrorCode::UnsupportedOperation`.
5. **`entry_descriptor()` + `from_provider_config(&ProviderConfig)`** + a line in
   `asr_entries()` / `tts_entries()` in `lib.rs`.
6. **A stream-gated wall test** in `orchest-providers/tests/selection.rs`:
   `reg.asr().provider("<p>").select()` resolves to the expected model.

`deepgram` (`src/asr/deepgram.rs`) is the cleanest reference for an all-text WS
dialect; `aliyun` (`src/asr/aliyun.rs` + `src/tts/aliyun.rs`) for text-control +
binary-audio; `volcengine` for the openspeech binary framing.

**Transport contract** (`src/transport.rs`): `WsFrame::{Binary(Vec<u8>),
Text(String)}`; `ByteDuplex::{send, recv}`. Audio is `Binary`, JSON control is
`Text` — match the real wire per dialect.

---

## 4. Remaining work (three slices, in recommended order)

### 4.1 Omni connect adapter + wall factory (`realtime_entries()`) — the headline

Everything network-independent is done. What's left is thin:

- **A live connect** that builds a `WsDuplex` from the four `X-Api-*` headers and
  spawns `run_omni_session`. Auth (from `agent-runtime-realtime-providers/.../live.rs`
  `build_realtime_request`): `X-Api-App-ID`, `X-Api-Access-Key`,
  `X-Api-Resource-Id`, `X-Api-App-Key`, `X-Api-Connect-Id`. WS URL default
  `wss://openspeech.bytedance.com/api/v3/realtime/dialogue`. Session config =
  `start_session_payload()` in the old `mod.rs` (asr/dialog/tts blocks).
- **The sync wall factory.** `Entry`'s factory is **sync**
  (`Fn(&ProviderConfig) -> Result<Box<dyn RealtimeSession>, ProtocolError>`), but
  connect is async. **Resolution (already validated by the trait shape):** the
  factory constructs an `OmniSession` that owns both channel ends and
  `tokio::spawn`s a connect-then-run task; a connect failure surfaces as a
  `StreamEvent::Error` on `events()` rather than a `Result` from the factory.
  `RealtimeSession` has no async `connect` method — `send`/`events`/`close` are
  the surface — so lazy/spawned connect is the intended path. Concretely: give
  `OmniSession` a live constructor that takes the input `Receiver` + an
  `EventStream` (the `RealtimeHandle` shape) and spawns
  `run_omni_session(WsDuplex::new(ws), session_id, config, input_rx, events_tx)`.
- **Credential plumbing:** `ProviderConfig` has `api_key` + `options:
  serde_json::Value`. Map `access_key = api_key`; pull `app_id` / `resource_id` /
  `app_key` from `options`. (`ProviderConfig` fields:
  `provider, model, api_key, api_url, max_tokens, options`.)
- Add the omni `Entry` to `realtime_entries()` (use `omni_descriptor("volcengine",
  "<model>")`); add a stream-gated `reg.realtime().provider("volcengine")` test to
  `selection.rs`.
- Update the stale `omni.rs` module doc (lines ~15–17 still say "live WS transport
  … layered on in a later slice" / "not registered through the wall yet").

### 4.2 Delete `agent-runtime-realtime-providers` (acceptance #3)

Once omni is wall-registered, nothing depends on it (it was only referenced from
`orchest-provider-stream` doc comments — verify with
`grep -rn "agent-runtime-realtime-providers\|agent_runtime_realtime_providers"`).
Delete the crate dir, remove it from the workspace `Cargo.toml` members, and drop
its `RealtimeError` / event enum. Confirm no provider-local `RealtimeError`
remains anywhere.

### 4.3 speechmatics + assemblyai → `orchest-provider-http` (batch REST)

These two are **not** WS dialects — they are batch REST (`submit` job → poll →
fetch transcript). They implement `Asr::transcribe` (one-shot) with `reqwest`,
`start_stream` → `UnsupportedOperation`, and belong in `orchest-provider-http`
(skeleton exists from Issue 004; create `src/asr/` there). Keep the **pure
response→`TranscribeResult` codec** separate and unit-tested; the live HTTP
submit/poll wraps it. Old sources: `agent-runtime-asr-providers/src/providers/
{speechmatics,assemblyai}/mod.rs`. Register via the http crate's entries through
the wall. (aliyun/elevenlabs ASR were WS and are already live; speechmatics and
assemblyai are the only batch-REST holdouts.)

---

## 5. Gotchas / lessons (paid for already — don't repeat)

- **Server vs client frame types.** `parse_frame` only decodes **server**
  openspeech frames (`MSG_FULL_SERVER_RESPONSE` / `MSG_AUDIO_ONLY_RESPONSE`). The
  `build_meta_frame` / `build_connect_frame` builders produce **client**
  (`MSG_FULL_CLIENT_REQUEST`) frames. In tests, fabricate server frames with the
  right msg type (see `omni.rs` test helpers `server_meta_frame` /
  `server_audio_frame`) — using a client builder for a server frame makes
  `await_lifecycle` loop forever (a silent test hang, not a failure).
- **`pkill -f "cargo test"` kills its own shell** (the script's command line
  contains the pattern). To kill a hung test binary, match
  `target/debug/deps/<crate>-` instead.
- **Build times under disk pressure.** The container runs ~6–7 G free; a rebuild
  touching a wide dependency (e.g. `tts/volcengine.rs`, which omni imports) plus a
  cold `cargo test --workspace` can take minutes. Run **one** `cargo test` at a
  time — parallel invocations contend on the target-dir lock and serialize, which
  looks like a hang. Use a `timeout 200 cargo test … --lib <module>` to isolate.
- **Clippy `-D warnings`** is a gate: `single_match` → `if let`, 6+ args →
  justified `#[allow(clippy::too_many_arguments)]`, fallible-frame builders keep
  the workspace `result_large_err` allow comment.

---

## 6. Running the gates

```sh
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace                              # default (no stream feature)
cargo test -p orchest-provider-stream               # the stream crate unit tests
cargo test -p orchest-providers --features stream   # wall selection tests
bash scripts/lint-check.sh
```

The default `cargo test --workspace` must stay at its current pass count (stream
is feature-gated off there); the stream-feature wall tests are where new dialects
prove they are selectable. Commit only when all four are green.
