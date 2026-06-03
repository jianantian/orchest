# 005 · Mid-run Steering API

## 背景

v0.8 铺好了内部通路：`AgentMsg::Inject` 可工作，`AgentMsg::Steer` 是 stub，`AgentRef` 有 `pub(crate)` 的 `steer()` / `inject()`。v0.9 补齐最后一英里——暴露 `RunHandle` 公共 API + 实现 Steer handler + 扩展 `WatcherAction`。

## 契约

### 输入
- `RunHandle` 只有 `abort()` 和 `attach_watcher()` 公共方法
- `AgentMsg::Steer` handler 返回 "not yet implemented" 警告
- `SteerCmd` / `SteerResult` 为空 struct
- `WatcherAction` 只有 `Continue / Inject / Abort`

### 输出
- `RunHandle::inject_message(&self, msg: &str)` — 注入 user-role 消息（fire-and-forget）
- `RunHandle::steer(&self, instruction: &str)` — 注入 system-role 指令（fire-and-forget）
- `SteerCmd { instruction: String }`
- `AgentMsg::Steer(SteerCmd)` — 改为 cast（去掉 `RpcReplyPort`），与 Inject 保持一致的 fire-and-forget 语义
- `AgentMsg::Steer` handler：将 instruction 作为 system-role 消息注入 `state.messages`
- `SteerResult` 移除（不再需要 RPC reply）
- `WatcherAction::Steer(String)` 新变体
- `attach_watcher` 中处理 `Steer` 变体（cast `AgentMsg::Steer`）

### 设计决策

**Steer 改为 cast（非 RPC）**：v0.8 的 `AgentMsg::Steer` 使用 `RpcReplyPort<SteerResult>` 是预留设计。实际上 steer 只是将指令注入 `state.messages`——瞬间操作，没有需要等待的返回值。如果保持 RPC，watcher task 在 `call!` 时会阻塞等 reply，当 actor 忙于长 tool call 时 watcher 被卡住无法处理后续事件。改为 cast 后与 Inject 语义一致，watcher 永远不会被阻塞。

## 影响范围

- `crates/agent-runtime-core/src/run/handle.rs` — 新增公共方法
- `crates/agent-runtime-core/src/run/actor.rs` — `SteerCmd` 填充字段 + Steer handler 实现
- `crates/agent-runtime-core/src/run/agent_ref.rs` — 移除 `#[allow(dead_code)]` 标注
- `crates/agent-runtime-core/src/run/watcher.rs` — `WatcherAction` 新增 `Steer` 变体

## 验收标准

- [ ] `RunHandle::inject_message()` 可用，注入的消息 role 为 `User`
- [ ] `RunHandle::steer()` 可用，注入的消息 role 为 system 类
- [ ] `SteerCmd` 包含 `instruction: String`
- [ ] `AgentMsg::Steer` handler 将 instruction 注入 `state.messages`
- [ ] `WatcherAction` 包含 `Continue / Inject / Steer / Abort` 四个变体
- [ ] `attach_watcher` 处理 `Steer` 变体
- [ ] 集成测试：inject_message 后模型下一轮可见
- [ ] 集成测试：steer 后模型下一轮可见且为 system-role
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
