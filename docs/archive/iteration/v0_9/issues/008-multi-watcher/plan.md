# 008 · 多 Watcher 协调 — 实现计划

## 步骤

### 1. 验证 FIFO 语义
- 写集成测试：2 个 watcher，各自在特定事件后 inject 不同消息
- 验证 `state.messages` 中消息顺序与 watcher cast 顺序一致
- 预期：已有机制自然满足（Ractor message ordering）

### 2. 验证 Abort 传播
- 写集成测试：2 个 watcher，watcher A 在第 3 个事件后 Abort
- 验证 watcher B 的 task 在 channel closed 后退出
- 验证 `RuntimeEvent::RunAborted` 被 emit

### 3. 验证 Abort 后不再处理 Inject
- 写集成测试：watcher A abort 同时 watcher B inject
- 验证 actor 收到 cancel 后不再处理 inject（`state.cancelled = true` → stop）

### 4. 验证跨 restart 存活
- 写集成测试：
  1. 注册 2 个 watcher
  2. worker panic（via panic tool）
  3. supervisor restart worker
  4. 验证两个 watcher 都收到 `RunRestarted` 事件
  5. 验证 watcher 继续收到后续 tool call 事件
- 依赖 006 的 SupervisorActor + re-attach 逻辑

### 5. 边界 case 测试
- 0 个 watcher：纯 event subscriber 模式，无干预
- 1 个 watcher：基本场景，确保无 regression
- watcher on_event panic：不影响其他 watcher（attach_watcher spawn 独立 task）

### 6. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
