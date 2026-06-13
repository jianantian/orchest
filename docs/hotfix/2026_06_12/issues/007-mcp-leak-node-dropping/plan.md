# 007 · MCP 子进程泄漏与 Node Event Dropping — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Ensure MCP stdio child processes are killed on drop even when the mutex is temporarily held, and make Node event callback backpressure observable.

**Architecture:** Keep `McpStdioClient` synchronous `Drop`; use an OS thread as fallback only when `try_lock` fails. For Node, detect `ThreadsafeFunction::call` failure and surface it via `EventsDropped` or warning.

**Tech Stack:** Rust std thread/mutex, Tokio process, napi-rs ThreadsafeFunction, tracing.

---

## 要读的现有代码

- `crates/agent-runtime-core/src/tool/mcp.rs`
- `crates/agent-runtime-node/src/lib.rs`
- `crates/agent-runtime-core/src/events.rs`

## 文件改动

- Modify/Test: `crates/agent-runtime-core/src/tool/mcp.rs`
- Modify: `crates/agent-runtime-node/src/lib.rs`
- Optional Test: Node binding unit test if napi test harness exists

## 步骤

### 1. MCP Drop fallback thread

- [ ] In `impl Drop for McpStdioClient`, keep `self.reader_abort.abort()` first.
- [ ] Clone `self.child` before trying the lock:

```rust
let child = Arc::clone(&self.child);
```

- [ ] If `try_lock()` succeeds, call `start_kill()` as today.
- [ ] If `try_lock()` fails, spawn an OS thread:

```rust
std::thread::spawn(move || {
    if let Ok(mut guard) = child.lock() {
        let _ = guard.start_kill();
    }
});
```

- [ ] Add a short comment explaining why an OS thread is used in `Drop`: no async wait is available and no tokio runtime can be assumed.

### 2. MCP Drop regression test

- [ ] Add a unit/integration test that starts a long-running stdio process through `McpStdioClient`.
- [ ] Force the child mutex to be held at drop time if practical. If private fields make this too invasive, factor the kill fallback into a private helper that accepts `Arc<Mutex<Child>>` and test the helper with a held lock.
- [ ] Assert dropping the client eventually terminates the child process. Use bounded polling with timeout; do not leave background processes alive on failure.

### 3. Node event callback failure handling

- [ ] In `runStream` event forwarding loop, capture the return status from the existing `tsfn.call(value, ThreadsafeFunctionCallMode::NonBlocking)` call instead of ignoring it.
- [ ] If status is not ok, attempt a best-effort notification:

```rust
let status = tsfn.call(value, ThreadsafeFunctionCallMode::NonBlocking);
if status != napi::Status::Ok {
    tracing::warn!("node event callback dropped: {:?}", status);
}
```

- [ ] If the current architecture exposes a primary Rust event channel to the Node binding, prefer sending `RuntimeEvent::EventsDropped { subscriber_id, count: 1 }` there first; if that channel is unavailable or full, log `tracing::warn!`.
- [ ] Do not block the event forwarding loop waiting for JS.

### 4. Node verification

- [ ] If there is an existing Node test harness, add a test that simulates `ThreadsafeFunctionCallMode::NonBlocking` failure or wraps forwarding behind a testable helper.
- [ ] If no harness exists, add a small Rust unit around the helper and rely on `cargo build -p agent-runtime-node` for binding compilation.

### 5. 验证

```bash
cargo test -p agent-runtime-core mcp
cargo test -p agent-runtime-core
cargo build -p agent-runtime-node
cargo fmt --check
```

Manual check after tests:

```bash
ps -axo pid,ppid,stat,command | rg "mcp|node|python" || true
```

No test-spawned zombie process should remain.
