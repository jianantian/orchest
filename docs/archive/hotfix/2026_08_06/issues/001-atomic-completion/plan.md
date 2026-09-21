# Provider-neutral Atomic Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add typed JSON-object output and a provider-neutral single-turn completion helper without starting an Agent run.

**Architecture:** `orchest-protocol` owns the response-format option, HTTP dialects lower or reject it, and `orchest::atomic` owns message construction, retry, stop-reason validation, text extraction, and JSON-object validation. Bindings consume this helper in Issue 003.

**Tech Stack:** Rust, serde/serde_json, async-trait, tokio, existing `ChatModel` and runtime retry policy.

## Global Constraints

- No provider-specific logic in `orchest`; no agent run, tools, budget, or runtime events.
- `ResponseFormat::Text` is serde-compatible with payloads created before this field existed.
- Only `EndTurn` and `StopSequence` are successful atomic stop reasons.
- Tests precede production changes and run red before green.

---

## Files to Read

- `crates/orchest-protocol/src/options.rs`
- `crates/orchest-provider-http/src/chat.rs`
- `crates/orchest-provider-http/src/messages.rs`
- `crates/orchest/src/run/retry.rs`
- `crates/orchest-protocol/src/adapter.rs`

### Task 1: Typed response format

**Files:**
- Modify: `crates/orchest-protocol/src/options.rs`
- Modify: `crates/orchest-protocol/src/lib.rs`
- Modify: `crates/orchest/src/model/mod.rs`

**Interfaces:**
- Produces: `ResponseFormat::{Text, JsonObject}` and `RequestOptions.response_format`.

- [ ] **Step 1: Add failing serde tests**

```rust
#[test]
fn request_options_without_response_format_defaults_to_text() {
    let value = serde_json::json!({"thinking":"Off"});
    let options: RequestOptions = serde_json::from_value(value).unwrap();
    assert_eq!(options.response_format, ResponseFormat::Text);
}

#[test]
fn text_response_format_is_omitted_from_json() {
    let value = serde_json::to_value(RequestOptions::default()).unwrap();
    assert!(value.get("response_format").is_none());
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-protocol request_options_without_response_format_defaults_to_text`
Expected: compile failure because `ResponseFormat` and the field do not exist.

- [ ] **Step 3: Implement and re-export the enum**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum ResponseFormat { #[default] Text, JsonObject }

#[serde(default, skip_serializing_if = "ResponseFormat::is_text")]
pub response_format: ResponseFormat,
```

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-protocol request_options`
Expected: PASS.

### Task 2: Wire lowering and Messages rejection

**Files:**
- Modify: `crates/orchest-provider-http/src/chat.rs`
- Modify: `crates/orchest-provider-http/src/messages.rs`
- Test: existing provider request suites under `crates/orchest-provider-http/src/providers/`

**Interfaces:**
- Consumes: `RequestOptions.response_format`.
- Produces: Chat `response_format:{"type":"json_object"}` and pre-network Messages error code `unsupported_response_format`.

- [ ] **Step 1: Add failing request-shape and Messages-preflight tests**

```rust
let options = RequestOptions { response_format: ResponseFormat::JsonObject, ..Default::default() };
let (body, _) = adapter.request_body_for_test(&messages, &[], &options).unwrap();
assert_eq!(body["response_format"], json!({"type":"json_object"}));
```

```rust
let err = adapter.complete(&messages, &[], &options, None).await.unwrap_err();
assert_eq!(err.code.as_deref(), Some("unsupported_response_format"));
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest-provider-http response_format`
Expected: Chat body lacks the field and Messages does not reject it.

- [ ] **Step 3: Add minimal dialect behavior**

```rust
if effective_options.response_format == ResponseFormat::JsonObject {
    body["response_format"] = json!({"type": "json_object"});
}
```

Messages checks `JsonObject` before request construction and returns
`ModelError::internal("Messages protocol does not support json_object response format", "unsupported_response_format")`.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p orchest-provider-http response_format`
Expected: PASS.

### Task 3: Atomic completion helper

**Files:**
- Create: `crates/orchest/src/atomic.rs`
- Modify: `crates/orchest/src/lib.rs`
- Modify: `crates/orchest/src/run/retry.rs` only if crate-visible helpers need exposing.

**Interfaces:**
- Produces: `CompletionRequest { system, user, options, retry_policy }`.
- Produces: `pub async fn complete(model: &dyn ChatModel, request: CompletionRequest) -> Result<String, ModelError>`.

- [ ] **Step 1: Add a scripted fake and failing helper tests**

```rust
let text = complete(&fake, CompletionRequest {
    system: Some("system".into()), user: "user".into(),
    options: RequestOptions::default(), retry_policy: None,
}).await.unwrap();
assert_eq!(text, "hello world");
assert_eq!(fake.seen_tools(), 0);
```

Add separate tests for omitted empty system, ordered text blocks, each rejected stop reason, `StopSequence`, malformed/non-object JSON, retryable 429, and non-retryable protocol errors.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p orchest atomic::tests`
Expected: compile failure because `orchest::atomic` does not exist.

- [ ] **Step 3: Implement the minimal orchestration loop**

```rust
loop {
    match model.complete(&messages, &[], &request.options, None).await {
        Ok(response) => return validate_response(response, request.options.response_format),
        Err(error) => {
            if !should_retry(&classify(&error), attempt, &request.retry_policy) {
                return Err(error);
            }
            let Some(policy) = request.retry_policy.as_ref() else { return Err(error) };
            tokio::time::sleep(compute_delay(attempt, &error, policy)).await;
            attempt += 1;
        }
    }
}
```

- [ ] **Step 4: Verify GREEN and regression suite**

Run: `cargo test -p orchest atomic::tests && cargo test -p orchest-provider-http && cargo test -p orchest-protocol`
Expected: PASS.

- [ ] **Step 5: Commit Issue 001**

```bash
git add crates/orchest-protocol crates/orchest-provider-http crates/orchest
git commit -m "feat: add atomic completion helper"
```
