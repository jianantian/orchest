# 006 · SupervisorActor + 崩溃恢复 — 实现计划

## 步骤

### 1. 定义 SupervisionStrategy
文件：`crates/agent-runtime-core/src/run/config.rs`
- 新增 `SupervisionStrategy` 枚举（`Restart { max_retries: u32 } / Stop`），derive `Debug, Clone`
- `AgentConfig` 新增 `pub supervision_strategy: SupervisionStrategy`（默认 `Stop`，`#[serde(default)]`）
- `AgentConfigBuilder` 新增 `.supervision_strategy()` 方法

### 2. 定义 SupervisorMsg 和 SupervisorState
文件：`crates/agent-runtime-core/src/run/actor.rs`（或新建 `supervisor.rs`）
- `SupervisorMsg` 枚举：`RegisterWatcher(Arc<dyn Watcher>, usize)` / `Shutdown`
- `SupervisorState`：strategy, attempts, store, worker_args_template, event_subs, watchers, actor_ref_shared
- `SupervisorArgs`：supervisor 启动参数

### 3. 实现 SupervisorActor
文件：同上
- `pre_start`：`spawn_linked(WorkerActor, args, myself.get_cell())`，存 worker_ref 到 `actor_ref_shared`
- `handle_msg`：
  - `RegisterWatcher` → push 到 `state.watchers` + 调用 `reattach_watcher`
  - `Shutdown` → stop worker + stop self
- `handle_supervisor_evt`：
  - `ActorFailed` → 按 strategy 决定 restart or stop
  - `ActorTerminated` → 正常退出，propagate stop
- restart 逻辑：load snapshot → rebuild args → `spawn_linked` new worker → update `actor_ref_shared` → re-attach all watchers

### 4. 新增 RuntimeEvent::RunRestarted
文件：`crates/agent-runtime-core/src/events.rs`
- `RunRestarted { attempt: u32 }`

### 5. 修改 spawn_actor
文件：`crates/agent-runtime-core/src/run/mod.rs`
- `spawn_actor()` 改为 spawn `SupervisorActor`（而非直接 spawn `WorkerActor`）
- supervisor spawn worker 并设置 `actor_ref_shared`
- `RunHandle` 结构不变——仍然引用 `actor_ref_shared`

### 6. 修改 attach_watcher
文件：`crates/agent-runtime-core/src/run/handle.rs`
- 当前逻辑保留（spawn watcher task）
- 新增：通过 supervisor_ref（需要在 RunHandle 中持有）发送 `RegisterWatcher` 消息
- 如果 supervisor 不存在（Stop 策略时可能不需要 supervisor？不——统一走 supervisor 更简单）

### 7. 辅助函数
- `reattach_watcher(worker_ref, watcher, capacity)` — subscribe events + spawn watcher task
- `load_latest_snapshot(store, session_id)` — 从 SessionStore 加载
- `rebuild_args_from_snapshot(snapshot, template)` — 用 snapshot 数据 + template 的 model/registry 重建 AgentRunArgs

### 8. 测试
- 集成测试：注入一个会 panic 的 tool → 验证 restart + snapshot 恢复
- 集成测试：max_retries = 0 → 立即 Stop
- 集成测试：多次 restart → 超过限制 → RunAborted
- 集成测试：restart 后 inject_message 仍然工作（actor_ref 更新）
- 集成测试：restart 后 watcher 仍然收到事件（re-attach）
- 回归测试：默认 `Stop` 策略行为与 v0.8 一致

### 9. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
