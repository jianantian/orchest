# 002 · Core Runtime 正确性修复

## 背景

`agent-runtime-core` 的 code review 发现 2 个 Critical、1 个 Medium、3 个 Important 问题，涉及代码执行安全、Python 绑定兼容性、MCP 健壮性和 budget 准确性。

## 2a. Python pre-sentinel stdout loop 无行数上限（Critical）

**文件**：`crates/agent-runtime-core/src/tool/code_exec.rs:165-191`

```rust
loop {
    let mut line = String::new();
    let read = session.stdout.read_line(&mut line).await.map_err(io_tool_error)?;
    if read == 0 { ... }
    let trimmed = line.trim_end();
    if let Some(payload) = trimmed.strip_prefix(SENTINEL_PREFIX) { ... }
    // 非 sentinel 行：无计数、无 emit、直接丢弃继续 loop
}
```

问题：
1. 用户 Python 代码产出大量 stdout 行时，loop 无限自旋
2. 非 sentinel 行被静默丢弃，不通过 `emit_update` 发出

**修复**：

```rust
const MAX_PRE_SENTINEL_LINES: usize = 10_000;
let mut line_count = 0;

loop {
    let mut line = String::new();
    let read = session.stdout.read_line(&mut line).await.map_err(io_tool_error)?;
    if read == 0 {
        if let Some(mut dead) = guard.take() {
            let _ = dead.child.kill().await;
        }
        return Err(ToolError::fatal("python session exited").with_code("SESSION_EXITED"));
    }

    line_count += 1;
    let trimmed = line.trim_end();

    if let Some(payload) = trimmed.strip_prefix(SENTINEL_PREFIX) {
        // ... existing sentinel handling ...
        return Ok(output);
    }

    emit_update(ctx, json!({"stdout_line": trimmed})).await;

    if line_count >= MAX_PRE_SENTINEL_LINES {
        if let Some(mut dead) = guard.take() {
            let _ = dead.child.kill().await;
        }
        return Err(ToolError::fatal("python output exceeded line limit before sentinel")
            .with_code("OUTPUT_LIMIT"));
    }
}
```

## 2b. JS tool `which::which("deno")` TOCTOU 竞态（Medium）

**文件**：`crates/agent-runtime-core/src/tool/code_exec.rs:280,301`

`which::which("deno")` 调用两次：line 280 决定 spawn 哪个 command，line 301 决定是否 pipe stdin。两次调用之间 deno 状态可能变化（概率极低，后果为 stdin 写入被忽略的 node 进程）。

**修复**：

```rust
let use_deno = which::which("deno").is_ok();
let mut command = if use_deno {
    let mut cmd = Command::new("deno");
    cmd.arg("run").arg("--allow-net").arg("--allow-read").arg("-");
    cmd
} else {
    let mut cmd = Command::new("node");
    cmd.arg("-e").arg(code);
    cmd
};
// ...
if use_deno {
    if let Some(stdin) = child.stdin.as_mut() { ... }
}
```

## 2c. Python 绑定 `asyncio.run()` 在已有 event loop 中崩溃（Critical）

**文件**：`crates/agent-runtime-py/src/lib.rs:138-141`（tool handler）和 `207-215`（async job poll）

```rust
let asyncio = py.import("asyncio")?;
let awaited = asyncio.call_method1("run", (raw_result.bind(py),))?;
```

`asyncio.run()` 在已运行的 event loop 中抛 `RuntimeError: This event loop is already running`。`loop.run_until_complete()` 也有同样限制。FastAPI / uvicorn / Jupyter 等场景下 Python 一定有正在运行的 loop。

**修复**：检测是否有正在运行的 event loop，分路处理。

**无 running loop**：沿用 `asyncio.run(coro)`（当前行为）。

**有 running loop**（FastAPI / uvicorn / Jupyter）：在独立 OS 线程中运行 `asyncio.run(coro)`。关键约束：
- `asyncio.run()` 和 `loop.run_until_complete()` 都不能在已有 running loop 中调用
- 必须释放 GIL 后再等待结果，否则阻塞 Python 主线程
- `py.allow_threads` 闭包内不能碰 Python 对象

```rust
fn await_coroutine(py: Python, coro: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let asyncio = py.import("asyncio")?;
    let has_running_loop = asyncio.call_method0("get_running_loop").is_ok();

    if !has_running_loop {
        return asyncio.call_method1("run", (coro.bind(py),)).map(|v| v.unbind());
    }

    // 有 running loop：在独立线程中 asyncio.run，通过 std channel 传结果
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

    // 释放 GIL，等待独立线程完成
    let result = py.allow_threads(|| {
        rx.recv().map_err(|_| PyRuntimeError::new_err("async coroutine thread panicked"))
    })?;

    result
}
```

两处调用（tool handler line 138 和 async job poll line 207）共用此函数。

**注意**：`coro` 是 `Py<PyAny>`（GIL-independent reference），可安全传入新线程。新线程通过 `Python::with_gil` 重新获取 GIL 来执行 coroutine。`std::sync::mpsc` 在 `allow_threads`（GIL 释放）期间阻塞等待，不会死锁。

## 2d. MCP stdio reader parse 错误清空所有 pending request（Important）

**文件**：`crates/agent-runtime-core/src/tool/mcp.rs:103-119`

```rust
let response = match serde_json::from_str::<Value>(&line) {
    Ok(response) => response,
    Err(_) => {
        pending_for_reader.lock().await.clear();  // 清空所有！
        break;                                      // 退出 reader！
    }
};
```

一行 parse 失败就清空所有 pending request 并退出 reader task。MCP server 可能发送非 JSON 的 log 行或 notification。

**修复**：

```rust
let response = match serde_json::from_str::<Value>(&line) {
    Ok(response) => response,
    Err(_) => continue,  // 跳过非 JSON 行，继续读取
};
```

loop 结尾的 `pending_for_reader.lock().await.clear()` 保留——它在 EOF 时（`next_line` 返回 `None` 导致 `while let` 结束）清空，这是正确行为。

## 2e. `max_tool_calls` budget 竞态（Important）

**文件**：`crates/agent-runtime-core/src/run/actor.rs:818-835`（check）和 `1092`（record）

Budget check 在 line 818 检查 `tool_calls_used >= max`，但 `record_tool_call()` 在 line 1092 tool 执行完成后才调用。单步中多个 tool call 共享同一个 `tool_calls_used` 快照，全部通过 check。

**修复**：budget check 通过后立即 record：

```rust
// line 818 附近
if let Some(max) = state.config.budget.max_tool_calls {
    if state.budget.usage().tool_calls_used >= max {
        // ... emit budget exceeded error ...
        continue;
    }
    state.budget.record_tool_call();  // 立即计数
}
```

删除 budget check 之后各执行结果路径里的延迟 `state.budget.record_tool_call()`，包括 timeout、handoff 和正常执行完成路径，避免立即计数后重复计数。`before_tool` hook 的 `Skip` / `Reject` 分支发生在 budget check 之前；本 hotfix 不改变其既有计数语义，可暂时保留那两个分支的 record。

## 2f. `register_persistence_hook` 可重复注册（Important）

**文件**：`crates/agent-runtime-core/src/run/config.rs:370-381`

`start` 和 `resume` 都调用 `register_persistence_hook`，可能在同一 `AgentConfig` 上注册两个相同的 `SessionPersistenceHook`。

**修复**：push 前检查：

```rust
pub fn register_persistence_hook(&mut self, session_id: &str, store: Arc<dyn SessionStore>) {
    let dominated = self.hooks.iter().any(|h| {
        h.as_any().downcast_ref::<SessionPersistenceHook>()
            .map_or(false, |sph| sph.session_id() == session_id)
    });
    if dominated {
        return;
    }
    self.hooks.push(Arc::new(SessionPersistenceHook::new(session_id.to_string(), store)));
}
```

注意：需要 `SessionPersistenceHook` 暴露 `session_id()` getter，以及 Hook trait 需要 `as_any()` 方法（或已有）。如果 Hook trait 没有 `as_any()`，替代方案是在 `AgentConfig` 上加一个 `persistence_hook_registered: bool` flag。

## 验收标准

- [ ] Python pre-sentinel loop 有 `MAX_PRE_SENTINEL_LINES` 上限，超限后 kill 子进程并返回错误
- [ ] 非 sentinel 的 stdout 行通过 `emit_update` 发出
- [ ] `which::which("deno")` 只调用一次，结果缓存为 `use_deno`
- [ ] Python async tool handler 在 `asyncio.run()` 和已有 event loop 环境下均可执行
- [ ] `await_coroutine` 辅助函数被 tool handler 和 async job poll 共用
- [ ] MCP stdio reader 对 parse 错误 `continue` 而非 `break`
- [ ] `max_tool_calls` 在 budget check 通过后立即 `record_tool_call()`
- [ ] `register_persistence_hook` 对同一 `session_id` 不重复注册
- [ ] `cargo test -p agent-runtime-core` 全绿
