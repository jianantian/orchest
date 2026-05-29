# v0.8 Spec：持久化 + 安全 + Supervised Delegation 基础

## 背景

v0.7 建立了 Hook 框架、Handoff 两层语义，并通过 Ractor PoC 决定了 runtime 的通信架构走向。runtime 具备了扩展能力，但仍缺少产品级 agent 应用的关键能力：

1. **Session 持久化**：当前 run 的状态纯内存，进程退出即丢失。长时间运行的 agent、多轮对话场景无法恢复。竞品对比标记为 🔴 级缺口
2. **安全层**：`requires_approval: bool` 是唯一的安全机制。产品级应用需要 guardrail 和多模式权限控制
3. **Supervised Delegation 基础通信层**：[Actor Model 评估](../../research/actor-model-evaluation.md) 识别的核心验证 case，需要双向通信、多方事件订阅、watcher 注册等基础能力。这些基础在 v0.8 建好后，v0.9 的 Mid-run Steering 和 Supervised Delegation 完整实现就变成"已有能力的 API 暴露"而不是"从零搭建通信层"

## 目标

1. 提供可插拔的 `SessionStore` trait，支持 run 状态的持久化和恢复
2. 提供 Guardrail 框架（Input / Output / Tool 三层），作为 Hook 实现
3. 权限模型从布尔值扩展为多模式
4. 建立 Supervised Delegation 所需的双向通信和多方事件订阅基础

## 范围

### Session 持久化

可插拔 `SessionStore` trait：

```rust
#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session_id: &str, state: &SessionSnapshot) -> Result<(), SessionError>;
    async fn load(&self, session_id: &str) -> Result<Option<SessionSnapshot>, SessionError>;
    async fn delete(&self, session_id: &str) -> Result<(), SessionError>;
    async fn list(&self) -> Result<Vec<String>, SessionError>;
}
```

内置实现：
- `InMemorySessionStore`（默认，行为与当前一致）
- `SqliteSessionStore`（本地持久化）

通过 hook 框架的 `on_run_end` 回调触发自动保存。`AgentConfig` 增加 `session_store` 和 `session_id` 配置。

`SessionSnapshot` 包含：messages、tool states、budget 消耗、step 计数。不包含 runtime 内部状态（channel、task handle 等）。

竞品参考：
- OpenAI `Session`（SQLite / Server 两种后端）
- pi-agent `SessionStore`（可插拔后端）
- Claude SDK JSONL 文件持久化

### Guardrails

基于 Hook 框架实现，不改 core loop。三层 guardrail：

| 层级 | hook 点 | 作用 |
|------|---------|------|
| InputGuardrail | `before_model` | 审查发送给模型的消息，可拒绝或修改 |
| OutputGuardrail | `after_model` | 审查模型输出，可拒绝或修改 |
| ToolGuardrail | `before_tool` / `after_tool` | 审查工具输入/输出，可拒绝或修改 |

每层返回三种结果：`Allow`、`Reject(reason)`、`Abort(reason)`。`Reject` 将拒绝原因作为 tool result 返回给模型（让模型换策略），`Abort` 终止整个 run。

提供 guardrail 注册 API，底层实现为注册对应 hook 点的 Hook。用户也可以直接用 Hook trait 实现更灵活的审查逻辑。

竞品参考：
- OpenAI 4 层 guardrail（Input / Output / ToolInput / ToolOutput）
- Claude SDK `PreToolUse` / `PostToolUse` hook

### 权限模型扩展

从 `requires_approval: bool` 扩展为 `ApprovalMode` 枚举：

| 模式 | 行为 |
|------|------|
| `None` | 不需要审批（当前 `false`） |
| `All` | 所有工具调用需审批（当前 `true`） |
| `SideEffectOnly` | 只有 `side_effect: true` 的工具需审批 |
| `Custom(fn)` | 自定义审批函数 |

可结合 ToolGuardrail 实现更细粒度的权限控制。

竞品参考：
- Craft Agents 5 种模式（Safe / AcceptEdits / Plan / Ask / Admin）

### Supervised Delegation 基础通信层

这是 v0.9 Supervised Delegation 完整实现和 Mid-run Steering 的前置基础。[Actor Model 评估](../../research/actor-model-evaluation.md) 识别了当前 runtime 在这个场景下的四项能力缺失，v0.8 解决其中的通信基础问题。

#### 双向通信

扩展 RunHandle 内部通信机制，支持向运行中的 agent 注入消息。v0.8 建立内部通道，v0.9 在其上暴露面向用户的 `inject_message()` / `steer()` 公共 API。

```rust
// v0.8 内部机制（不直接暴露为公共 API）

// PoC 通过（Ractor 方案）
// RunHandle 内部持有 ActorRef<AgentMsg>，通过 cast! 发送消息

// PoC 未通过（Channel 方案）
// RunHandle 内部持有 mpsc::Sender<AgentMessage>
```

Run loop 在每次迭代前检查消息队列（Ractor：actor mailbox 天然支持；Channel：手动 `try_recv`）。

#### 多方事件订阅

当前 `mpsc::Receiver` 是 single-consumer，watcher 无法独立消费事件流。

```rust
// PoC 通过（Ractor 方案）
// 方案 A：Ractor ProcessGroup 广播——worker 事件 cast 到 pg，所有注册的 watcher actor 收到
// 方案 B：worker actor 内部维护 subscriber list，事件逐个转发给 watcher ActorRef

// PoC 未通过（Channel 方案）
// mpsc → broadcast channel
impl RunHandle {
    pub fn subscribe_events(&self) -> broadcast::Receiver<RuntimeEvent> { ... }
}
```

#### Watcher 注册 API

允许注册 watcher 函数或 watcher agent，监听 worker 事件流并在满足条件时触发干预：

```rust
pub trait Watcher: Send + Sync {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction;
}

pub enum WatcherAction {
    Continue,
    Steer(String),
    Abort(String),
}
```

v0.8 只提供注册和事件分发机制，不实现 watcher LLM（v0.9 交付）。

## 不在范围内

- Session 的跨进程同步 / 分布式存储（用户可自行实现 `SessionStore`）
- 内置的 LLM-based guardrail（用户可基于 guardrail API 自行实现）
- Watcher LLM 实现（watcher 回调中调用另一个模型做判断）→ v0.9
- Mid-run Steering 的 `RunHandle::steer()` / `inject_message()` 公共 API → v0.9
- Supervised Delegation 完整实现（watcher 中途干预、崩溃恢复完整流程）→ v0.9
- Provider 扩展 → v0.9

## 依赖

- v0.7 Hook 框架（guardrail 和 session 自动保存都基于 hook 实现）
- v0.7 Handoff 重构（session snapshot 需要包含 handoff 状态）
- v0.7 PoC 结论（决定双向通信和事件订阅的实现方式：Ractor actor vs channel）

## 验收标准

- [ ] `SessionStore` trait 定义完整
- [ ] `SqliteSessionStore` 可保存和恢复 run 状态
- [ ] 恢复后的 run 可继续执行（消息历史、budget 消耗正确）
- [ ] InputGuardrail / OutputGuardrail / ToolGuardrail 各有工作示例
- [ ] Guardrail `Reject` 结果正确回传模型，`Abort` 正确终止 run
- [ ] `ApprovalMode` 枚举替代 `requires_approval: bool`
- [ ] 向后兼容：`requires_approval: true/false` 的现有行为不变
- [ ] RunHandle 内部双向通道可用——向运行中的 agent 注入消息后，run loop 在下一轮迭代响应
- [ ] 事件订阅支持多 consumer（broadcast channel 或 actor 多引用）
- [ ] `Watcher` trait 定义完整，可注册 watcher 并接收事件
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
