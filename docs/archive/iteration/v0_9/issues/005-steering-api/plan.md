# 005 · Mid-run Steering API — 实现计划

## 步骤

### 1. 改 Steer 为 cast + 填充 SteerCmd
文件：`crates/agent-runtime-core/src/run/actor.rs`
- `SteerCmd` 新增 `pub instruction: String`
- `SteerResult` 删除
- `AgentMsg::Steer(SteerCmd, RpcReplyPort<SteerResult>)` → `AgentMsg::Steer(SteerCmd)`（去掉 reply port）

### 2. 实现 Steer handler
文件：`crates/agent-runtime-core/src/run/actor.rs`
- `AgentMsg::Steer(cmd)` handler：
  - 将 `cmd.instruction` 作为 system-role `Message` push 到 `state.messages`
  - emit `RuntimeEvent::SteerApplied { instruction: cmd.instruction.clone() }`（新事件）
- 移除 "not yet implemented" 警告

### 3. 更新 AgentRef::steer
文件：`crates/agent-runtime-core/src/run/agent_ref.rs`
- `steer()` 从 `ractor::call!` 改为 `self.inner.cast(AgentMsg::Steer(cmd))`
- 返回类型从 `Result<SteerResult, AgentError>` 改为 `()`（fire-and-forget）
- 移除 `#[allow(dead_code)]` 标注

### 4. 新增 WatcherAction::Steer
文件：`crates/agent-runtime-core/src/run/watcher.rs`
- `WatcherAction` 新增 `Steer(String)` 变体

### 5. 更新 attach_watcher
文件：`crates/agent-runtime-core/src/run/handle.rs`
- match 分支新增 `WatcherAction::Steer(instruction)`：
  ```rust
  WatcherAction::Steer(instruction) => {
      if let Ok(guard) = actor_ref.lock() {
          if let Some(ref aref) = *guard {
              let _ = aref.cast(AgentMsg::Steer(SteerCmd { instruction }));
          }
      }
  }
  ```
- 与 `Inject` 分支模式完全一致（都是 cast）

### 6. RunHandle 公共 API
文件：`crates/agent-runtime-core/src/run/handle.rs`
```rust
pub fn inject_message(&self, msg: &str) {
    if let Ok(guard) = self.actor_ref.lock() {
        if let Some(ref aref) = *guard {
            let _ = aref.cast(AgentMsg::Inject(InjectCmd { message: msg.to_string() }));
        }
    }
}

pub fn steer(&self, instruction: &str) {
    if let Ok(guard) = self.actor_ref.lock() {
        if let Some(ref aref) = *guard {
            let _ = aref.cast(AgentMsg::Steer(SteerCmd { instruction: instruction.to_string() }));
        }
    }
}
```
- 两个方法都是同步的 fire-and-forget（cast 不阻塞）
- 不需要 `async` 也不需要等 `ready`——如果 `actor_ref` 还是 `None`（actor 未启动），cast 静默跳过

### 7. 新增 RuntimeEvent::SteerApplied
文件：`crates/agent-runtime-core/src/events.rs`
- 新增变体 `SteerApplied { instruction: String }`，watcher 可观察 steer 指令是否被应用

### 8. 清理 dead_code 标注
文件：`crates/agent-runtime-core/src/run/agent_ref.rs`
- 移除 `AgentRef`、`AgentError` 和方法上的 `#[allow(dead_code)]`

### 9. 测试
- 集成测试：inject_message → 检查模型调用的 messages 包含注入的 user-role 消息
- 集成测试：steer → 检查 messages 包含 system-role 指令 + `SteerApplied` 事件
- 单元测试：WatcherAction::Steer 在 attach_watcher 中被正确分发

### 10. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
