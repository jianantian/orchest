# 002 · Core Runtime 正确性修复 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Fix core runtime correctness issues around code execution, Python async handlers, MCP robustness, budget counting, and persistence hook duplication.

**Architecture:** Keep external SDK APIs stable. Add narrowly-scoped helpers in the modules that already own the behavior, with regression tests at the closest existing test layer.

**Tech Stack:** Rust, Tokio, PyO3, MCP stdio JSON-RPC, existing runtime budget/session hooks.

---

## 要读的现有代码

- `crates/agent-runtime-core/src/tool/code_exec.rs`
- `crates/agent-runtime-py/src/lib.rs`
- `crates/agent-runtime-core/src/tool/mcp.rs`
- `crates/agent-runtime-core/src/run/actor.rs`
- `crates/agent-runtime-core/src/run/config.rs`
- `crates/agent-runtime-core/src/session/persistence_hook.rs`
- `crates/agent-runtime-core/src/hook/mod.rs`

## 文件改动

- Modify/Test: `crates/agent-runtime-core/src/tool/code_exec.rs`
- Modify/Test: `crates/agent-runtime-core/src/tool/mcp.rs`
- Modify/Test: `crates/agent-runtime-core/src/run/actor.rs`
- Modify/Test: `crates/agent-runtime-core/src/run/config.rs`
- Modify: `crates/agent-runtime-core/src/session/persistence_hook.rs`
- Modify: `crates/agent-runtime-core/src/hook/mod.rs` only if downcasting is chosen
- Modify/Test: `crates/agent-runtime-py/src/lib.rs`
- Test: `python/tests/test_types_and_tools.py` or a new focused Python binding test

## 步骤

### 1. Python code exec pre-sentinel loop 加上限和 updates

- [ ] 在 `code_exec.rs` 顶部添加：

```rust
const MAX_PRE_SENTINEL_LINES: usize = 10_000;
```

- [ ] 在 Python session 读取 sentinel 的 loop 中维护 `line_count`。
- [ ] 对非 sentinel 行调用现有 `emit_update(ctx, json!({"stdout_line": trimmed})).await;`。
- [ ] 当 `line_count >= MAX_PRE_SENTINEL_LINES` 时 kill child，清理 session guard，返回：

```rust
ToolError::fatal("python output exceeded line limit before sentinel")
    .with_code("OUTPUT_LIMIT")
```

- [ ] 添加单元测试：模拟 Python 代码输出超过上限且不输出 sentinel，断言返回 `OUTPUT_LIMIT`。
- [ ] 添加单元测试：模拟 sentinel 前普通 stdout，断言收到 `ToolCallUpdate` partial。

### 2. JS code exec 缓存 deno 探测结果

- [ ] 在 JS execution 分支中引入一次性变量：

```rust
let use_deno = which::which("deno").is_ok();
```

- [ ] spawn command 和 stdin 写入分支都使用 `use_deno`。
- [ ] 用 `rg "which::which\\(\"deno\"\\)" crates/agent-runtime-core/src/tool/code_exec.rs` 确认只剩一处。

### 3. Python binding 抽出 `await_coroutine`

- [ ] 在 `crates/agent-runtime-py/src/lib.rs` 中新增 helper：

```rust
fn await_coroutine(py: Python<'_>, coro: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let asyncio = py.import("asyncio")?;
    let has_running_loop = asyncio.call_method0("get_running_loop").is_ok();

    if !has_running_loop {
        return asyncio.call_method1("run", (coro.bind(py),)).map(|v| v.unbind());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let coro_clone = coro.clone_ref(py);

    std::thread::spawn(move || {
        Python::with_gil(|py| {
            let result = py
                .import("asyncio")
                .and_then(|asyncio| asyncio.call_method1("run", (coro_clone.bind(py),)));
            let _ = tx.send(result.map(|v| v.unbind()));
        });
    });

    py.allow_threads(|| {
        rx.recv()
            .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("async coroutine thread panicked"))
    })?
}
```

- [ ] Replace the tool handler coroutine path with `await_coroutine(py, raw_result.unbind())?`.
- [ ] Replace async job poll coroutine path with the same helper, mapping `PyErr` to existing `ToolError` shape.
- [ ] Add Rust/PyO3 regression coverage around `await_coroutine`; do not use `Agent("test/provider", ...).run_sync(...)` unless a fake provider is explicitly wired into the Python binding test harness. The focused test should create a coroutine and call `await_coroutine` in both no-running-loop and running-loop contexts.

No-running-loop case:

```rust
Python::with_gil(|py| {
    let asyncio = py.import("asyncio")?;
    let coro = asyncio.call_method1("sleep", (0,))?.unbind();
    let result = await_coroutine(py, coro);
    assert!(result.is_ok());
    Ok::<(), PyErr>(())
})?;
```

Running-loop case: execute a small Python coroutine that calls back into a Rust-exposed test helper while `asyncio.run(...)` has an active loop, or add a test-only helper that forces the running-loop branch. The assertion is that `await_coroutine` returns `Ok` and does not raise `RuntimeError: This event loop is already running`.

Only add a Python-level `Agent.run_sync` test if the test first provides a deterministic fake model adapter; do not depend on a real provider or `test/provider`.

### 4. MCP stdio reader 跳过 malformed line

- [ ] In `tool/mcp.rs`, change parse error handling inside reader loop to `continue`.
- [ ] Keep final pending clear after EOF.
- [ ] Add a unit test with a fake stdio server that writes one non-JSON line followed by a valid JSON-RPC response; assert request completes.
- [ ] Add a unit test where EOF occurs with pending request; assert pending request fails/clears as current behavior expects.

### 5. `max_tool_calls` check 通过后立即计数

- [ ] In `run/actor.rs`, find the main tool-call budget check.
- [ ] Move `state.budget.record_tool_call()` immediately after the budget check passes.
- [ ] Remove delayed post-check `record_tool_call()` calls from timeout, handoff, and normal execution completion paths; those paths are now covered by the immediate record.
- [ ] Keep pre-budget `before_tool` hook `Skip` / `Reject` records only if preserving existing semantics; if moving those branches after the budget check, ensure they are counted exactly once.
- [ ] Add regression test: configure `max_tool_calls = 1`, make model return two tool calls in one step, assert first can start and second fails with budget exceeded.

### 6. `register_persistence_hook` 去重

- [ ] Prefer minimal approach if hook downcasting is already available. If `Hook` lacks `as_any`, add:

```rust
fn as_any(&self) -> &dyn std::any::Any;
```

to the trait and implement it for hook types.

- [ ] Add `SessionPersistenceHook::session_id(&self) -> &str`.
- [ ] In `AgentConfig::register_persistence_hook`, before push, return early if an existing `SessionPersistenceHook` has the same session id.
- [ ] If adding `as_any` across all hooks is too broad, use a private `persistence_hook_registered: bool` or `HashSet<String>` field on `AgentConfig`; do not expose it in public config serialization unless necessary.
- [ ] Add unit test: call `register_persistence_hook()` twice on a config with the same session store/session id, assert hook count only increases once.

### 7. 验证

```bash
cargo test -p agent-runtime-core
cargo test -p agent-runtime-py
uvx maturin develop
.venv/bin/python -m pytest python/tests/test_types_and_tools.py python/tests/test_run_sync.py -v
cargo fmt --check
```

If `uvx maturin develop` is unavailable in the environment, record that packaging verification was not run and keep Rust tests passing.
