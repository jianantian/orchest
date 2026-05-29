# v0.9 Spec：产品成熟度 + Supervised Delegation

## 背景

v0.7-v0.8 完成了扩展性地基（Hook 框架）、sub-agent 语义统一（Agent-as-Tool + Handoff）、持久化（Session）、安全层（Guardrail + 权限模型）、以及 Supervised Delegation 的基础通信层（双向通信、多方事件订阅、Watcher 注册）。

v0.9 基于这些基础，完成两件事：
1. **Supervised Delegation 完整实现**——这是 [Actor Model 评估](../../research/actor-model-evaluation.md) 识别的核心验证 case，也是 Orchest 与竞品差异化的关键能力
2. **产品发布准备**——Provider 扩展、文档、示例、发布流程

## 目标

1. 用户可以在 run 过程中注入新指令（Mid-run Steering），基于 v0.8 双向通信基础暴露公共 API
2. Supervised Delegation 完整可用——watcher LLM 中途干预、崩溃恢复、多 watcher 协调
3. Provider 覆盖扩大，支持主流模型
4. 文档、示例、发布流程就绪

## 范围

### Mid-run Steering

基于 v0.8 建立的双向通信基础，暴露面向用户的高层 API：

```rust
impl RunHandle {
    pub fn abort(&self) { ... }                                    // v0.7 已有
    pub async fn inject_message(&self, msg: Message) { ... }       // 新增
    pub async fn steer(&self, instruction: &str) { ... }           // 新增
}
```

- `inject_message`：向 run 的消息队列注入一条用户消息，模型在下一轮迭代时看到
- `steer`：注入一条系统级指令（如"停止当前方向，改为..."），不作为用户消息呈现

**实现路径**（取决于 v0.7 PoC 结论）：
- Ractor 方案：`inject_message` → `cast!(actor_ref, AgentMsg::Inject(msg))`，`steer` → `cast!(actor_ref, AgentMsg::Steer(instruction))`
- Channel 方案：`inject_message` / `steer` → `steering_tx.send(AgentMessage::...)`

Steering 消息纳入 v0.8 的 session snapshot，恢复时不丢失。

竞品参考：
- pi-agent-core steering queue + follow-up queue

### Supervised Delegation 完整实现

基于 v0.8 的 Watcher trait + 双向通信 + 多方事件订阅，完成端到端的 Supervised Delegation 流程。

#### Watcher LLM

Watcher trait 的 LLM-powered 实现——用另一个模型监控 worker 事件流，自主判断是否干预：

```rust
pub struct LlmWatcher {
    model: Arc<dyn Model>,
    system_prompt: String,
    intervention_criteria: Vec<String>,
}

#[async_trait]
impl Watcher for LlmWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        // 累积事件，周期性调用 watcher LLM 判断
        // 返回 Continue / Steer / Abort
    }
}
```

#### 崩溃恢复

| 方案 | 实现 |
|------|------|
| Ractor | `handle_supervisor_evt` 中根据 `SupervisionEvent::ActorPanicked` 决定 restart 策略 |
| Channel | 手写 spawn + state 恢复逻辑，从 SessionSnapshot 重建 run state |

崩溃恢复与 v0.8 Session 持久化配合——crash 前的状态自动保存，restart 时从 snapshot 恢复。

#### 多 Watcher 协调

支持注册多个 watcher，协调规则：

- 多个 watcher 独立消费事件流（broadcast / actor 多引用）
- Steer 指令按优先级或时间顺序合并
- 任一 watcher 发出 Abort → 立即终止

#### 端到端流程

```
User → start worker agent (long-running task)
         │
         ├── watcher LLM 注册，开始监听事件流
         │
         ├── worker 执行 tool call → emit event → watcher 评估
         │     ├── WatcherAction::Continue → 不干预
         │     ├── WatcherAction::Steer("change direction") → inject steering
         │     └── WatcherAction::Abort("off track") → terminate worker
         │
         ├── worker panic → supervision event → 决定 restart/stop
         │
         └── worker 完成 → emit RunCompleted → watcher 停止
```

### Provider 扩展

基于 hotfix 落地的 `ProviderFactory` trait，新增 provider：

| Provider | 优先级 | 说明 |
|----------|--------|------|
| Google Gemini | P0 | 主流，API 差异较大，需独立 adapter |
| Ollama / 本地模型 | P1 | 本地部署场景，OpenAI 兼容 API |
| Mistral | P2 | OpenAI 兼容，复用 `openai_compat` |

每个新 provider 只需实现 `ProviderFactory` trait，不改 core。

### 文档与示例

- API 参考文档（rustdoc 级别）
- 入门教程（从零到一个可运行的 agent）
- 进阶示例：自定义 Hook、Guardrail、SessionStore、Handoff 编排、Supervised Delegation
- SDK 文档（Python / TypeScript 使用指南）

### 发布准备

- crates.io 发包流程
- 版本号策略（semver）
- CHANGELOG
- CI 增加发布 pipeline

## 不在范围内

- 分布式 agent 编排（多进程 / 多机）——进程内用 actor，跨进程用稳定传输层（NATS / Redis Streams / gRPC），不在本轮范围
- 内置 observability 平台（保持 event stream + tracing 信号暴露，不内置 collector）
- Skill marketplace
- Web UI / Dashboard

## 依赖

- v0.8 Session 持久化（steering 消息需要持久化，崩溃恢复需要 snapshot）
- v0.8 Supervised Delegation 基础通信层（双向通信、多方事件订阅、Watcher trait）
- v0.8 权限模型（provider 扩展后的安全模型需要权限框架支撑）

## 验收标准

- [ ] `RunHandle::inject_message()` 和 `steer()` 可用，run loop 在下一轮迭代时响应
- [ ] Steering 消息纳入 session snapshot，恢复后不丢失
- [ ] `LlmWatcher` 可用——watcher LLM 监控 worker 事件流并自主干预
- [ ] 崩溃恢复可用——worker panic 后根据策略 restart，从 snapshot 恢复状态
- [ ] 多 watcher 可协调——Steer 合并、Abort 立即生效
- [ ] Supervised Delegation 端到端示例可运行
- [ ] Google Gemini adapter 通过集成测试
- [ ] Ollama adapter 通过本地模型集成测试
- [ ] `examples/` 覆盖 hook、guardrail、session、handoff、steering、supervised delegation、多 provider 场景
- [ ] rustdoc 生成完整 API 文档
- [ ] crates.io 发包流程可用
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
- [ ] `bash scripts/lint-check.sh` 全 PASS
