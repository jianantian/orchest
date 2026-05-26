# Actor Model 评估：Ractor vs Kameo vs Channel 原语

## 背景

Orchest runtime 当前用 tokio channel + JoinHandle 手写通信原语。随着三种 sub-agent 模式（Agent-as-Tool、Handoff、Supervised Delegation）的通信需求增长，需要评估是否引入 actor 框架。

本文档基于 Supervised Delegation 作为核心验证 case 进行评估——这是对 runtime 通信能力要求最高的场景。

## 现状：Orchest 中的 proto-actor 模式

当前代码已经隐式使用了若干 actor 概念：

| Actor 概念 | Orchest 对应 | 位置 |
|-----------|-------------|------|
| Actor | `AgentRun`（tokio::spawn 任务） | `run/mod.rs:47` |
| Mailbox | `mpsc::channel(256)` | `run/mod.rs:44` |
| Actor Ref | `RunHandle`（run_id + JoinHandle + ApprovalBus） | `run/handle.rs:42-46` |
| Message | `RuntimeEvent` enum（26 variants） | `events.rs:13` |
| Supervision Signal | `CancellationToken`（规划中） | v0.7 spec |
| Child Spawn | `AgentRun::start_with_bus()` | `run/sub_agent.rs:48` |

**缺失的 actor 能力：**

1. **双向通信**：RunHandle 只有 `wait()` 和 `respond_approval()`，无法向运行中的 agent 注入消息或 steering 指令
2. **监督层级**：parent-child 通过共享 ApprovalBus 弱关联，无结构化监督树、无 restart 策略
3. **多方事件订阅**：event channel 是 single-consumer（`mpsc::Receiver`），watcher LLM 无法独立消费事件流
4. **Actor 生命周期回调**：无 pre_start / post_stop / on_panic 等生命周期 hook

## 候选框架

### Ractor

Erlang/OTP 风格。重量级监督，完整的 actor 原语。定位：Rust 版 Erlang gen_server / OTP primitive。

```rust
#[async_trait]
trait Actor: Send + Sync + 'static {
    type Msg: Send + 'static;
    type State: Send + 'static;
    type Arguments: Send;

    async fn pre_start(&self, myself: ActorRef<Self::Msg>,
                       args: Self::Arguments) -> Result<Self::State, ActorProcessingErr>;
    async fn handle(&self, myself: ActorRef<Self::Msg>,
                    message: Self::Msg, state: &mut Self::State)
                    -> Result<(), ActorProcessingErr>;

    // provided: post_start, post_stop, handle_supervisor_evt
}
```

**消息模型：一个 actor 一个 `Msg` enum。** 所有消息类型集中在一个 enum 中，通过 match 分发。接近 Erlang/Akka/Actix 传统思路。协议集中，适合"服务进程"建模；但 agent 能力变多时 enum 容易膨胀。

**通信原语：**
- `cast!` — fire-and-forget（告知）
- `call!` / `call_t!` — request-reply with timeout（询问）
- `forward!` — RPC 成功后把结果转发给另一个 actor（适合 agent pipeline）
- `ActorRef<Msg>` — 类型安全的 actor 引用
- `SupervisionEvent` — parent 收到 child 的 start/panic/stop 通知

**消息优先级（四级）：**
1. Signal（最高，如 Kill）——立即终止当前 async work
2. Stop——不打断正在运行的 handler，下一轮优先处理
3. SupervisionEvent
4. 普通 user messages（最低）——同一 sender FIFO，不同 sender 无 ordering guarantee

**额外能力：**
- `ActorProcessingGroup` — 进程组，支持广播、random dispatch、partial-broadcast
- `factory` 模块 — 工作池模式
- `registry` — 全局 actor 注册表（by name 查找）
- `message_span_propagation` feature — actor 间 tracing span 传播
- `ractor_cluster` — 分布式（TCP NodeSession，官方声明不应认为 production ready）

**依赖权重：** ractor 0.15+ 依赖 tokio、dashmap。编译增量可控（纯 Rust，无 proc macro 依赖）。版本迭代稳定，Meta 内部有使用案例。

### Kameo

轻量级，derive 宏驱动，typed message handler 优先。定位：现代 Rust async 风格的 typed actor library，强调 ergonomics。

```rust
trait Actor: Sized + Send + 'static {
    type Args: Send;
    type Error: ReplyError;

    // required
    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>)
        -> Result<Self, Self::Error>;

    // provided
    fn name() -> &'static str { type_name::<Self>() }
    fn supervision_strategy() -> SupervisionStrategy { OneForOne }
    async fn on_panic(&mut self, _err: PanicError) -> Result<Option<ActorStopReason>, Self::Error> { ... }
    async fn on_link_died(&mut self, _id: ActorID, _reason: ActorStopReason)
        -> Result<Option<ActorStopReason>, Self::Error> { ... }
    async fn on_stop(self, _reason: ActorStopReason) -> Result<(), Self::Error> { ... }
    async fn on_message(&mut self, msg: Box<dyn Any + Send>) { ... }
    async fn next(&mut self) -> Option<()> { ... }
}
```

**消息模型：一个 actor 实现多个 `Message<T>` handler。** 任意 static 类型都可以作为 message，每个 message 自带 `Reply` 关联类型。handler 拿 `&mut self`，不需要锁。协议分散但模块化，适合 agent 内部多命令建模：

```rust
// 一个 agent 暴露多个 typed command
impl Message<RunTask> for AgentActor { type Reply = TaskResult; ... }
impl Message<AddMemory> for AgentActor { type Reply = (); ... }
impl Message<Steer> for AgentActor { type Reply = (); ... }
impl Message<CancelTask> for AgentActor { type Reply = Ack; ... }
```

**通信原语：**
- `actor_ref.ask(msg)` — request-reply（支持 mailbox timeout + reply timeout）
- `actor_ref.tell(msg)` — fire-and-forget
- `ActorRef<A>` — 强类型引用，泛型参数是 Actor 而非 Msg

**Supervision：**
- 策略：OneForOne / OneForAll / RestForOne（声明式）
- Restart policy：Permanent / Transient / Never
- Restart limit（默认 5 restarts within 5 seconds）
- Peer-to-peer actor linking + `on_link_died` 回调

**额外能力：**
- `#[derive(Actor)]` — 省去 boilerplate
- mailbox 可配置 bounded/unbounded
- 分布式基于 libp2p（Kademlia DHT registry + request-response）

**依赖权重：** kameo 0.20 依赖 tokio + proc_macro。derive 宏增加编译时间，但 API 更简洁。版本迭代较快（0.20 于 2026-04-07 发布），API 变化风险较高。

## 性能对比

基于独立 benchmark（Rust 1.84.0，MacOS，Quad-Core i7 / 16GB，Kameo 0.14.0，Ractor 0.14.6）：

### 消息处理（100,000 条消息）

| 场景 | Kameo | Ractor | 差距 |
|------|-------|--------|------|
| Fire-and-forget 单线程 (tell/cast) | **13.95 ms** | 36.83 ms | Kameo ~2.6x 快 |
| Fire-and-forget 多线程 (tell/cast) | **27.48 ms** | 34.51 ms | Kameo ~1.3x 快 |
| Request-reply 单线程 (ask/call) | **85.92 ms** | 121.69 ms | Kameo ~1.4x 快 |
| Request-reply 多线程 (ask/call) | **1.157 s** | 1.171 s | 基本持平 |

### Actor 创建（10,000 个 actor）

| 框架 | 耗时 |
|------|------|
| Kameo | **47.91 ms** |
| Ractor | 68.51 ms |
| Actix（参考） | 5.03 ms |

数据来源：[rust-actors-benches](https://github.com/xuchaoqian/rust-actors-benches)，[actor-benchmarks (Kameo author)](https://github.com/tqwewe/actor-benchmarks)

### 对 Orchest 的影响

**性能差距不影响选型。** Kameo 在所有场景都快于 Ractor，但绝对差距在微秒级（单条消息 ~0.14μs vs ~0.37μs）。Orchest 的瓶颈在 LLM 调用（数百毫秒至数十秒）和 tool 执行（IO-bound），actor 消息传递开销相比之下完全可忽略。多线程 request-reply（最接近真实 agent 交互场景）两者基本持平。

## 三种 Sub-agent 模式的通信需求

### 1. Agent-as-Tool

主 agent 调用 sub-agent 作为 tool，等待结果。

| 需求 | 当前实现 | Actor 如何改善 |
|------|---------|---------------|
| 同步等待 | `child_rx.recv()` 循环 | `call!` / `ask()` 内置 |
| 预算传播 | 手动 `record_external_usage` | Actor state 封装 |
| 事件转发 | `SubAgentEvent` wrapper | Actor 消息转发或 PubSub |
| 深度限制 | `run_depth >= 3` 硬编码检查 | 监督树天然有层级 |

改善程度：**中等**。当前方案可工作，actor 主要简化了 boilerplate。

### 2. Handoff

当前 agent 将控制权交给另一个 agent，自身退出。

| 需求 | 当前实现 | Actor 如何改善 |
|------|---------|---------------|
| 状态转移 | 无统一机制（v0.7 重构目标） | Actor 消息传递上下文 |
| 身份切换 | 双路径问题（v0.7 要解决） | ActorRef 替换，registry 查找 |
| 资源回收 | 无 | `post_stop` 回调 |

改善程度：**中等偏高**。Handoff 的"交接"语义与 actor start/stop 生命周期天然契合。

### 3. Supervised Delegation（核心验证 case）

长时 agent 运行，watcher LLM 监控事件流并中途干预。

| 需求 | 当前实现 | Actor 如何改善 |
|------|---------|---------------|
| 长时运行 | JoinHandle（可以，但无生命周期管理） | Actor 生命周期 + restart |
| 事件流消费 | single-consumer mpsc（watcher 需要独立流） | actor 消息分发 / PubSub |
| Mid-run Steering | **无**（RunHandle 只有 wait + approval） | `cast!` / `tell` 注入消息 |
| AsyncJob poll | 阻塞 poll 循环 + sleep | Actor 定时器 / `next()` |
| 崩溃恢复 | **无** | SupervisionEvent / on_panic |
| 多 watcher | **不支持** | 多个 ActorRef 引用同一 actor |

改善程度：**高**。这是 channel 原语最吃力的场景，也是 actor 框架价值最明显的地方。

## 核心对比

### API 模型与人体工学

| 维度 | Channel 原语 | Ractor | Kameo |
|------|-------------|--------|-------|
| **消息模型** | 单一 enum | 一个 actor 一个 Msg enum | 一个 actor 多个 Message\<T\> handler |
| **协议演进** | 改 enum + 改 match | enum 膨胀，需拆 actor 缓解 | 新增 `impl Message<NewCmd>` 即可 |
| **Reply 类型** | 需手写 oneshot | RpcReplyPort（单一类型） | 每个 message 自带 Reply 关联类型 |
| **Request-Reply** | 需手写 oneshot | `call!` / `call_t!` 宏 | `ask()` 方法 |
| **Fire-and-Forget** | `tx.send()` | `cast!` 宏 | `tell()` 方法 |
| **Pipeline 转发** | 无 | `forward!` 宏 | 无内置 |
| **学习曲线** | 低（团队已掌握） | 中高（OTP 概念） | 中（derive 宏降低门槛） |
| **Macro 风格** | N/A | 过程宏较少，函数式宏 | `#[derive(Actor)]` |

Kameo 的 typed message 模式对 agent 协议建模更友好。当 agent 能力随迭代增长时（v0.7 加 steering、v0.8 加 session 操作、v0.9 加 inject），新增 `impl Message<T>` 比扩展一个大 enum 更自然。

### 运行时语义

| 维度 | Channel 原语 | Ractor | Kameo |
|------|-------------|--------|-------|
| **消息优先级** | 无 | 四级（Signal > Stop > Supervision > User） | 无显式优先级 |
| **Cancel/Kill 语义** | CancellationToken（手动） | Kill 立即终止 / Stop 优雅停止 | actor lifecycle，但不如 Ractor 细 |
| **监督树** | 需自建 | 完整（SupervisionEvent 区分 started/panicked/stopped） | 声明式策略（OneForOne/OneForAll/RestForOne） |
| **生命周期回调** | 无 | pre/post_start/stop + handle_supervisor_evt | on_start/stop/panic/link_died |
| **PubSub / 广播** | 需 broadcast channel | ProcessGroup（random/broadcast/partial） | 需自建 |
| **Actor 查找** | 无 | Registry（by name）+ pg（named groups） | 分布式 registry（Kademlia），本地无内置 |
| **Worker Pool** | 无 | factory 模块 | 需自建 |
| **Tracing** | 手动 | message_span_propagation feature | tracing 支持，但无 span 传播 |
| **依赖增量** | 0 | ~3 crates | ~2 crates + proc_macro |
| **生产成熟度** | N/A | 较成熟（v0.15+，Meta 使用） | 较新（v0.20，迭代快） |

Ractor 在运行时语义上更完整：消息优先级对 Cancel 场景至关重要，registry/pg 对 Handoff 路由和多 watcher 管理不可或缺，span propagation 对复杂 agent trace 有直接价值。

## Supervised Delegation 伪代码对比

### 方案 A：Channel 原语（当前方向 + v0.7-v0.9 增量）

```rust
// v0.7: RunHandle 扩展
pub struct RunHandle {
    run_id: RunId,
    task: JoinHandle<()>,
    approval_bus: ApprovalBus,
    steering_tx: mpsc::Sender<SteeringMessage>,     // v0.9 新增
    event_tx: broadcast::Sender<RuntimeEvent>,       // 需改为 broadcast
}

// v0.9: Watcher LLM 消费事件流
let (handle, event_rx) = AgentRun::start(config, input, model, registry);
let watcher_rx = handle.subscribe_events();          // broadcast::Receiver
tokio::spawn(async move {
    while let Ok(event) = watcher_rx.recv().await {
        if watcher_llm.should_intervene(&event).await {
            handle.steer("change direction to ...").await;
        }
    }
});

// 问题：
// 1. broadcast channel 的 Lagged 错误处理
// 2. steering_tx 与 run_loop 的协调需手写
// 3. 崩溃重启需手写 spawn + state 恢复
// 4. 多 watcher 的生命周期管理
```

### 方案 B：Ractor

```rust
struct WorkerAgent { /* state */ }

enum WorkerMsg {
    Steer(String),
    InjectMessage(Message),
}

#[async_trait]
impl Actor for WorkerAgent {
    type Msg = WorkerMsg;
    type State = AgentRunState;
    type Arguments = AgentConfig;

    async fn pre_start(&self, myself: ActorRef<WorkerMsg>, config: AgentConfig)
        -> Result<AgentRunState, ActorProcessingErr> {
        // 初始化 run state
    }

    async fn handle(&self, myself: ActorRef<WorkerMsg>, msg: WorkerMsg, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr> {
        match msg {
            WorkerMsg::Steer(instruction) => state.inject_steering(instruction),
            WorkerMsg::InjectMessage(msg) => state.add_to_queue(msg),
        }
    }

    async fn handle_supervisor_evt(&self, myself: ActorRef<WorkerMsg>,
        msg: SupervisionEvent, state: &mut AgentRunState)
        -> Result<(), ActorProcessingErr> {
        // parent 处理 child 崩溃 / 完成
    }
}

// 主 Agent 使用
let (worker_ref, _) = Actor::spawn(None, WorkerAgent, config).await?;
let (watcher_ref, _) = Actor::spawn_linked(None, WatcherAgent, worker_ref.clone(),
                                            worker_ref.clone()).await?;

// watcher 通过 cast! 注入 steering
cast!(worker_ref, WorkerMsg::Steer("stop current direction".into()));
```

### 方案 C：Kameo

```rust
#[derive(Actor)]
struct WorkerAgent { /* state */ }

struct Steer(String);
impl Message<Steer> for WorkerAgent {
    type Reply = ();
    async fn handle(&mut self, msg: Steer, _ctx: Context<'_, Self, Self::Reply>) {
        self.inject_steering(msg.0);
    }
}

impl Actor for WorkerAgent {
    type Args = AgentConfig;
    type Error = WorkerError;

    async fn on_start(config: AgentConfig, actor_ref: ActorRef<Self>)
        -> Result<Self, WorkerError> {
        // 初始化
    }

    async fn on_link_died(&mut self, id: ActorID, reason: ActorStopReason)
        -> Result<Option<ActorStopReason>, WorkerError> {
        // 处理关联 actor 崩溃
        Ok(None)
    }
}

// 使用
let worker_ref = kameo::spawn(WorkerAgent::new(config));
let watcher_ref = kameo::spawn(WatcherAgent::new(worker_ref.clone()));
worker_ref.link(&watcher_ref).await;

// steering
worker_ref.tell(Steer("change direction".into())).await?;
```

## 面向 Orchest 三种模式的选型矩阵

| 选型维度 | Kameo | Ractor | 判断 |
|---------|-------|--------|------|
| typed ask/reply | 很自然，每个 message 有 Reply | 可做，但更偏 RPC port / enum | Kameo 胜 |
| actor API ergonomics | 简洁，上手快 | 更 OTP-ish，样板稍多 | Kameo 胜 |
| supervision 现成策略 | 声明式 OneForOne/OneForAll/RestForOne | 通过 supervision events，更底层 | Kameo 更快上手，Ractor 更可控 |
| cancel / stop / kill 语义 | 有 actor lifecycle，但优先级不细 | priority channel、Kill、Stop 语义清楚 | **Ractor 胜** |
| registry / process group | 分布式 registry 有，本地 group 不突出 | named registry + pg process groups | **Ractor 胜** |
| worker pool / factory | 需自建 | 有 factory module | Ractor 小胜 |
| tracing / span propagation | tracing 支持 | 有 message span propagation feature | **Ractor 胜** |
| 分布式 actor | libp2p + Kademlia | ractor_cluster + TCP NodeSession | 看架构偏好；都谨慎 |
| 生产成熟度 | 较新，发展快 | 更成熟，文档语义更严肃 | Ractor 胜 |
| 多 agent prototype | 很舒服 | 可以，但重一点 | Kameo 胜 |
| 长期服务端 agent runtime | 可用 | 更合适 | Ractor 胜 |

**关键发现**：Kameo 在 API 人体工学上明显更优（typed message、协议演进）；Ractor 在运行时语义上明显更优（优先级、registry、tracing）。没有一方全面碾压。

## 风险与成本分析

### 引入 Actor 框架的成本

1. **重写量**：AgentRun / RunHandle / sub_agent.rs / events.rs 核心模块需要重构（~600 行直接相关代码，间接影响更大）
2. **概念负荷**：团队需要理解 actor 语义、消息流转、监督策略（Ractor 的 OTP 心智负担更重）
3. **调试复杂度**：消息传递比直接函数调用更难 trace
4. **依赖风险**：Kameo 版本迭代快（v0.20），API 变化风险较高；Ractor 更稳定但更重
5. **Msg enum 膨胀风险**（Ractor 特有）：agent 协议复杂时，一个 actor 一个大 enum 会变得笨重

### 不引入 Actor 的成本

1. **渐进复杂化**：v0.7 → v0.9 需要逐步手写双向 channel、broadcast、steering queue、restart logic
2. **重复发明**：broadcast + steering + lifecycle 回调 + supervision ≈ 不完整的 actor 系统
3. **一致性风险**：多个 sub-agent 模式各自的通信逻辑分散，缺乏统一抽象

### "先 Kameo 再迁 Ractor"的迁移成本

有一种思路是先用 Kameo 做 prototype，产品化时迁到 Ractor。**这个路径的迁移成本不应低估**——Kameo 的 `Message<T>` 多 handler 模型和 Ractor 的 `Msg` enum 模型是根本不同的代码组织方式，迁移不是换个 import 的事，而是重新设计所有 actor 的消息协议。

### 时间线影响

| 方案 | v0.7 影响 | v0.8 影响 | v0.9 影响 |
|------|----------|----------|----------|
| Channel 原语 | 无变化 | 需自建 session 序列化 | steering + broadcast 手写量大 |
| v0.7 引入 Actor | **延期 1-2 周**（重构 AgentRun） | Session = actor state 持久化 | steering 直接用消息注入 |
| v0.9 引入 Actor | 无变化 | 无变化 | **大规模重构**，风险集中 |

## 评估结论

### 推荐：v0.7 引入 Ractor，在其上封装 Kameo 风格的 typed API

**核心判断：Orchest 是框架，不是应用。**

如果 Orchest 只是一个 agent 应用，Kameo 的 ergonomics 优势足以让它成为首选。但 Orchest 是给别人用的 runtime——我们选的 actor 框架会成为框架用户的隐式依赖。这要求更高的 API 稳定性和运行时语义完整性。

**选 Ractor 的理由（运行时语义不可妥协）：**

1. **消息优先级**：Supervised Delegation 场景中，用户发 Cancel 时 worker mailbox 可能积压大量 LLM streaming event。Ractor 的四级优先级（Signal > Stop > Supervision > User）是硬需求，Kameo 没有这个能力
2. **Registry / Process Group**：Handoff 的"路由到目标 agent"需要 by-name 查找；多 watcher 管理需要 group broadcast。Ractor 的 registry + pg 直接可用，Kameo 本地无内置
3. **Tracing span propagation**：复杂 agent trace（user request → planner → researcher → coder → sandbox）在 Ractor 中有 `message_span_propagation` feature 直接支持
4. **生产成熟度**：Ractor v0.15+ 版本稳定，Meta 内部有使用案例；Kameo v0.20 迭代快，作为框架底层依赖风险较高

**但要借鉴 Kameo 的 typed message 模式：**

Ractor 的 Msg enum 在 agent 协议复杂时会膨胀。我们应该在 Ractor 之上封装一层 typed message 体验：

```rust
// 底层用 Ractor 的 enum 模型
enum AgentMsg {
    Steer(SteerCmd, RpcReplyPort<SteerResult>),
    Inject(InjectCmd, RpcReplyPort<()>),
    Cancel(CancelCmd),
    // ...
}

// 上层提供 Kameo 风格的 typed API
impl AgentRef {
    pub async fn steer(&self, cmd: SteerCmd) -> Result<SteerResult, AgentError> {
        call!(self.inner, AgentMsg::Steer(cmd, _))
    }
    pub fn cancel(&self, cmd: CancelCmd) {
        cast!(self.inner, AgentMsg::Cancel(cmd));
    }
}
```

这样框架用户看到的是 typed method（不需要知道 Ractor），内部保留完整的 actor 语义。

**有条件地：**

1. **先做 PoC**（1-2 天）：用 Ractor 实现一个最小 Supervised Delegation，验证：
   - actor 消息流与 run_loop 的集成是否自然
   - Kill/Stop 优先级是否真的能打断积压的 streaming event
   - supervision event 能否满足 watcher LLM 的需求
2. **渐进引入**：先将 AgentRun 重构为 Actor，保持 RunHandle 公共 API 不变（内部用 ActorRef 实现）
3. **设退出条件**：如果 PoC 阶段发现 Ractor 的消息序列化或 async 边界与 tokio 生态冲突，退回 channel 方案

### 不推荐的路径

**"先 Kameo 后迁 Ractor"**——Kameo 的 `Message<T>` 多 handler 模型和 Ractor 的 `Msg` enum 模型是根本不同的代码组织方式。如果最终需要 Ractor 的运行时能力（优先级、registry、pg），迁移代价不亚于从零引入。不如一步到位。

**"直接用任一框架的分布式层"**——Kameo 的 libp2p 和 Ractor 的 ractor_cluster 都不应在早期当作可靠消息层。进程内用 actor，跨进程用稳定传输层（NATS / Redis Streams / gRPC）。等本机模型跑通后再考虑 remote actor。

### 备选判断

如果以下条件成立，应该选择 **保持 Channel 原语**：

- Supervised Delegation 的优先级降低或延后到 v1.0+
- 团队评估后认为 actor 概念负荷过高
- PoC 揭示 Ractor 与现有 runtime 的集成成本超过预期

如果选择 Channel 方案，v0.9 的 Mid-run Steering 需要提前设计统一的 `SteeringBus`（类似 ApprovalBus 但支持任意消息），避免 v0.7-v0.8 的实现成为 v0.9 的改造负担。

## 下一步

1. [ ] 在 v0.7 启动前完成 Ractor PoC
2. [ ] PoC 交付物：用 Ractor 实现 `WorkerAgent` + `WatcherAgent`，验证 steering 注入 + 事件消费 + 崩溃重启 + Kill 优先级
3. [ ] PoC 同时验证 typed API 封装层的可行性（AgentRef 方法 → AgentMsg enum → Ractor handler）
4. [ ] PoC 结论写入本文档的附录
5. [ ] 基于 PoC 结论决定 v0.7 的 AgentRun 实现方式
