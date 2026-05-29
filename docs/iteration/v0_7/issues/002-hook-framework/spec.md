# 002 · Hook Framework

## 背景

runtime 的 run loop（`run/loop_.rs`，585 行）是硬编码单体——approval、budget、compaction、error mapping 全部直接写在循环里，用户无法注入自定义行为。三份竞品研究一致指出 Hook / 中间件框架是 Orchest 与竞品之间最根本的架构差异。

Hook 框架是 v0.7 后续所有 issue（003-007）的前置条件，也是 v0.8 Guardrail / Session 自动保存的基础。

## 目标

定义 runtime 生命周期 hook 点，用户可注入自定义行为。

## 范围

### Hook Trait

```rust
#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_run_start(&self, ctx: &mut RunHookContext) {}
    async fn on_run_end(&self, ctx: &RunHookContext, result: &RunResult) {}
    async fn on_run_error(&self, ctx: &RunHookContext, error: &RuntimeError) {}

    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction { ModelHookAction::Continue }
    async fn after_model(&self, ctx: &mut ModelHookContext, response: &ModelResponse) -> HookAction { HookAction::Continue }

    async fn before_tool(&self, ctx: &mut ToolHookContext) -> HookAction { HookAction::Continue }
    async fn after_tool(&self, ctx: &mut ToolHookContext, output: &ToolOutput) -> HookAction { HookAction::Continue }

    async fn on_handoff(&self, ctx: &HandoffHookContext) {}

    async fn before_compact(&self, ctx: &mut CompactHookContext) -> HookAction { HookAction::Continue }
}

pub enum HookAction {
    Continue,
    Skip,
    Abort(String),
}

pub enum ModelHookAction {
    Continue,
    Abort(String),
}
```

所有方法有默认空实现。

**HookAction 语义**：
- `Skip`：跳过当前步骤。`before_tool` 返回 Skip → 不执行该 tool call，向模型返回 `"tool call skipped by hook"` 作为 tool result。`before_compact` 返回 Skip → 跳过本次 compaction。
- `Abort(reason)`：终止整个 run。

**before_model 不支持 Skip**：跳过 model call 后 run loop 没有 response 无法继续（没有 tool_uses、没有 text_parts）。`before_model` 使用 `ModelHookAction`（只有 Continue / Abort），如需修改发送给模型的内容，通过 `&mut ModelHookContext` 修改 messages。

### Hook Panic 恢复

Hook 实现可能 panic（用户代码）。Runtime 用 `catch_unwind` 包裹每个 hook 调用：
- panic 被捕获并转化为 `on_run_error` 回调（只通知其他 hook，不递归调用 panic 的 hook）
- panic 信息记录到 `RuntimeEvent::HookPanicked { hook_name, message }`
- run 继续执行（不 abort）——单个 hook panic 不应终止用户的 agent run。如果用户希望 panic 终止 run，应在 hook 中显式返回 Abort

### Hook Context 类型

每个 hook 点有对应的 context 结构，提供该阶段可用的信息：

| Context | 可用信息 |
|---------|---------|
| `RunHookContext` | run_id, agent_config, step |
| `ModelHookContext` | run_id, messages（可修改）, model_spec |
| `ToolHookContext` | run_id, tool_name, tool_input（可修改）, tool_metadata |
| `HandoffHookContext` | run_id, previous_agent, new_agent, handoff_input |
| `CompactHookContext` | run_id, messages（可修改）, token_count |

`&mut` context 允许 hook 修改 messages / input 等（wrap 模式）。`&` context 为只读通知。

### AgentConfig 集成

```rust
pub struct AgentConfig {
    // ... 现有字段 ...
    pub hooks: Vec<Arc<dyn Hook>>,  // 新增
}
```

Builder 方法：

```rust
impl AgentConfig {
    pub fn with_hook(mut self, hook: Arc<dyn Hook>) -> Self {
        self.hooks.push(hook);
        self
    }
}
```

### Hook 调用链

在 `run/loop_.rs` 的关键位置插入 hook 调用：

| 位置 | hook 点 | 当前代码位置 |
|------|---------|-------------|
| run loop 入口 | `on_run_start` | `loop_.rs` loop 开始前 |
| model call 前 | `before_model` | `loop_.rs` `model.complete()` 调用前 |
| model call 后 | `after_model` | `loop_.rs` `model.complete()` 返回后 |
| tool dispatch 前 | `before_tool` | `tool_exec.rs` `tool.execute()` 调用前 |
| tool dispatch 后 | `after_tool` | `tool_exec.rs` `tool.execute()` 返回后 |
| handoff 发生时 | `on_handoff` | 新增的 handoff 分支中（依赖 004） |
| compaction 前 | `before_compact` | `compaction.rs` compact 调用前 |
| run loop 正常退出 | `on_run_end` | `loop_.rs` loop 结束后 |
| run loop 异常退出 | `on_run_error` | `loop_.rs` error 路径 |

多个 hook 按注册顺序链式调用。任一 hook 返回 `Abort` → 短路，后续 hook 不执行。

### 设计原则

- Core 只提供框架和调用链，**不内置任何具体 middleware**
- Hook 不改变 run loop 的核心控制流（不引入新的循环或分支）
- Hook 调用是 sync point——不 spawn task，不引入并发
- `on_handoff` hook 点为空实现——在 004（Handoff）落地前不会被调用

## 需要修改的文件

| 文件 | 变更 |
|------|------|
| `run/config.rs` | `AgentConfig` 新增 `hooks` 字段 + builder 方法 |
| `run/loop_.rs` | 在关键位置插入 hook 调用链 |
| `run/tool_exec.rs` | `before_tool` / `after_tool` 调用 |
| `run/compaction.rs` | `before_compact` 调用 |
| 新增 `hook.rs` | `Hook` trait、`HookAction`、所有 Context 类型 |

## 不在范围内

- 具体 middleware 实现（Guardrail → v0.8，Loop Detection → 007）
- `on_handoff` 的实际触发（依赖 004）
- Hook 的持久化或序列化

## 依赖

无。可与 001（Ractor PoC）并行启动。

## 验收标准

- [ ] `Hook` trait 定义完整，9 个 hook 点均有默认空实现
- [ ] `HookAction`（Continue / Skip / Abort）和 `ModelHookAction`（Continue / Abort）枚举定义
- [ ] `before_model` 返回 `ModelHookAction`（不支持 Skip）
- [ ] `AgentConfig` 支持 `hooks: Vec<Arc<dyn Hook>>` 和 `with_hook()` builder
- [ ] `run/loop_.rs` 在正确位置调用 hook 链
- [ ] 多 hook 按注册顺序执行，Abort 短路生效
- [ ] `before_model` hook 可修改 messages（通过 `&mut ModelHookContext`）
- [ ] `before_tool` hook 返回 Skip 时跳过 tool 执行，向模型返回 skip tool result
- [ ] Hook panic 被 `catch_unwind` 捕获，不终止 run，发出 `HookPanicked` 事件
- [ ] 测试：注册两个 hook，验证调用顺序和 context 数据正确
- [ ] 测试：hook 返回 Abort 时 run 正确终止
- [ ] 测试：hook panic 后 run 继续执行
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
