# 006 · SupervisorActor + 崩溃恢复

## 背景

当前 `AgentRun::start()` 直接 spawn `WorkerActor`，没有 supervision 层。Worker panic 时 actor 终止，无恢复能力。引入 `SupervisorActor` 作为 worker 的父 actor，利用 Ractor 的 supervision tree 实现崩溃检测 + 自动 restart from snapshot。

## 契约

### 输入
- `AgentRun::start()` 直接 `Actor::spawn(WorkerActor, args)`
- `RunHandle.actor_ref` 直接指向 worker
- 无 supervision 策略配置

### 输出
- `SupervisorActor`：spawn 并监控 `WorkerActor`（via `spawn_linked`）
- `SupervisionStrategy` 枚举：`Restart { max_retries } / Stop`
- `AgentConfig.supervision_strategy`：默认 `Stop`（与当前行为一致）
- `RuntimeEvent::RunRestarted { attempt }`：restart 后 emit
- `SupervisorState.watchers`：watcher 持久注册表，restart 后自动 re-attach
- `RunHandle.actor_ref` 仍指向 worker，supervisor 通过共享 Arc 在 restart 后更新

## 影响范围

- `crates/agent-runtime-core/src/run/mod.rs` — `spawn_actor()` 改为 spawn supervisor
- `crates/agent-runtime-core/src/run/actor.rs` — 新增 `SupervisorActor` + `SupervisorState` + `SupervisorMsg`
- `crates/agent-runtime-core/src/run/config.rs` — `AgentConfig` 新增 `supervision_strategy`
- `crates/agent-runtime-core/src/run/handle.rs` — `attach_watcher` 改为双写（spawn task + 注册到 supervisor）
- `crates/agent-runtime-core/src/events.rs` — 新增 `RunRestarted` 事件

## 设计约束

- `SupervisionStrategy::Stop` 是默认值——不改变现有用户的行为
- supervisor 不处理 `AgentMsg`，只处理 `SupervisionEvent` + 管理消息
- Ractor 0.15 的 `handle_supervisor_evt` 默认行为是 `myself.stop(None)`（supervisor 自停），必须 override
- Watcher 跨 restart：supervisor restart worker 后遍历 `watchers` 列表重新 attach
- `RunHandle::attach_watcher()` 通过 supervisor 消息（`SupervisorMsg::RegisterWatcher`）注册 watcher

## 验收标准

- [ ] `SupervisorActor` 实现 `Actor` trait，override `handle_supervisor_evt`
- [ ] `SupervisionStrategy::Restart { max_retries }` 时从 `SessionStore` 加载 snapshot → `AgentRun::resume()` 逻辑重建 worker
- [ ] `SupervisionStrategy::Stop` 时 emit `RunAborted` 并终止
- [ ] 超过 `max_retries` 后 emit `RunAborted` 并终止
- [ ] restart 后 `RunHandle.actor_ref` 指向新 worker（inject/steer 恢复工作）
- [ ] restart 后已注册 watcher 自动 re-attach
- [ ] `RuntimeEvent::RunRestarted { attempt }` 在每次 restart 后 emit
- [ ] `AgentConfig` 支持 `supervision_strategy` 字段
- [ ] 默认 `Stop` 行为与当前一致（无 regression）
- [ ] 集成测试：worker panic → restart → 从 snapshot 恢复继续执行
- [ ] 集成测试：超过 max_retries → RunAborted
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
