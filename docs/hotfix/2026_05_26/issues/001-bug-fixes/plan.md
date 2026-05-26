# 001 · Bug 修复 + 安全一致性 — 实施计划

## 依赖

无前置依赖，可立即开始。B1 使用 `{violation:?}` 临时实现，002 完成后改为 `{violation}`。

## 步骤

### Step 1: B1 — Budget 违规原因保留

**文件**：`crates/agent-runtime-core/src/run/loop_.rs:169`

1. 将 `if let Some(_violation) = budget.check()` 改为 `if let Some(violation) = budget.check()`
2. `RunFailed` 的 error 改为 `format!("budget_exceeded: {violation:?}")`（临时用 Debug）
3. 在 `crates/agent-runtime-core/src/budget.rs` 添加测试：四个 `BudgetViolation` 变体的 Debug 输出

**002 完成后回来改**：`{violation:?}` → `{violation}`（Display）

### Step 2: B2 — 审批超时

**文件**：`crates/agent-runtime-core/src/run/loop_.rs:321`

1. 在文件顶部（或 `run/helpers.rs`）添加常量：
   ```rust
   const APPROVAL_TIMEOUT: Duration = Duration::from_secs(3600);
   ```
2. 将 `approval_rx.await.unwrap_or(false)` 改为 `tokio::time::timeout` 包裹
3. 超时路径发出 `RunFailed { error: "approval_timeout" }` 并 return
4. 添加测试：spawn run → 不 respond → 验证超时后收到 RunFailed 事件

### Step 3: B3 — PythonSession EOF 死循环

**文件**：`crates/agent-runtime-core/src/tool/code_exec.rs:169-174`

1. 在 `if read == 0` 分支中，在返回 Err 前清理 session：
   ```rust
   if let Some(mut session) = guard.take() {
       let _ = session.child.kill();
   }
   ```
2. 确认 `guard` 类型是 `MutexGuard<Option<PythonSession>>`，`take()` 会将其置为 None
3. 确认 `child.kill()` 是同步的（tokio::process::Child::kill 返回 `io::Result<()>`，不是 async）
4. 添加测试：模拟 EOF → 验证下次调用重建 session

### Step 4: S1 — Python 环境变量隔离

**文件**：`crates/agent-runtime-core/src/tool/code_exec.rs:191-203`

1. 在 `Command::new("python3")` 后插入 `.env_clear()`
2. 添加 `.env("PATH", ...)` 和 `.env("HOME", ...)`
3. 添加测试：spawn session → 在 session 中 `os.environ` → 验证只有 PATH 和 HOME

## 验证

```bash
cargo test -p agent-runtime-core -- budget
cargo test -p agent-runtime-core -- approval
cargo test -p agent-runtime-core -- python
cargo test -p agent-runtime-core -- code_exec
cargo clippy -p agent-runtime-core -- -D warnings
```
