# 001 · Ractor PoC（Phase 1 Gate）

## 背景

当前 runtime 用 tokio channel + JoinHandle 手写 proto-actor 模式（`run/mod.rs` spawn task + `mpsc::channel` + `RunHandle`）。[Actor Model 评估](../../../research/actor-model-evaluation.md) 识别了四项能力缺失：双向通信、监督层级、多方事件订阅、生命周期回调。评估推荐 v0.7 引入 Ractor，但需要先做 PoC 验证集成可行性。

**本 issue 是 gate 决策点**——结论直接决定 005（AgentRun Actor Refactor）是否执行，以及 v0.8/v0.9 的通信架构走向。

## 目标

用 Ractor 实现最小 Supervised Delegation 场景，验证 actor 框架是否满足 Orchest runtime 需求。

## 范围

### 交付物

独立的 PoC 代码（integration test 或独立 binary），不合入 production code。

```rust
// WorkerAgent: 模拟 agent run loop
// - 接收 Steer / Inject / Cancel 消息
// - 内部运行简化的 loop（模拟 LLM call + tool call）
// - 通过事件通道向外发送 RuntimeEvent

// WatcherAgent: 模拟 watcher LLM
// - 消费 worker 事件流
// - 根据事件内容决定 Continue / Steer / Abort

// SupervisorAgent 或 spawn_linked: 管理 worker + watcher 关系
```

### 验证项

| # | 验证项 | 成功标准 |
|---|--------|---------|
| V1 | actor 消息流与 run_loop 集成 | WorkerAgent 的 `handle()` 能自然封装 "检查 mailbox → model call → tool exec → emit event" 循环 |
| V2 | Kill/Stop 优先级 | 当 worker mailbox 积压 100+ 模拟事件时，发送 Kill signal 后 actor 在 <100ms 内停止（不需要处理完积压消息） |
| V3 | supervision event | parent 通过 `handle_supervisor_evt` 收到 child 的 `ActorPanicked` 事件，并能决定 restart 或 stop |
| V4 | typed API 封装层 | `AgentRef::steer()` / `AgentRef::cancel()` 方法 → `AgentMsg` enum → Ractor `handle()` 的封装自然、类型安全 |

### V1 详细验证

Ractor actor 的 `handle()` 是消息驱动的——每条消息触发一次 `handle()` 调用。但 agent run loop 是长时运行的循环。需要验证两种集成模式：

**模式 A：一条 `Run(input)` 消息触发整个 loop**
```rust
async fn handle(&self, myself: ActorRef<Msg>, msg: Msg, state: &mut State) {
    match msg {
        Msg::Run(input) => {
            // 整个 run loop 在一次 handle() 调用中完成
            // 问题：loop 运行期间无法处理其他消息（Steer/Cancel）
        }
    }
}
```

**模式 B：loop 每一步是一条消息，self-message 驱动**
```rust
async fn handle(&self, myself: ActorRef<Msg>, msg: Msg, state: &mut State) {
    match msg {
        Msg::RunStep => {
            // 执行一步（model call 或 tool exec）
            // 完成后 self-send 下一步
            myself.cast(Msg::RunStep)?;
        }
        Msg::Steer(cmd) => { /* 立即响应 */ }
        Msg::Cancel => { /* 立即响应 */ }
    }
}
```

**模式 C：`handle()` 中启动长时 task，通过 select! 同时监听 mailbox**

PoC 需要验证哪种模式最适合 Orchest 的 run loop 结构。模式 B 最可能成功（消息优先级天然生效），但需要验证 self-message 的 overhead 是否可接受。

### Gate 规则

| 结论 | 条件 | 后续路径 |
|------|------|---------|
| **通过** | V1-V4 全部满足，集成成本可控 | 执行 005（AgentRun Actor Refactor） |
| **有条件通过** | V1-V3 满足，V4 需要 workaround | 执行 005，记录 workaround |
| **未通过** | V1 或 V2 存在根本冲突 | 跳过 005，后续用 channel 原语 |

### 退出条件

发现以下任一问题，立即终止 PoC 并记录结论：
- Ractor `handle()` 的 async 边界无法容纳 LLM streaming 的长时 await（模式 A/B/C 均不可行）
- Kill/Stop 优先级在模式 B 的 self-message 场景中不生效（Kill 被 RunStep 消息阻塞）
- Ractor 与 tokio runtime 存在不兼容（如 actor spawn 必须在特定 executor 上）

## 不在范围内

- 修改 production code
- 实现完整的 AgentRun actor（那是 005 的事）
- 分布式 actor（ractor_cluster）
- 性能 benchmark（评估文档已结论：消息开销相比 LLM 延迟可忽略）

## 依赖

无。可与 002（Hook Framework）并行启动。

## 验收标准

- [ ] PoC 代码可编译运行，演示 WorkerAgent + WatcherAgent + steering 注入场景
- [ ] V1-V4 每一项有明确的验证结论（通过/未通过/workaround）
- [ ] 集成模式选择有结论（模式 A/B/C 或其他）
- [ ] Gate 决策明确记录
- [ ] PoC 结论写入 `docs/research/actor-model-evaluation.md` 附录
