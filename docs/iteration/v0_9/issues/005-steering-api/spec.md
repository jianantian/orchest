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
- `RunHandle::inject_message(&self, msg: &str)` — 注入 user-role 消息
- `RunHandle::steer(&self, instruction: &str)` — 注入 system-role 指令
- `SteerCmd { instruction: String }`
- `AgentMsg::Steer` handler：将 instruction 作为 system-role 消息注入 `state.messages`
- `WatcherAction::Steer(String)` 新变体
- `attach_watcher` 中处理 `Steer` 变体（cast `AgentMsg::Steer`）

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
