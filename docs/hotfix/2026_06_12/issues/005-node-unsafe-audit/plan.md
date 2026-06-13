# 005 · Node 绑定 unsafe 审计 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Prove the existing `unsafe impl Send/Sync for JsTool` is sound for the pinned napi-rs type, or replace it with a safe wrapper.

**Architecture:** Prefer compile-time trait assertions with clear safety comments. Only change runtime structure if the assertion fails.

**Tech Stack:** Rust, napi-rs `ThreadsafeFunction`, compile-time trait bounds.

---

## 要读的现有代码

- `crates/agent-runtime-node/src/lib.rs`
- `crates/agent-runtime-node/Cargo.toml`

## 文件改动

- Modify: `crates/agent-runtime-node/src/lib.rs`
- Optional fallback modify: `JsTool` field type and call sites in the same file

## 步骤

### 1. 添加 compile-time trait assertion

- [ ] Near the `JsTool` definition, add a small assertion block using the exact `ThreadsafeFunction` type used by `JsTool`.

If `JsTool` currently stores `ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal>`, assert that exact type. If the stored type differs, mirror the real field type rather than using `JsUnknown`.

```rust
const _: () = {
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    fn check_threadsafe_function_bounds() {
        assert_send::<ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal>>();
        assert_sync::<ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal>>();
    }
};
```

- [ ] Run:

```bash
cargo build -p agent-runtime-node
```

### 2. 如果 assertion 编译通过，更新 unsafe 注释

- [ ] Keep `unsafe impl Send for JsTool {}` and `unsafe impl Sync for JsTool {}`.
- [ ] Replace the current vague comment with:

```rust
// Safety: JsTool contains a napi ThreadsafeFunction. The const assertion above
// verifies the concrete ThreadsafeFunction type is Send + Sync for the pinned
// napi-rs version, so forwarding those auto-traits to JsTool is sound.
unsafe impl Send for JsTool {}
unsafe impl Sync for JsTool {}
```

### 3. 如果 assertion 编译失败，移除 unsafe impl

- [ ] Change `JsTool.handler` to `Arc<std::sync::Mutex<ThreadsafeFunction<serde_json::Value, ErrorStrategy::Fatal>>>`.
- [ ] Remove both `unsafe impl` blocks.
- [ ] At each call site, lock the mutex briefly only to call `ThreadsafeFunction::call`; do not hold the lock across unrelated work.
- [ ] Re-run `cargo build -p agent-runtime-node`.

### 4. 验证

```bash
cargo build -p agent-runtime-node
cargo clippy -p agent-runtime-node -- -D warnings
rg "unsafe impl (Send|Sync) for JsTool|assert_send|assert_sync" crates/agent-runtime-node/src/lib.rs
```

Expected: either assertion + documented unsafe impl are present, or no unsafe impl remains.
