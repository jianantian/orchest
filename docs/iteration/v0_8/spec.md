# v0.8 Spec：持久化与安全

## 背景

v0.7 建立了 Hook 框架和 Handoff 两层语义。runtime 具备了扩展能力，但仍缺少两个产品级 agent 应用的关键能力：

1. **Session 持久化**：当前 run 的状态纯内存，进程退出即丢失。长时间运行的 agent、多轮对话场景无法恢复。三份竞品研究中 pi-agent 对比标记为 🔴 级缺口
2. **安全层**：`requires_approval: bool` 是唯一的安全机制。产品级应用需要 guardrail（输入/输出/工具级审查）和多模式权限控制

这两个能力都可以基于 v0.7 的 Hook 框架实现，不需要在 core loop 中硬编码。

## 目标

1. 提供可插拔的 `SessionStore` trait，支持 run 状态的持久化和恢复
2. 提供 Guardrail 框架（Input / Output / Tool 三层），作为 Hook 实现
3. 权限模型从布尔值扩展为多模式

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

## 不在范围内

- Session 的跨进程同步 / 分布式存储（用户可自行实现 `SessionStore`）
- 内置的 LLM-based guardrail（如用另一个模型审查输出）——用户可基于 guardrail API 自行实现
- Mid-run Steering → v0.9
- Provider 扩展 → v0.9

## 依赖

- v0.7 Hook 框架（guardrail 和 session 自动保存都基于 hook 实现）
- v0.7 Handoff 重构（session snapshot 需要包含 handoff 状态）

## 验收标准

- [ ] `SessionStore` trait 定义完整
- [ ] `SqliteSessionStore` 可保存和恢复 run 状态
- [ ] 恢复后的 run 可继续执行（消息历史、budget 消耗正确）
- [ ] InputGuardrail / OutputGuardrail / ToolGuardrail 各有工作示例
- [ ] Guardrail `Reject` 结果正确回传模型，`Abort` 正确终止 run
- [ ] `ApprovalMode` 枚举替代 `requires_approval: bool`
- [ ] 向后兼容：`requires_approval: true/false` 的现有行为不变
- [ ] `cargo test --workspace` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
