# v0.7 Spec：扩展性地基 + Actor PoC

## 背景

v0.1–v0.6 + 两轮 hotfix 完成了核心 runtime、多 provider、MCP、sub-agent、budget 等功能，并清偿了结构债务。但 runtime 的 run loop 仍然是"硬编码单体"——所有横切关注点直接写在循环里，用户无法注入自定义行为。

三份竞品研究一致指出：**Hook / 中间件框架是 Orchest 与竞品之间最根本的架构差异**。同时，两份新增研究揭示了更深层的架构决策：

- [Actor Model 评估](../../research/actor-model-evaluation.md)：当前 runtime 用 channel 手写 proto-actor 模式，随着 Supervised Delegation 等场景的通信需求增长，需要评估引入 Ractor。评估推荐 **v0.7 引入 Ractor**，在其上封装 Kameo 风格的 typed API
- [Sub-agent Handoff vs Agent-as-Tool](../../research/sub-agent-handoff-vs-agent-as-tool.md)：`__sub_agent_request` + `AgentDelegate` 双路径问题需要统一为 Agent-as-Tool + Handoff 两层语义

## 目标

1. 通过 Ractor PoC 验证 actor model 是否适合 Orchest runtime，结论作为 gate 决定后续架构走向
2. runtime 提供明确的生命周期 hook 点，用户可注入自定义行为
3. sub-agent 统一为 Agent-as-Tool + Handoff 两层语义，消除 `__sub_agent_request`
4. 模型调用失败时支持可配置的重试策略
5. 检测并阻止 agent 循环调用同一工具

## Phase 1：Ractor PoC（gate 决策点）

**时长**：1-2 天

**目标**：用 Ractor 实现最小 Supervised Delegation 场景，验证 actor 框架是否满足 Orchest runtime 需求。

### 验证项

1. **actor 消息流与 run_loop 集成**：AgentRun 的 run_loop 能否自然地封装在 actor `handle()` 方法中
2. **Kill/Stop 优先级**：当 worker mailbox 积压大量 LLM streaming event 时，Kill signal 能否立即终止
3. **supervision event**：parent actor 能否通过 `handle_supervisor_evt` 正确接收 child 的 started/panicked/stopped 通知
4. **typed API 封装层**：`AgentRef` 方法 → `AgentMsg` enum → Ractor handler 的封装是否自然

### 交付物

```rust
// 最小 PoC 结构
struct WorkerAgent { /* state */ }
struct WatcherAgent { /* state */ }

// WorkerAgent: 接收 Steer/Inject/Cancel 消息，内部运行 agent loop
// WatcherAgent: 消费 worker 事件流，调用 watcher LLM 判断是否干预

// 验证场景：
// 1. watcher 通过 cast! 注入 steering → worker 在下一轮迭代响应
// 2. worker panic → parent 通过 SupervisionEvent 感知并决定 restart/stop
// 3. Kill signal → 打断积压的 streaming event，立即停止
```

### Gate 规则

| 结论 | 条件 | 后续路径 |
|------|------|---------|
| **通过** | 4 项验证全部满足，集成成本可控 | Phase 2 用 Ractor 重构 AgentRun |
| **有条件通过** | 1-3 项满足，个别问题可绕过 | Phase 2 用 Ractor，记录 workaround |
| **未通过** | Kill 优先级或 run_loop 集成存在根本冲突 | Phase 2 保持 channel 原语，v0.9 steering 用 `SteeringBus` 手写 |

### 退出条件

如果 PoC 阶段发现以下问题，退回 channel 方案：
- Ractor 的消息序列化与 tokio 生态存在根本冲突
- actor `handle()` 的 async 边界无法容纳 LLM streaming 的长时 await
- Kill/Stop 优先级在实际 streaming 场景中不生效

## Phase 2：主体交付

### Hook 框架

定义生命周期 hook 点：

```
on_run_start / on_run_end / on_run_error
before_model / after_model
before_tool / after_tool
on_handoff
before_compact
```

`Hook` trait，所有方法有默认空实现，用户只覆写关心的 hook 点。`AgentConfig` 接受 `Vec<Arc<dyn Hook>>`，按注册顺序链式调用。

Core 只提供框架和调用链，不内置任何具体 middleware 实现（极简 Core 原则）。

设计参考：
- OpenAI `RunHooks`（7 回调）— 简洁、事件驱动
- DeerFlow `AgentMiddleware`（6 hook 点）— wrap 模式，可拦截可修改
- Claude SDK `HookMatcher`（10 事件）— 统一回调签名

### Handoff 重构

消除 `__sub_agent_request` 魔法字段，统一为两层语义：

**Agent-as-Tool**：委托子任务，父 agent 拿到结果继续。模型看到的是普通 tool。

```rust
impl AgentConfig {
    pub fn as_tool(&self) -> Arc<dyn Tool> { ... }
    pub fn as_tool_with_schema<S: JsonSchema>(&self, ...) -> Arc<dyn Tool> { ... }
}
```

- 内部同步调用子 run（不 spawn tokio task）
- 子 run 事件通过 `ChildRunEvent` 向上传播
- 结构化输入 schema + 可选 `output_extractor`

**Handoff**：路由会话到另一个 agent，当前 agent 退出。run loop 层面的控制流切换。

```rust
pub struct Handoff {
    pub tool_name: String,
    pub tool_description: String,
    pub input_schema: Value,
    pub target: HandoffTarget,
    pub input_filter: Option<Arc<dyn HandoffInputFilter>>,
    pub nest_history: bool,
}

pub enum HandoffTarget {
    Static(AgentConfig),
    Dynamic(Arc<dyn Fn(Value) -> AgentConfig + Send + Sync>),
}

pub struct HandoffInputData {
    pub input_history: Vec<Message>,
    pub pre_handoff_items: Vec<ContentBlock>,
    pub new_items: Vec<ContentBlock>,
}
```

- `ToolOutput::Handoff(HandoffResult)` 新增变体
- Run loop 中 handoff 分支：记录 tool result → 发出 `AgentUpdated` 事件 → 切换 config/registry/messages → 重新进入 loop
- `HandoffInputFilter`：裁减传递给新 agent 的上下文
- `nest_history`：将上游历史折叠为嵌套消息
- **Budget 处理**：Handoff 时继承父 agent 剩余预算（默认），或通过 `HandoffTarget` 配置独立预算。需修改 `budget.rs`

**需要删除的旧代码**：

| 文件/符号 | 原因 |
|-----------|------|
| `tool/agent.rs` — `AgentTool` | 被 `AgentConfig::as_tool()` 替代 |
| `run.rs` — `execute_sub_agent_request()` | 被 `AgentAsTool::execute()` 替代 |
| `run.rs` — `execute_agent_delegate()` | 被 `AgentAsTool::execute()` 替代 |
| `run.rs` — `__sub_agent_request` 魔法字段 | 不再需要 |
| `tool/mod.rs` — `AgentDelegate` / `ToolOutput::AgentDelegate` | 不再需要 |

**需要新增的**：

| 位置 | 内容 |
|------|------|
| `tool/agent_as_tool.rs` | `AgentAsTool` 实现 |
| `handoff.rs` | `Handoff`、`HandoffTarget`、`HandoffInputFilter`、`HandoffInputData` |
| `events.rs` | `RuntimeEvent::AgentUpdated` |
| `run.rs` | `ToolOutput::Handoff` 分支处理 |

设计文档：[sub-agent-handoff-vs-agent-as-tool.md](../../research/sub-agent-handoff-vs-agent-as-tool.md)

### AgentRun 重构（取决于 PoC 结果）

| 组件 | PoC 通过 | PoC 未通过 |
|------|---------|-----------|
| AgentRun | 重构为 `WorkerActor`（Ractor Actor trait） | 保持 JoinHandle + channel |
| RunHandle | 内部用 `ActorRef<AgentMsg>` 实现，公共 API 不变 | 保持 mpsc channel |
| 事件分发 | Actor 消息分发 | 保持 mpsc，v0.8 改 broadcast |
| Handoff 路由 | ActorRef 替换 + Registry 查找 | config 变量切换 |
| Hook 调用 | actor 生命周期回调 + Hook trait 互补 | Hook trait 独立实现 |

PoC 通过时，在 Ractor 之上封装 typed API：

```rust
// 底层 Ractor enum
enum AgentMsg {
    Steer(SteerCmd, RpcReplyPort<SteerResult>),
    Inject(InjectCmd, RpcReplyPort<()>),
    Cancel(CancelCmd),
}

// 上层 typed API（框架用户看到的）
impl AgentRef {
    pub async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, AgentError> {
        call!(self.inner, AgentMsg::Steer(cmd, _))
    }
    pub fn cancel(&self, cmd: CancelCmd) {
        cast!(self.inner, AgentMsg::Cancel(cmd));
    }
}
```

### LLM Retry

`RetryPolicy` 配置（max_retries、backoff strategy）。可重试错误分类：

- `rate_limit`（429）→ 指数退避
- `server_error`（5xx）→ 固定间隔重试
- `timeout` → 重试
- 其他错误 → 不重试

可以作为 hook 实现（`after_model` 拦截错误并重试），也可以在 loop 层实现。视实现复杂度决定。

### Loop Detection

检测 agent 循环调用同一工具模式，两级防御：

- **warn**：相同工具调用模式出现 N 次后注入警告消息（提示模型换策略）
- **hard stop**：超过上限后强制终止

作为 Hook trait 的具体实现提供。参考 DeerFlow `LoopDetectionMiddleware` 的滑动窗口 + 去重策略。

### 补充 Examples

v0.6 deferred。Hook、Handoff、Retry 的 API 稳定后补齐使用示例。

## 不在范围内

- Session 持久化 → v0.8
- 内置 Guardrail 实现 → v0.8（hook 框架提供基础）
- 权限模型扩展 → v0.8
- Supervised Delegation 基础通信层 → v0.8
- Mid-run Steering API → v0.9
- Supervised Delegation 完整实现 → v0.9
- 新 Provider → v0.9
- crates.io 发包 → v0.9

## 依赖

- hotfix 2026-05-26 全部完成（Error 类型稳定、crate 结构干净、ProviderFactory 就位）
- Phase 2 中 Handoff 重构依赖 Hook 框架先落地（handoff 事件需要通过 `on_handoff` hook 通知消费者）
- Phase 2 中 AgentRun 重构依赖 Phase 1 PoC 结论

## 验收标准

### Phase 1

- [ ] Ractor PoC 代码存在（可为独立 binary 或 integration test）
- [ ] PoC 验证结论写入 `docs/research/actor-model-evaluation.md` 附录
- [ ] Gate 决策明确记录（通过 / 有条件通过 / 未通过）

### Phase 2

- [ ] `Hook` trait 定义完整，所有 hook 点可注册自定义实现
- [ ] `AgentConfig` 支持 `hooks: Vec<Arc<dyn Hook>>`
- [ ] `crates/` 中无 `__sub_agent_request` 字符串
- [ ] `AgentConfig::as_tool()` 可用，子 run 事件正确向上传播
- [ ] `Handoff` 可用，run loop 正确切换 agent 并发出 `AgentUpdated` 事件
- [ ] `HandoffInputFilter` 和 `nest_history` 有测试覆盖
- [ ] `RetryPolicy` 可配置，rate_limit 错误触发重试
- [ ] Loop detection hook 可检测重复工具调用并注入警告
- [ ] 如 PoC 通过：AgentRun 重构为 Ractor actor，typed API 封装层可用
- [ ] `examples/` 目录包含 hook、handoff、retry 的使用示例
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS

## Issues 拆解

| Issue | 标题 | Phase | 核心交付 |
|-------|------|-------|---------|
| [001](./issues/001-ractor-poc/spec.md) | Ractor PoC | Phase 1 | Ractor WorkerAgent + WatcherAgent PoC，验证 4 项集成指标，gate 决策 |
| [002](./issues/002-hook-framework/spec.md) | Hook Framework | Phase 2 | Hook trait（9 hook 点）、HookAction、Context 类型、AgentConfig 集成、run loop 调用链 |
| [003](./issues/003-agent-as-tool/spec.md) | Agent-as-Tool + Old Path Cleanup | Phase 2 | AgentAsTool、AgentConfig::as_tool()、删除 AgentDelegate / __sub_agent_request 双路径 |
| [004](./issues/004-handoff/spec.md) | Handoff + AgentUpdated Event | Phase 2 | Handoff / HandoffTarget / HandoffInputFilter、run loop 控制流切换、AgentUpdated 事件、budget 继承 |
| [005](./issues/005-agent-run-actor/spec.md) | AgentRun Actor Refactor | Phase 2 | WorkerActor（Ractor）、AgentRef typed API、RunHandle 内部重构。**条件执行：仅 001 通过时** |
| [006](./issues/006-llm-retry/spec.md) | LLM Retry | Phase 2 | RetryPolicy / BackoffStrategy、429/5xx 自动重试、ModelRetry 事件 |
| [007](./issues/007-loop-detection/spec.md) | Loop Detection | Phase 2 | LoopDetectionHook（Hook 实现）、滑动窗口、warn + hard stop 两级防御 |
| [008](./issues/008-examples-validation/spec.md) | Examples + Final Validation | Phase 2 | 7 个使用示例、5 个集成验证场景、最终 lint + CI |

## 推荐执行顺序

1. **001 + 002 并行启动**：001 是 PoC（1-2 天），002 是 Phase 2 基础，互不依赖
2. **001 完成后**：gate 决策。通过 → 005 可启动；未通过 → 跳过 005
3. **002 完成后**：003 / 004 / 006 / 007 可并行启动
4. **003 先于 004**：003 清理旧路径并在 ToolOutput 中占位 Handoff variant，004 实现 Handoff 逻辑
5. **008 收尾**：所有 issue 完成后

依赖图：

```
001 (PoC) ──────────────────────────────────> 005 (条件)
                                                │
002 (Hook) ──┬──> 003 (Agent-as-Tool) ──> 004 (Handoff)
             │                                  │
             ├──> 006 (LLM Retry)               │
             │                                  │
             └──> 007 (Loop Detection)          │
                                                │
                              005 + 004 + 006 + 007 ──> 008
```

## v0.7 权威顺序

1. `docs/iteration/v0_7/issues/*/spec.md` 是实施与验收的第一权威。
2. 本文件约束迭代范围、依赖和成功指标。
3. 研究文档（actor-model-evaluation.md、sub-agent-handoff-vs-agent-as-tool.md）是设计参考；若与 issue 验收标准冲突，先更新 issue/spec 再实现。
