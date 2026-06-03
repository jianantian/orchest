# 008 · 多 Watcher 协调 + Watcher 跨 Restart 存活

## 背景

v0.8 的 `attach_watcher` 支持多次调用（多个 watcher 独立消费事件流），但协调语义未明确定义。006 引入的 SupervisorActor 带来 watcher 跨 restart 存活的需求。本 issue 明确协调规则并确保 restart 后 watcher 自动重连。

依赖 006（SupervisorActor 提供 watcher 持久注册机制）和 007（LlmWatcher 是主要的多 watcher 使用场景）。

## 契约

### 输入
- `attach_watcher` 支持多次调用
- SupervisorActor 持有 `watchers: Vec<Arc<dyn Watcher>>`（006）
- `WatcherAction::Abort` 触发 cancel

### 输出
- FIFO 语义：多个 watcher 的 Inject/Steer 按到达时间序处理
- Abort 优先：任一 watcher 发出 Abort → 立即终止 run
- Abort 后其他 watcher task 因 channel closed 自动退出
- restart 后所有已注册 watcher 自动 re-attach

## 影响范围

- `crates/agent-runtime-core/src/run/handle.rs` — 确保 Abort 传播逻辑正确
- `crates/agent-runtime-core/src/run/actor.rs`（supervisor） — watcher re-attach 逻辑
- 集成测试

## 设计说明

**FIFO 已自然满足**：每个 watcher 独立 cast `AgentMsg::Inject/Steer`，Ractor 按消息到达顺序处理。无需额外机制。

**Abort 传播**：当一个 watcher 触发 `AgentMsg::Cancel`，worker actor 的 `cancelled = true` + `myself.stop(None)`。stop 后 event channel 的 sender 端 drop，其他 watcher 的 `rx.recv()` 返回 `None`，watcher task 自然退出。已有逻辑足够，需验证。

**跨 restart 存活**：006 的 SupervisorActor 在 restart 后遍历 `watchers` 调用 `reattach_watcher`。本 issue 验证该路径端到端正确。

## 验收标准

- [ ] 多个 watcher 的 Inject/Steer 按 FIFO 到达 actor
- [ ] 任一 watcher Abort → run 终止 → 其他 watcher task 退出
- [ ] restart 后所有 watcher 继续收到新 worker 的事件
- [ ] 集成测试：2 个 watcher，一个 inject + 一个 abort → 验证顺序和终止
- [ ] 集成测试：worker restart → watcher 继续收到 RunRestarted + 后续事件
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
