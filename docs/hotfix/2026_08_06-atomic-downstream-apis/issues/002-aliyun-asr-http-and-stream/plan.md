# Aliyun HTTP and Realtime ASR Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Aliyun one-shot HTTP transcription and repair realtime request mapping and application-level startup handshake.

**Architecture:** A focused HTTP adapter builds and parses the multimodal-generation dialect; both HTTP and WS reuse validation helpers for simplified context. The WS driver gates its public handle on `task-started`, while the wall continues to register models by catalog identity.

**Tech Stack:** Rust, reqwest, base64, serde_json, tokio channels/oneshot, in-memory `ByteDuplex` fixtures.

## Global Constraints

- HTTP and WS impls remain below the `orchest-provider` wall.
- HTTP `sample_rate` is a JSON string; WS `sample_rate` is a JSON integer.
- The existing realtime Aliyun registry default remains unchanged.
- No live credentials, OSS upload, transcoding, sample probing, or `continue-task`.
- Tests precede production changes and run red before green.

---

## Files to Read

- `docs/external/aliyun/asr/non-realtime/api.md`
- `docs/external/aliyun/asr/realtime/client-event.md`
- `crates/orchest-provider-http/src/asr/assemblyai.rs`
- `crates/orchest-provider-stream/src/asr/aliyun.rs`
- `crates/orchest-provider/src/registry.rs`

### Task 1: Shared audio-format and context contract

**Files:**
- Modify: `crates/orchest-protocol/src/stream.rs`
- Create: `crates/orchest-provider-core/src/aliyun_asr.rs`
- Modify: `crates/orchest-provider-core/src/lib.rs`
- Modify exhaustive `AudioFormat` matches reported by `cargo check --workspace`.

**Interfaces:**
- Produces: `AudioFormat::{M4a, Aac}`.
- Produces: `parse_context(options: &Value) -> Result<Option<Vec<Value>>, ProtocolError>` and reserved-option helpers usable by HTTP and WS.

- [ ] **Step 1: Add failing serde and context-validation tests**

```rust
assert_eq!(serde_json::to_value(AudioFormat::M4a).unwrap(), json!("m4a"));
assert!(parse_context(&json!({"context":[{"role":"assistant","text":"orphan"}]})).is_err());
```

Cover role counts, user-before-assistant ordering, Unicode character count, 400-character round limit, and secret-free errors.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-protocol audio_format && cargo test -p orchest-provider-core aliyun_asr`
Expected: new variants/module are missing.

- [ ] **Step 3: Implement minimal typed validation**

```rust
pub struct ContextMessage { pub role: ContextRole, pub text: String }
pub fn parse_context(options: &Value) -> Result<Vec<ContextMessage>, ProtocolError> {
    let raw = options.get("context").cloned().unwrap_or_else(|| json!([]));
    let messages: Vec<ContextMessage> = serde_json::from_value(raw)
        .map_err(|error| ProtocolError::new(ErrorCode::InvalidRequest, error.to_string()))?;
    let (mut users, mut assistants, mut pending_user_chars) = (0, 0, None);
    for message in &messages {
        match message.role {
            ContextRole::User => {
                if pending_user_chars.is_some() { return Err(invalid_context_order()); }
                users += 1;
                pending_user_chars = Some(message.text.chars().count());
            }
            ContextRole::Assistant => {
                let user_chars = pending_user_chars.take().ok_or_else(invalid_context_order)?;
                assistants += 1;
                if user_chars + message.text.chars().count() > 400 { return Err(context_too_long()); }
            }
        }
    }
    if users > 5 || assistants > 5 { return Err(context_too_many_messages()); }
    if pending_user_chars.is_some_and(|chars| chars > 400) { return Err(context_too_long()); }
    Ok(messages)
}
```

- [ ] **Step 4: Verify GREEN and exhaustive matches**

Run: `cargo test -p orchest-provider-core aliyun_asr && cargo check --workspace`
Expected: PASS.

### Task 2: Aliyun synchronous HTTP ASR

**Files:**
- Create: `crates/orchest-provider-http/src/asr/aliyun.rs`
- Modify: `crates/orchest-provider-http/src/asr/mod.rs`
- Modify: `crates/orchest-provider-http/Cargo.toml`

**Interfaces:**
- Produces: `AliyunAsr`, `AliyunAsrConfig`, `from_provider_config`, `build_request_body`, and `parse_response`.

- [ ] **Step 1: Write failing fixture tests**

```rust
let body = build_request_body("qwen-audio-3.0-asr-flash", &request).unwrap();
assert_eq!(body["parameters"]["sample_rate"], "16000");
assert!(body["input"]["messages"].last().unwrap()["content"][0]["input_audio"]["data"]
    .as_str().unwrap().starts_with("data:audio/mp4;base64,"));
```

Add AAC MIME, language hints, vocabulary/context order, full endpoint preservation, response metadata, missing text, HTTP status, and unsupported streaming tests.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-provider-http asr::aliyun`
Expected: module does not exist.

- [ ] **Step 3: Implement request, response, and transport**

```rust
let response = shared_client().post(&config.api_url)
    .bearer_auth(&config.api_key)
    .header("X-DashScope-SSE", "disable")
    .json(&build_request_body(&config.model, &request)?)
    .send().await?;
```

Errors keep status/provider/model but never include authorization or Base64 bodies.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-provider-http asr::aliyun`
Expected: PASS.

### Task 3: Catalog and wall registration

**Files:**
- Modify: `crates/orchest-provider-http/src/catalog/asr.rs`
- Modify: `crates/orchest-provider-http/src/lib.rs`
- Modify: `crates/orchest-provider/tests/selection.rs`

**Interfaces:**
- Produces IDs `aliyun/qwen-audio-3.0-asr-flash` and `aliyun/fun-asr-flash-2026-06-15`.

- [ ] **Step 1: Add failing catalog/selection tests**

```rust
let entry = Registry::with_builtin().asr()
    .id("aliyun/qwen-audio-3.0-asr-flash").select().unwrap();
assert!(!entry.descriptor.streaming);
```

Assert both HTTP rows are non-default and `aliyun/fun-asr-realtime` remains the sole Aliyun default.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-provider selection --features asr`
Expected: unknown HTTP model.

- [ ] **Step 3: Add rows and the `aliyun` HTTP factory arm**

```rust
"aliyun" => Box::new(asr::aliyun::from_provider_config(&pinned)?),
```

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-provider --features asr`
Expected: PASS.

### Task 4: Realtime request mapping and startup gate

**Files:**
- Modify: `crates/orchest-provider-stream/src/asr/aliyun.rs`

**Interfaces:**
- Produces: run-task context under `payload.input.context` and a ready signal that resolves only on `task-started`.

- [ ] **Step 1: Add failing mapping and ordering tests**

```rust
input_tx.send(SessionInput::Audio(Bytes::from_static(b"pcm"))).await.unwrap();
assert!(out_rx.try_recv().is_err());
in_tx.send(WsFrame::Text(event("task-started", None, false))).await.unwrap();
assert!(matches!(out_rx.recv().await.unwrap(), WsFrame::Binary(_)));
```

Also assert context placement, integer sample rate, `task-failed` before ready, malformed start event, and early EOF.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-provider-stream asr::aliyun`
Expected: audio is emitted before `task-started` and context is under parameters.

- [ ] **Step 3: Split startup from active drive**

```rust
let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
tokio::spawn(run_aliyun_stream(
    WsDuplex::new(ws_stream), run_task, finish_task, input_rx, events_tx, ready_tx,
));
ready_rx.await.map_err(|_| startup_closed_error())??;
```

The driver receives server frames exclusively until `task-started`, then begins selecting audio and server events.

- [ ] **Step 4: Verify GREEN and ASR regression suite**

Run: `cargo test -p orchest-provider-stream asr::aliyun && cargo test -p orchest-provider-http && cargo test -p orchest-provider --features asr`
Expected: PASS.

- [ ] **Step 5: Commit Issue 002**

```bash
git add crates/orchest-protocol crates/orchest-provider-core crates/orchest-provider-http crates/orchest-provider-stream crates/orchest-provider
git commit -m "feat: add Aliyun atomic ASR APIs"
```
