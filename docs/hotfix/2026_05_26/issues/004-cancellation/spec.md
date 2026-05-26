# 004 · 运行取消机制

## 背景

`RunHandle` 提供 `wait()` 但无法取消运行中的 agent。Drop event receiver 只导致 channel 发送端阻塞，不终止 run loop。`RunStatus::Aborted` 状态已存在但无触发路径。

## 目标

提供轻量的协作式取消机制。不需要强制终止——在 loop 顶部检查即可。

## 设计

使用 `tokio_util::sync::CancellationToken`：

```rust
// run/handle.rs
pub struct RunHandle {
    pub(crate) event_rx: mpsc::Receiver<RuntimeEvent>,
    pub(crate) cancel_token: CancellationToken,
    // ...
}

impl RunHandle {
    /// 请求取消当前 run。run loop 会在当前 step 完成后终止。
    pub fn abort(&self) {
        self.cancel_token.cancel();
    }
}
```

```rust
// run/loop_.rs — loop 顶部
loop {
    if cancel_token.is_cancelled() {
        emit(&tx, RuntimeEvent::RunAborted).await;
        return;
    }
    // ... budget check, model call, tool dispatch ...
}
```

注意：取消是用户主动行为，不是故障。应使用已有的 `RunStatus::Aborted` 语义，发出 `RuntimeEvent::RunAborted`（如不存在则新增），而非复用 `RunFailed`。这样消费者可以区分"出错"和"主动停止"。

`CancellationToken` 在 `AgentRun::start()` 中创建，传入 `run_loop` 和 `RunHandle`。子 agent 继承父 token 的 `child_token()`。

## 依赖

`Cargo.toml` 新增：`tokio-util = { version = "0.7", features = ["rt"] }`

## 验收标准

- [ ] `RunHandle` 暴露 `pub fn abort(&self)`
- [ ] `run_loop` 每次迭代顶部检查 `cancel_token.is_cancelled()`
- [ ] 取消后发出 `RuntimeEvent::RunAborted` 事件（不复用 `RunFailed`）
- [ ] 子 agent 使用 `cancel_token.child_token()`，父取消时子也终止
- [ ] 测试覆盖：启动 run → 调 abort → 验证收到 `RunFailed` 事件
- [ ] 测试覆盖：父 agent abort → 子 agent 也终止
- [ ] `cargo test --workspace` 全绿
