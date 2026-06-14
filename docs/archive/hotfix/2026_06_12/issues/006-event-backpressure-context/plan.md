# 006 · Event Backpressure 与 Context Window 防御 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Prevent event subscribers from blocking the run loop indefinitely and fail early when estimated context exceeds the configured model window.

**Architecture:** Keep the current mpsc subscriber model for the hotfix. Add a bounded timeout to primary event sends and reuse existing `model.spec.context_window_size` instead of adding new config.

**Tech Stack:** Rust, Tokio mpsc/time, existing tokenizer/compaction utilities.

---

## 要读的现有代码

- `crates/agent-runtime-core/src/run/actor.rs`
- `crates/agent-runtime-core/src/tokenizer.rs`
- `crates/agent-runtime-core/src/run/compaction.rs`
- `crates/agent-runtime-core/src/events.rs`
- `crates/agent-runtime-core/src/run/tests.rs`

## 文件改动

- Modify/Test: `crates/agent-runtime-core/src/run/actor.rs`
- Optional modify: `crates/agent-runtime-core/src/tokenizer.rs` only if no existing helper can estimate tool definitions
- Test: `crates/agent-runtime-core/src/run/tests.rs`

## 步骤

### 1. Event send timeout 常量

- [ ] Add near actor module constants:

```rust
const EVENT_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
```

- [ ] Ensure `tokio::time::timeout` is imported or fully qualified.

### 2. Primary subscriber send 加 timeout

- [ ] Update `async fn emit(subs: &[mpsc::Sender<RuntimeEvent>], event: RuntimeEvent)`.
- [ ] Replace primary `.send(event.clone()).await` with:

```rust
if let Some(primary) = subs.first() {
    match tokio::time::timeout(EVENT_SEND_TIMEOUT, primary.send(event.clone())).await {
        Ok(Ok(())) | Ok(Err(_)) => {}
        Err(_) => {
            if primary.try_send(RuntimeEvent::EventsDropped {
                subscriber_id: 0,
                count: 1,
            }).is_err() {
                tracing::warn!("primary event subscriber timed out and EventsDropped notification channel is full");
            }
        }
    }
}
```

- [ ] Keep secondary subscribers as `try_send`.
- [ ] Preserve existing behavior that sends `EventsDropped` to primary when secondary drops, but add a `tracing::warn!` fallback when the primary channel is full.

### 3. Event backpressure regression test

- [ ] Add test that subscribes a bounded primary receiver and intentionally does not drain it.
- [ ] Trigger enough events to fill the channel.
- [ ] Wrap the awaited run completion in `tokio::time::timeout(Duration::from_secs(2), handle.wait())`.
- [ ] Assert the run does not hang.
- [ ] For a primary-full timeout test, assert only that the run does not hang; `EventsDropped` is best effort and may not fit in the full channel.
- [ ] Add a separate secondary-full test where primary has spare capacity; fill a secondary subscriber, emit an event, then assert primary receives `RuntimeEvent::EventsDropped { .. }`.

### 4. Context window estimate before model call

- [ ] Locate model call path just before `ModelCallStarted` / adapter completion in `run_one_step`.
- [ ] Compute estimated message tokens using existing tokenizer logic. Prefer existing compaction helper if available; otherwise add a small private helper in `actor.rs`:

```rust
fn estimate_tool_defs_tokens(tool_defs: &[ToolDef]) -> u64 {
    serde_json::to_string(tool_defs)
        .map(|s| (s.len() as u64).div_ceil(4))
        .unwrap_or(0)
}
```

Only use `.unwrap_or(0)` for estimate fallback; do not panic on serialization failure.

- [ ] Check:

```rust
if let Some(context_window_size) = state.config.model.spec.context_window_size {
    if estimated_tokens > context_window_size {
        emit(&subs, RuntimeEvent::RunFailed {
            error: format!(
                "context window exceeded: estimated {estimated_tokens} tokens, limit {context_window_size}"
            ),
        }).await;
        return false;
    }
}
```

- [ ] Do not add `AgentConfig.context_window`.
- [ ] Do not trigger proactive compaction in this hotfix.

### 5. Context window regression test

- [ ] Add test with `config.model.spec.context_window_size = Some(1)`.
- [ ] Provide an input/messages/tool-def setup whose estimate exceeds 1.
- [ ] Assert emitted events contain `RunFailed { error }` and `error.contains("context window exceeded")`.
- [ ] Assert fake model adapter was not called. If existing fake adapter cannot expose call count, add a test-only counter to the fake in `run/tests.rs`.

### 6. 验证

```bash
cargo test -p agent-runtime-core event
cargo test -p agent-runtime-core context
cargo test -p agent-runtime-core
cargo clippy -p agent-runtime-core -- -D warnings
cargo fmt --check
```
