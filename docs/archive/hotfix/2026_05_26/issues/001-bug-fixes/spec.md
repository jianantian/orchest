# 001 · Bug 修复 + 安全一致性

## 背景

Review 发现 3 个行为正确性 bug 和 1 个安全模型不一致问题。全部是几行级修复。

## B1. Budget 检查丢弃违规原因

**文件**：`crates/agent-runtime-core/src/run/loop_.rs:169`

`BudgetGuard::check()` 返回 `Option<BudgetViolation>`（`TokenLimit` / `CostLimit` / `DurationLimit` / `ToolCallLimit` 四个变体），被 `_violation` 丢弃。`RunFailed` 事件只发出 `"budget_exceeded"` 字符串。

**修复**：将 violation 变体名格式化到 error 字符串中：

```rust
if let Some(violation) = budget.check() {
    // ...
    emit(&tx, RuntimeEvent::RunFailed {
        error: format!("budget_exceeded: {violation}"),
    }).await;
    return;
}
```

注意：使用 `{violation}`（Display）而非 `{violation:?}`（Debug）。需要 002 先完成 `BudgetViolation` 的 thiserror 派生。如果 001 先于 002 实施，临时用 `{violation:?}`，002 完成后改回 `{violation}`。

## B2. 审批等待无超时

**文件**：`run/loop_.rs:321`

`approval_rx.await.unwrap_or(false)` 无限等待。调用方忘记 `respond_approval()` 时 run loop 永久挂起。

**修复**：

```rust
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(3600); // 1 小时

let approved = match tokio::time::timeout(APPROVAL_TIMEOUT, approval_rx).await {
    Ok(result) => result.unwrap_or(false),
    Err(_) => {
        emit(&tx, RuntimeEvent::RunFailed {
            error: "approval_timeout".into(),
        }).await;
        return;
    }
};
```

超时时长提取为常量。1 小时是合理上限——真实场景中用户不会等超过这个时间。

## B3. PythonSession EOF 后死循环

**文件**：`tool/code_exec.rs:162-174`

`execute_python_in_session` 中 `read_line` 返回 0（EOF）时返回 `SESSION_EXITED` 错误，但未 `session.take()`。下次调用 `guard.is_none()` 为 false，跳过重建，再次 EOF，死循环。

对比超时路径（`code_exec.rs:101-104`）正确调用了 `session.take()`。

**修复**：EOF 路径在返回错误前清理 session：

```rust
if read == 0 {
    *guard = None;  // 清理已退出的 session，下次调用会重建
    return Err(ToolError {
        message: "python session exited".into(),
        code: Some("SESSION_EXITED".into()),
    });
}
```

注意：`guard` 是 `MutexGuard<Option<PythonSession>>`，`*guard = None` 会 drop 旧 session。需要确认 `PythonSession` 的 `Drop` 是否会清理子进程——当前 `PythonSession` 持有 `tokio::process::Child`，其 `Drop` 不会自动 kill。应该在置 `None` 前显式 kill：

```rust
if read == 0 {
    if let Some(mut session) = guard.take() {
        let _ = session.child.kill();  // kill() 是同步的（发送 SIGKILL）
    }
    return Err(ToolError { ... });
}
```

## S1. ExecutePythonTool 继承完整父进程环境变量

**文件**：`tool/code_exec.rs:192`

`spawn_python_session()` 中 `Command::new("python3")` 未调用 `.env_clear()`。子进程继承 `PATH`、`HOME`、`AWS_*` 等完整环境变量。

对比 `BareSubprocessExecutor`（`skill/executor.rs`）使用 `.env_clear().envs(&ctx.env)` 只传入声明的变量。

同一代码库两个执行路径的安全模型不一致。

**修复**：

```rust
let mut child = Command::new("python3")
    .arg("-u").arg("-i").arg("-q")
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", std::env::var("HOME").unwrap_or_default())
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()?;
```

只保留 Python 运行必需的最小环境变量。如果 `AgentConfig` 后续支持 `code_execution_env` 配置，可以在此扩展。

## 验收标准

- [ ] B1：`RunFailed` 事件的 `error` 字段包含违规类型（如 `"budget_exceeded: TokenLimit"`）
- [ ] B1：测试验证 `BudgetViolation` 的四个变体都能正确格式化
- [ ] B2：审批超时后发出 `RunFailed { error: "approval_timeout" }` 事件
- [ ] B2：`APPROVAL_TIMEOUT` 是提取的常量
- [ ] B3：PythonSession EOF 后下次调用成功重建新 session（不死循环）
- [ ] B3：EOF 路径显式 kill 子进程
- [ ] B3：测试覆盖 EOF → 重建 → 正常执行路径
- [ ] S1：`spawn_python_session()` 调用 `.env_clear()`
- [ ] S1：只传入 `PATH` 和 `HOME`（或经过 review 确认的最小集合）
- [ ] `cargo test --workspace` 全绿
