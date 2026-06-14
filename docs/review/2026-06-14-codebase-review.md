# Code Review: Orchest Agent Runtime — 2026-06-14

Full-repo review covering all crates, JS/TS SDK, and Python SDK.

---

## 🔴 Critical

### 1. `unwrap()` in production run loop — `crates/agent-runtime-core/src/run/actor.rs:607`

```rust
let delay = super::retry::compute_delay(
    retry_attempt,
    &e,
    state.config.retry_policy.as_ref().unwrap(),
);
```

`unwrap()` is banned in library code per AGENTS.md unless an invariant is explicitly documented in a comment. The invariant here is that `should_retry` (line 603) returned `true`, which checks `retry_policy.is_some()` internally. This is cross-function coupling: a refactor of `should_retry` that returns `true` without checking `is_some()` causes a panic.

**Fix:** Either add `// INVARIANT: should_retry only returns true when retry_policy is Some` at the call site, or restructure to `if let Some(policy) = state.config.retry_policy.as_ref() { compute_delay(..., policy) }`.

---

### 2. `ToolContext` cloned on every tool call — `crates/agent-runtime-core/src/tool/in_process.rs:73-85`

```rust
async fn execute(&self, input: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
    let ctx_owned = ToolContext {
        run_id: ctx.run_id,
        run_depth: ctx.run_depth,
        tool_call_id: ctx.tool_call_id.clone(),
        event_tx: ctx.event_tx.clone(),
        webhook_base_url: ctx.webhook_base_url.clone(),
        approval_bus: ctx.approval_bus.clone(),
        remaining_budget: ctx.remaining_budget.clone(),
        parent_messages: ctx.parent_messages.clone(),  // O(N) clone of entire message history
    };
    (self.callback)(input, ctx_owned).await
}
```

The `ToolCallback` type (`in_process.rs:12-19`) takes `ToolContext` by value, but the `Tool` trait's `execute` passes `&ToolContext`. This mismatch forces a full clone of `ToolContext` on every in-process tool call. `parent_messages: Vec<Message>` can be large — this is O(N) allocation per tool invocation.

**Fix:** Change `ToolCallback` to accept `&ToolContext`:

```rust
pub type ToolCallback = Arc<
    dyn Fn(Value, &ToolContext) -> Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send>>
        + Send + Sync,
>;
```

Then remove the `ctx_owned` clone. Callers that need owned fields can clone selectively.

---

### 3. Webhook server 16 KB buffer truncation — `crates/agent-runtime-core/src/run/webhook.rs:43,61-63`

```rust
let mut buffer = vec![0; 16 * 1024];
// ...
loop {
    // ...
    if read_total == buffer.len() {
        break;  // drops incomplete request without responding
    }
}
```

Payloads > 16 KB cause the read loop to break before the full HTTP body arrives. `httparse` returns `Status::Partial`, the spawned task returns without calling `waiter.send`. In `poll_async_job` (`tool_exec.rs:68`), the webhook timeout is silently swallowed (`Ok(Err(_)) | Err(_) => {}`), and execution falls through to the polling loop. Consequences:

- **Fresh jobs** (`poll: Some`): polling eventually succeeds. No visible error, but ~3× `poll_interval` extra latency with zero diagnostic.
- **Deserialized jobs** (`poll: None`): returns `"async job has no polling fallback"` — wrong error; the real problem is the webhook payload was too large.

**Fix:** Grow the buffer dynamically, or use `hyper`. At minimum, return `413 Payload Too Large` and log a warning when truncation occurs. Also fix `tool_exec.rs:68` to log the `RecvError`/timeout instead of silently discarding it (see issue 9).
---

### 4. Unnecessary heap allocation in handoff — `crates/agent-runtime-core/src/tool/handoff_tool.rs:55`

```rust
HandoffTarget::Static(config) => *config.clone(),
```

`HandoffTarget::Static` holds `Box<AgentConfig>`. In this match arm, `config` is `&Box<AgentConfig>`. `config.clone()` allocates a new `Box<AgentConfig>` on the heap, then `*` immediately moves the inner `AgentConfig` out and drops the `Box`. This is a heap allocation + immediate free.

**Fix:**

```rust
HandoffTarget::Static(config) => (**config).clone(),
```

This dereferences through `&Box<AgentConfig>` to `&AgentConfig` and clones directly — zero heap allocation.

---

### 5. TypeScript declaration file out of sync — `js/index.d.ts` vs `js/index.ts`

`js/index.ts` emits `{ type, run_depth, child_run_id }` on nearly every `RuntimeEvent` variant. `js/index.d.ts` is an older snapshot that differs in three ways:

1. **Missing `run_depth` and `child_run_id`** on most variants (only `run_aborted` and `events_dropped` retain them in `.d.ts`).
2. **Different field types** — e.g. `tool_call_started.metadata` is `ToolMetadata` in `.d.ts` but `unknown` in `index.ts`; `tool_call_failed.error` is `ToolExecutionError` in `.d.ts` but `unknown` in `index.ts`.
3. **Missing variants** — `index.ts` has 28 variants; `.d.ts` has the same names but reads like a v0.1-era snapshot.

TypeScript consumers using the `.d.ts` file will see `run_depth` and `child_run_id` absent from their autocomplete and type-checking, and field types that disagree with what the runtime actually emits.

**Fix:** Delete `index.d.ts` and generate it from `index.ts` via `tsc --declaration`, or make `index.ts` the single source of truth and drop the hand-maintained `.d.ts`.
---

## 🟡 Conventions

### 6. `expect()` in library code without documented invariants

AGENTS.md: `expect()` banned in library code except where an invariant is explicitly documented in a comment.

| File | Line | Expression |
|------|------|------------|
| `run/supervisor.rs` | 265 | `.expect("SupervisorActor spawn failed")` |
| `tool/agent_as_tool.rs` | 339 | `.expect("SubAgentBuilder requires .model() before .build()")` |
| `tool/agent_as_tool.rs` | 342 | `.expect("SubAgentBuilder requires .registry() before .build()")` |
| `run/actor.rs` | 1256 | `.expect("event_subs always has at least one subscriber")` |
| `run/llm_watcher.rs` | 57 | `self.model.expect("LlmWatcherBuilder requires a model")` |

**Fix:** For the builder cases (`agent_as_tool.rs`, `llm_watcher.rs`), return a `Result` with a proper error type instead of panicking. For `supervisor.rs:265` and `actor.rs:1256`, add an `// INVARIANT:` comment documenting why the expect is unreachable.

---

### 7. File size far exceeds 400-line guideline

AGENTS.md: "consider splitting if a file exceeds ~400 lines."

| File | Lines |
|------|-------|
| `run/actor.rs` | 1292 |
| `providers/anthropic.rs` | 1627 |
| `tool/builtin.rs` | 442 (borderline) |
| `tool/code_exec.rs` | 436 (borderline) |
| `aigc-providers/gateway.rs` | 787 |
| `tts-providers/routing.rs` | 704 |

`actor.rs`'s `run_one_step` function alone spans lines 426–1202 (777 lines). The per-variant tool-result handling (`ToolOutput::Immediate`, `ToolOutput::Structured`, `ToolOutput::AsyncJob`, `ToolOutput::Handoff`) repeats the same pattern of emit → record → finalize_after_tool with slight differences.

**Fix:** Extract per-variant handler functions (e.g. `handle_immediate_tool_result`, `handle_async_job_result`) into a submodule. Ditto `anthropic.rs` — split streaming response handling and tool-use mapping into separate modules.

---

### 8. `model/mod.rs` contains a type alias — `crates/agent-runtime-core/src/model/mod.rs:11`

```rust
pub type ModelStreamChunk = StreamEvent;
```

AGENTS.md: "mod.rs only re-exports and declares submodules — keep logic in the subfiles." This alias is minor but lives in the wrong place.

**Fix:** Move to `agent-runtime-model/src/types.rs` or into the `pub use` block as a re-export from model.

---

## 🟠 Design & Performance

### 9. Silent error swallowing on oneshot channels — `run/actor.rs:808` and `run/tool_exec.rs:68`

Two sites discard `RecvError`/`Timeout` on `oneshot` channels without logging:

**Site A — `actor.rs:808`** (approval):
```rust
let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
    Ok(result) => result.unwrap_or(false),  // RecvError → denied, no log
    Err(_) => { /* timeout → abort */ }
};
```

**Site B — `tool_exec.rs:68`** (webhook callback):
```rust
Ok(Err(_)) | Err(_) => {}  // RecvError or timeout → silent fallthrough to polling
```

Site A masks race conditions between `ApprovalBus::cancel` and timeout. Site B is hit by the webhook buffer overflow (issue 3), degrading silently into polling fallback with zero diagnostic.

**Fix:** Log a warning in both cases:

```rust
// Site A
Ok(result) => match result {
    Ok(approved) => approved,
    Err(_) => {
        tracing::warn!(run_id = %run_id, "approval sender dropped before response");
        false
    }
}

// Site B
Ok(Err(e)) => tracing::warn!(?e, "webhook oneshot sender dropped"),
Err(_) => tracing::warn!("webhook response timed out after {:?}", wait_for),
```
---

### 10. Raw HTTP parsing in webhook server — `run/webhook.rs:66-112`

The webhook server does manual byte-level HTTP parsing with `httparse`. Missing protections:

- No `Content-Type` validation
- No concurrent connection limiting (each connection spawns an unbounded task)
- No graceful shutdown beyond `AbortHandle::abort()` (in-flight connections are killed mid-response)
- Error paths return no body (confusing for webhook senders)

For v0.4+, consider replacing with `hyper` or `axum` for a lightweight but standards-compliant HTTP server.

---

### 11. `InProcessTool::new` — 6 positional arguments — `tool/in_process.rs:32`

```rust
#[allow(clippy::too_many_arguments)]
pub fn new(
    name: String,
    description: String,
    input_schema: JsonSchema,
    output_schema: Option<JsonSchema>,
    metadata: ToolMetadata,
    callback: ToolCallback,
) -> Self { .. }
```

Marked with `#[allow(clippy::too_many_arguments)]`. The comment says "builder would be over-engineering." Six positional args with three `String`/`Option` types next to each other is error-prone at call sites (`new("desc", "name", ...)` compiles silently with swapped args).

**Fix:** Add a builder. It does not need to be complex — just field-by-field `.name()`, `.description()`, etc., with `.build()` validating required fields. This is a public API.

---

## 🟢 Minor

### 12. `BareSubprocessExecutor::new()` duplicates `Default` — `skill/executor.rs:54-58`

```rust
#[derive(Debug, Default)]
pub struct BareSubprocessExecutor;

impl BareSubprocessExecutor {
    pub fn new() -> Self { Self }
}
```

`new()` returns a unit struct, identical to `Default::default()`. Having both paths is noise.

**Fix:** Remove `new()` or make it delegate:

```rust
pub fn new() -> Self { Self::default() }
```

---

### 13. ~~`canonical_json` re-encodes per object — `hook/loop_detection.rs:149-170`~~ → **WITHDRAWN**

~~For each tool call, the loop detection hook serializes the input to JSON via `serde_json::to_string` to produce a canonical hash.~~ Incorrect. The `canonical_json` function at `hook/loop_detection.rs:149-170` is a custom recursive formatter that manually builds key-sorted JSON strings with `format!` — it does NOT re-serialize through serde. The string allocation is inherent to the hashing approach and no evidence suggests it's a hot path. Removed from findings.


### 14. `unwrap_or_default()` on system clock — `session/sqlite.rs:53`
```rust
.unwrap_or_default().as_secs()
```

`SystemTime::now().duration_since(UNIX_EPOCH)` returning `Err` means the system clock is pre-1970. This is effectively unreachable, but a `tracing::warn!` would aid forensic debugging if it ever triggers.

### 15. `concat!` for multi-line prompt template — `prompts.rs:16-29`

```rust
pub const COMPACTION_SUMMARY_PROMPT: &str = concat!(
    "Below is a transcript...",
    "while preserving...",
    ...
);
```

Works correctly at compile time, but editing is cumbersome. If the project gains an `indoc` dependency, consider using it for readability. Not worth a new dependency today.

### 16. `std::sync::Mutex` in async test fake — `guardrail/tests.rs:189`

```rust
*self.seen_input.lock().unwrap() = Some(input.clone());
```

Inside `Tool::execute` (an async method), uses `std::sync::Mutex`. In a test fake this is harmless (held briefly), but the pattern could be copied into production code. Prefer `tokio::sync::Mutex` for anything inside async context.

---

## ✅ Positive Findings

- **Clean crate separation**: `agent-runtime-model` is a proper leaf crate with zero deps on other workspace crates. Core depends on model; providers depend on model but not core; bindings depend on core and providers. No circular deps.
- **Systematic error types**: Every module uses `thiserror` — `ToolError`, `ModelError`, `SessionError`, `HandoffError`, `ScriptError`, `McpError`, `RegistryError`, `ConfigError`, `ScanError`, `EnvError`.
- **Correct `spawn_blocking` usage**: `SqliteSessionStore` wraps all `rusqlite` calls in `tokio::task::spawn_blocking`. `BareSubprocessExecutor` uses `tokio::process::Command`. No blocking I/O in async contexts.
- **`JobHandle` serialization**: Custom `Serialize`/`Deserialize` impls properly skip the `poll: Option<Arc<PollFn>>` closure field. Cross-process restore caveat documented in type-level doc comment.
- **Guardrail → Hook adapter**: Clean separation — guardrails decide, hook adapters apply. `InputGuardrailHook`, `OutputGuardrailHook`, etc. wrap the trait impls correctly. Test coverage exists for all four guardrail points.
- **`InMemorySessionStore` JSON round-trip**: Saves by serializing to JSON and deserializing back, catching `serde(skip)` field loss and schema version issues that would only manifest in persistent backends.
- **Tool search**: Trigram + Jaccard similarity is a pragmatic approach with well-defined scoring and no external NLP dependency.
- **Hook panic recovery**: `hook/runner.rs` wraps every hook call in `AssertUnwindSafe` + `catch_unwind`, emitting `HookPanicked` events so a single misbehaving hook never crashes the run.
- **ApprovalBus tree sharing**: Root run and all sub-agents share a single `ApprovalBus` via `Arc<AsyncMutex<HashMap>>`, enabling parent → child approval routing through a single channel.

---
## Summary

| Severity | Count | Key items |
|----------|-------|-----------|
| Critical | 5 | unwrap in prod (actor:607), ToolContext clone perf (in_process:73), webhook 16KB truncation (webhook:43), redundant Box alloc (handoff_tool:55), TS declarations out of sync |
| Convention | 3 | expect() without invariant docs (5 sites), file size (2 files >> 400 lines), mod.rs logic |
| Design | 3 | silent error swallowing on oneshot channels (2 sites), raw HTTP parsing, 6-arg constructor |
| Minor | 4 | empty new() duplicating Default, system clock unwrap_or_default, concat! prompt, sync Mutex in async test |

**One finding withdrawn:** `canonical_json` in loop_detection (originally claimed serde re-serialization; actually a custom formatter with no evidence of hot-path concern).
