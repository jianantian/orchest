# 004 · 运行取消机制 — 实施计划

## 依赖

无前置依赖。需要新增 `tokio-util` 依赖。

## 步骤

### Step 1: 添加依赖

**文件**：`crates/agent-runtime-core/Cargo.toml`

1. 添加 `tokio-util = { version = "0.7", features = ["rt"] }`

### Step 2: 新增 RuntimeEvent::RunAborted

**文件**：`crates/agent-runtime-core/src/events.rs`

1. 在 `RuntimeEvent` enum 中添加：
   ```rust
   RunAborted,
   ```
2. 位置放在 `RunCompleted` / `RunFailed` 附近，语义是同一组生命周期事件

### Step 3: 改造 RunHandle

**文件**：`crates/agent-runtime-core/src/run/handle.rs`

1. 添加 `use tokio_util::sync::CancellationToken;`
2. `RunHandle` 新增字段：`pub(crate) cancel_token: CancellationToken`
3. 添加方法：
   ```rust
   pub fn abort(&self) {
       self.cancel_token.cancel();
   }
   ```

### Step 4: 创建并传递 CancellationToken

**文件**：`crates/agent-runtime-core/src/run/mod.rs`

1. 在 `AgentRun::start_with_bus()` 中创建 `CancellationToken::new()`
2. clone 一份传入 `run_loop`
3. 原件存入 `RunHandle`

### Step 5: run_loop 检查取消

**文件**：`crates/agent-runtime-core/src/run/loop_.rs`

1. `run_loop_inner` 签名新增 `cancel_token: CancellationToken` 参数
2. 在主循环顶部（`loop {` 之后、budget check 之前）添加：
   ```rust
   if cancel_token.is_cancelled() {
       emit(&tx, RuntimeEvent::RunAborted).await;
       return;
   }
   ```

### Step 6: 子 agent 继承 cancel token

**文件**：`crates/agent-runtime-core/src/run/sub_agent.rs`

1. `execute_agent_delegate` 签名新增 `cancel_token: CancellationToken`
2. 调用 `AgentRun::start_with_bus` 时传入 `cancel_token.child_token()`
3. 父 abort 时子 agent 的 child_token 也被取消

### Step 7: 更新 AgentRun::start_with_bus 签名

将 cancel_token 作为参数传入（或在内部创建后通过 RunHandle 暴露）。需要同步更新 `start()` 方法。

### Step 8: 测试

**文件**：`crates/agent-runtime-core/src/run/tests.rs`（或新建测试文件）

1. 测试 1：启动 run → 立即 abort → 验证收到 `RunAborted` 事件（非 `RunFailed`）
2. 测试 2：启动 parent + child run → 父 abort → 验证子也终止

## 文件影响范围

```
crates/agent-runtime-core/Cargo.toml        — 新增 tokio-util
crates/agent-runtime-core/src/events.rs      — 新增 RunAborted variant
crates/agent-runtime-core/src/run/handle.rs  — 新增 cancel_token 字段 + abort()
crates/agent-runtime-core/src/run/mod.rs     — 创建并传递 token
crates/agent-runtime-core/src/run/loop_.rs   — 检查 is_cancelled
crates/agent-runtime-core/src/run/sub_agent.rs — child_token 传递
```

## 验证

```bash
cargo test -p agent-runtime-core -- cancel
cargo test -p agent-runtime-core -- abort
cargo test --workspace
cargo clippy -p agent-runtime-core -- -D warnings
```
