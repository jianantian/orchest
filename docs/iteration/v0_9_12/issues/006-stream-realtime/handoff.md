# Issue 006 — Handoff (COMPLETE)

_Last updated: 2026-06-30. Branch: `claude/pensive-curie-zz0t4v`._

Issue 006 is **functionally complete**: all five acceptance criteria are met and
every commit on the branch is green on the four gates. This records the final
state, the pattern used, and the one follow-up that belongs to Issue 008. Run
`git log --oneline` on the branch to see the per-slice commits.

---

## 1. Final state — every ASR/TTS/realtime path has a spine home

Selectable via `Registry::with_builtin()` → `reg.asr()/reg.tts()/reg.realtime()`,
each a full vertical (codec → loop → `Asr`/`Tts`/`RealtimeSession` impl → wall
entry), with the network-independent parts unit-tested:

| Capability | WS tier (`orchest-provider-stream`) | REST tier (`orchest-provider-http`) |
|---|---|---|
| **ASR** | volcengine · deepgram · soniox · aliyun · elevenlabs | assemblyai · speechmatics (batch) |
| **TTS** | volcengine · minimax · aliyun | — (all TTS dialects are WS) |
| **Realtime** | omni (Volcengine openspeech full-duplex) | — |

- `orchest-provider-stream`: `asr_entries()` = 5, `tts_entries()` = 3,
  `realtime_entries()` = 1 (omni).
- `orchest-provider-http`: `asr_entries()` = 2 (assemblyai, speechmatics).
- Wall selection tests: `crates/orchest-providers/tests/selection.rs` — stream-gated
  (16 incl. omni) + http-gated (assemblyai, speechmatics).

**Gate status (established gate, every commit):** `cargo fmt --check`,
`cargo clippy --workspace -- -D warnings`, `cargo test --workspace` (default,
stream feature off), `scripts/lint-check.sh` — all green. Stream crate 63 unit
tests; http crate 6 asr unit tests.

---

## 2. Acceptance criteria — all met

- [x] The openspeech binary protocol exists **once**, shared by asr/tts/omni —
      `orchest-provider-stream/src/openspeech.rs` (header + constants + gzip); the
      event-frame body codec in `tts/volcengine.rs`
      (`build_connect_frame`/`build_meta_frame`/`build_audio_frame`/`parse_frame`)
      is reused by omni.
- [x] Omni runs as `RealtimeSession` (`send`/pulled `events`); the **omni ruler**
      passes (`omni::tests::omni_ruler_audio_never_blocks_across_tool_use`), the
      live transport is `run_omni_session`, and it is wall-registered via
      `OmniSession::spawn_live` (sync factory spawns connect-then-run; connect
      failure → fatal `Error` on `events()`).
- [x] `agent-runtime-realtime-providers` is **deleted** (crate + workspace member +
      dead example); no provider-local `RealtimeError`/event enum remains.
- [x] ASR/TTS streaming behavior preserved (existing + new in-memory loop tests).
- [x] Every ASR/TTS path has a home — WS dialects in `-stream`, batch REST
      (assemblyai/speechmatics) in `-http`. No un-migrated provider path remains.

---

## 3. The pattern (for reference / future dialects)

**WS dialect** (`-stream`): one module with a pure `parse_*`/`map_*`
(`ServerEvent` → `Vec<StreamEvent>`), frame builders, a
`run_*_stream<T: ByteDuplex>` loop (generic over the `WsFrame` binary+text duplex,
so it's testable in-memory with a channel-backed `ByteDuplex` — no network), the
`Asr`/`Tts` impl that connects + spawns the loop, and
`entry_descriptor()`/`from_provider_config()` wired into `*_entries()`. References:
`asr/deepgram.rs` (all-text), `asr/aliyun.rs` (text control + binary audio),
`tts/volcengine.rs` (openspeech binary), `omni.rs` (full-duplex + handshake).

**Batch REST** (`-http`): one module implementing one-shot `Asr::transcribe` over
`crate::http::shared_client()` (submit → poll → fetch), with the pure
request-build + response-assemble functions unit-tested and the live HTTP as thin
glue; `start_stream` → `UnsupportedOperation`. References: `asr/assemblyai.rs`
(bytes upload), `asr/speechmatics.rs` (multipart job + json-v2 assembly).

---

## 4. Follow-up — belongs to Issue 008 (cleanup), NOT 006

The old `agent-runtime-asr-providers` and `agent-runtime-tts-providers` crates are
now **redundant** — every provider has a spine home — but still exist as workspace
members. Their deletion (and any remaining `AsrError`/`TtsError` removal) is Issue
008 cleanup, the same way the realtime crate's deletion was 006's. Before deleting
them, confirm nothing else imports them
(`grep -rn "agent_runtime_asr_providers\|agent_runtime_tts_providers"`).

---

## 5. Gotchas / lessons (paid for already)

- **Server vs client frame types.** `parse_frame` only decodes *server* openspeech
  frames (`MSG_FULL_SERVER_RESPONSE` / `MSG_AUDIO_ONLY_RESPONSE`); the `build_*`
  builders produce *client* frames. In tests, fabricate server frames with the
  right msg type (see `omni.rs` `server_meta_frame`/`server_audio_frame`) — a
  client builder makes `await_lifecycle` loop forever (silent hang, not a failure).
- **`pkill -f "cargo test"` kills its own shell** (the script command line matches).
  To kill a hung test binary, match `target/debug/deps/<crate>-` instead.
- **Build times under disk pressure** (~6 G free): run **one** `cargo test` at a
  time — parallel invocations contend on the target-dir lock and look like a hang.
  Use `timeout 200 cargo test … --lib <module>` to isolate.
- **The established clippy gate is `cargo clippy --workspace -- -D warnings`** — NOT
  `--all-targets`. `--all-targets` surfaces pre-existing `result_large_err` lints in
  the test fixtures that the project does not gate on; don't add it.
- **Public fn over a private type** → `private_interfaces` error: keep helper fns
  that take module-private structs non-`pub` (e.g. `assemble_transcript`).
- New provider modules need their `#[allow(clippy::result_large_err)]` on every
  fallible fn / entry closure, matching the workspace convention.

---

## 6. Running the gates

```sh
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace                              # default (no impl features)
cargo test -p orchest-provider-stream               # WS dialects (63 tests)
cargo test -p orchest-provider-http asr::           # batch REST asr (6 tests)
cargo test -p orchest-providers --features stream   # WS wall selection tests
cargo test -p orchest-providers --features http     # REST wall selection tests
bash scripts/lint-check.sh
```
