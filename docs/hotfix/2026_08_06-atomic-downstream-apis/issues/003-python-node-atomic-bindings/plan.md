# Python and TypeScript Atomic Bindings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose completion, one-shot ASR, and realtime ASR symmetrically in Python and TypeScript.

**Architecture:** Focused native modules convert arguments and call only `orchest`/`orchest-provider` wall APIs. Thin Python/TypeScript public wrappers own naming and callback-exception mediation; realtime start/send/wait are async while finish is synchronous and idempotent.

**Tech Stack:** PyO3 0.28, pyo3-async-runtimes, napi-rs 2, TypeScript, Python typing, Rust tokio.

## Global Constraints

- Both bindings enable `orchest-provider`'s `asr` feature and never name implementation crates.
- Omitted one-shot provider resolves exactly to `aliyun/qwen-audio-3.0-asr-flash`.
- Callbacks are synchronous, serial, and callback errors are rethrown by `wait()` rather than becoming Node fatal exceptions.
- Existing binding `lib.rs` files only assemble modules and exports.
- Tests precede production changes and run red before green.

---

## Files to Read

- `crates/orchest-py/src/lib.rs`
- `crates/orchest-node/src/lib.rs`
- `python/orchest/__init__.py`
- `js/index.ts`
- `docs/guide/python.md`
- `docs/guide/typescript.md`

### Task 1: Focused native completion bindings

**Files:**
- Create: `crates/orchest-py/src/atomic.rs`
- Create: `crates/orchest-node/src/atomic.rs`
- Modify: both binding `lib.rs` and Cargo feature declarations.

**Interfaces:**
- Python: `complete(model, system, user, api_key=None, api_key_env=None, api_url=None, json_mode=False, retry=False, request_options=None) -> str`.
- Node native: `complete(options: NativeCompletionOptions) -> Promise<string>`.

- [ ] **Step 1: Add failing native argument/default tests**

```rust
let cfg = completion_provider_config("deepseek/deepseek-chat", None, Some("KEY"), None, None);
assert_eq!(cfg.api_key_env.as_deref(), Some("KEY"));
```

Assert JSON mode maps `ResponseFormat::JsonObject` and retry maps `RetryPolicy::recommended()`.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-py atomic && cargo test -p orchest-node atomic`
Expected: modules/functions do not exist.

- [ ] **Step 3: Implement thin adapters over `orchest::atomic::complete`**

```rust
let model = create_adapter_from_config(provider_config)?;
orchest::atomic::complete(model.as_ref(), request).await
```

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-py atomic && cargo test -p orchest-node atomic`
Expected: PASS.

### Task 2: One-shot ASR native bindings

**Files:**
- Create: `crates/orchest-py/src/asr.rs`
- Create: `crates/orchest-node/src/asr.rs`
- Modify: both binding Cargo manifests to enable `orchest-provider/asr`.

**Interfaces:**
- Python: `transcribe(audio, format, language=None, provider=DEFAULT, api_key=None, api_key_env=None, api_url=None, options=None) -> str`.
- Node native: `transcribe(audio: Uint8Array, options: NativeTranscribeOptions) -> Promise<string>`.

- [ ] **Step 1: Add failing parsing/default tests**

```rust
assert_eq!(parse_audio_format("m4a").unwrap(), AudioFormat::M4a);
assert_eq!(resolve_asr_id(None), "aliyun/qwen-audio-3.0-asr-flash");
assert!(validate_audio(&[]).is_err());
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-py asr && cargo test -p orchest-node asr`
Expected: parsing/helpers are missing.

- [ ] **Step 3: Construct through the wall and call `Asr::transcribe`**

```rust
let entry = Registry::with_builtin().asr().id(provider).select()?;
let asr = entry.create(&provider_config)?;
let result = asr.transcribe(request).await?;
Ok(result.text)
```

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-py asr && cargo test -p orchest-node asr`
Expected: PASS.

### Task 3: Realtime session native handles

**Files:**
- Create: `crates/orchest-py/src/asr_stream.rs`
- Create: `crates/orchest-node/src/asr_stream.rs`
- Modify: both binding `lib.rs` module registrations.

**Interfaces:**
- `start_asr_stream` / `startAsrStream`: async and resolves after provider `task-started`.
- `AsrStream.send_audio` / `sendAudio`: async bounded-channel send.
- `finish`: synchronous idempotent sender close.
- `wait`: async completion; repeated calls return the stored result.

- [ ] **Step 1: Add failing state-machine tests with fake handles**

```rust
session.finish().unwrap();
session.finish().unwrap();
assert!(session.send_bytes(Bytes::from_static(b"late")).await.is_err());
```

Cover empty chunks, ordering under channel saturation, provider fatal error, repeated wait, and drop finish.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-py asr_stream && cargo test -p orchest-node asr_stream`
Expected: session wrappers do not exist.

- [ ] **Step 3: Implement shared state semantics in focused modules**

```rust
enum SessionState { Open(Sender<SessionInput>), Finishing, Closed(Result<(), ProtocolError>) }
```

The event pump invokes one callback at a time. Native delivery errors become session errors; public-language callback exceptions are mediated in Task 4.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-py asr_stream && cargo test -p orchest-node asr_stream`
Expected: PASS.

### Task 4: Public Python/TypeScript wrappers and type declarations

**Files:**
- Modify: `python/orchest/__init__.py`, `python/orchest/__init__.pyi`
- Modify: `python/orchest/exceptions.py`, `python/orchest/exceptions.pyi` if structured fields need extending.
- Modify: `js/index.ts`, `js/index.d.ts`, `js/index.js`, `js/native.d.ts`
- Test: `python/tests/test_atomic_api.py`
- Test: `js/tests/atomic-api.test.ts` or the repository's existing type-smoke command.

**Interfaces:**
- Public callbacks return only `None`/`void`; coroutine/Promise values are rejected.
- Wrappers store the first callback exception, call finish, suppress later callbacks, and rethrow the same exception from wait.

- [ ] **Step 1: Add failing import/type and callback-wrapper tests**

```python
assert callable(complete)
assert inspect.iscoroutinefunction(start_asr_stream)
```

```typescript
const stream = await startAsrStream(options, () => { throw marker; });
await expect(stream.wait()).rejects.toBe(marker);
```

- [ ] **Step 2: Verify RED**

Run: `uv run pytest python/tests/test_atomic_api.py && npm run typecheck`
Expected: exports/types do not exist.

- [ ] **Step 3: Implement thin wrappers and synchronized declarations**

```typescript
let callbackError: unknown;
const nativeCallback = (event: AsrStreamEvent): void => {
  try { onEvent(event); } catch (error) { callbackError ??= error; native.finish(); }
};
```

Python implements the same first-error/finish/wait behavior and rejects awaitable callback returns.

- [ ] **Step 4: Verify GREEN**

Run: `uv run pytest python/tests/test_atomic_api.py && npm run typecheck`
Expected: PASS.

### Task 5: Build/install guides and package smoke tests

**Files:**
- Modify: `docs/guide/python.md`
- Modify: `docs/guide/typescript.md`
- Modify package test/build scripts only where required for the new exports.

- [ ] **Step 1: Add exact local build/install commands**

```bash
uv run maturin develop --manifest-path crates/orchest-py/Cargo.toml
npm run build:native
npm pack
```

Document CPython 3.12 ABI rebuild, uv path/editable use, tarball installation, and a three-API smoke snippet for each SDK.

- [ ] **Step 2: Run binding and package verification**

Run: `cargo test -p orchest-py && cargo test -p orchest-node && uv run pytest python/tests && npm test && npm run typecheck`
Expected: PASS.

- [ ] **Step 3: Run workspace verification and commit Issue 003**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
bash scripts/lint-check.sh
git add crates/orchest-py crates/orchest-node python js docs/guide
git commit -m "feat: expose atomic provider APIs in Python and TypeScript"
```

