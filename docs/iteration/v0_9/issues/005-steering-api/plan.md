# 005 · Mid-run Steering API — 实现计划

## 步骤

### 1. 填充 SteerCmd
文件：`crates/agent-runtime-core/src/run/actor.rs`
- `SteerCmd` 新增 `pub instruction: String`
- `SteerResult` 保持不变（确认信号）

### 2. 实现 Steer handler
文件：`crates/agent-runtime-core/src/run/actor.rs`
- `AgentMsg::Steer(cmd, reply)` handler：
  - 将 `cmd.instruction` 作为 system-role `Message` push 到 `state.messages`
  - emit `RuntimeEvent::SteerApplied { instruction: cmd.instruction.clone() }`（新事件）
  - reply `SteerResult`
- 移除 "not yet implemented" 警告

### 3. 新增 WatcherAction::Steer
文件：`crates/agent-runtime-core/src/run/watcher.rs`
- `WatcherAction` 新增 `Steer(String)` 变体

### 4. 更新 attach_watcher
文件：`crates/agent-runtime-core/src/run/handle.rs`
- match 分支新增 `WatcherAction::Steer(instruction)`：
  - 用 `ractor::call!` 发送 `AgentMsg::Steer(SteerCmd { instruction }, reply_port)`
  - 或用 cast 简化（fire-and-forget，不等回复）

### 5. RunHandle 公共 API
文件：`crates/agent-runtime-core/src/run/handle.rs`
```rust
pub async fn inject_message(&self, msg: &str) {
    // wait for actor_ref ready, then cast AgentMsg::Inject
}

pub async fn steer(&self, instruction: &str) {
    // wait for actor_ref ready, then call AgentMsg::Steer
}
```
- 两个方法都需要等 `ready` notify（与 `subscribe_events` 相同的等待模式）

### 6. 清理 dead_code 标注
文件：`crates/agent-runtime-core/src/run/agent_ref.rs`
- 移除 `AgentRef` 和方法上的 `#[allow(dead_code)]`

### 7. 新增 RuntimeEvent::SteerApplied
文件：`crates/agent-runtime-core/src/events.rs`
- 新增变体，watcher 可以观察到 steer 指令是否被应用

### 8. 测试
- 集成测试：inject_message → 检查模型调用的 messages 包含注入的 user-role 消息
- 集成测试：steer → 检查 messages 包含 system-role 指令
- 单元测试：WatcherAction::Steer 在 attach_watcher 中被正确分发

### 9. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
