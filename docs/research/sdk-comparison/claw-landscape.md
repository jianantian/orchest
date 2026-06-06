# Claw 产品横向研究：对 Orchest SDK 的参考价值

> **阅读前提**：本文分析的 7 个仓库都是**完整的 agent 产品**（claw 类 = 面向终端用户的 AI 助手平台，含多频道接入、用户管理、Web UI 等）。Orchest 是**底层 SDK**，是他们的发动机而不是整辆车。
>
> 因此本文刻意过滤掉产品层的内容（多频道路由、用户配对、Dashboard UI、技能市场等），只提取在**运行时核心**层面对 Orchest 有参考价值的设计。

---

## 七个仓库速览

| 仓库 | 语言 | 定位特点 |
|------|------|---------|
| **claw-code** | Rust | Anthropic 官方参考实现的 Rust 移植，最接近 Claude Code 本体 |
| **hermes-agent** | Python | 功能最全，支持 200+ 模型、自主创建 skill、持续学习循环 |
| **ironclaw** | Rust | 安全优先，WASM 沙箱隔离工具执行，多租户 PostgreSQL 后端 |
| **nanoclaw** | TypeScript | 极简主义，"小到能读懂"，SQLite 作为进程间通信边界 |
| **nullclaw** | Zig | 极致轻量，678 KB 二进制，< 8 ms 启动，嵌入式/边缘场景 |
| **openclaw** | TypeScript | 功能最复杂，20+ 频道，插件系统，最大的代码量 |
| **zeroclaw** | Rust | 现代 Rust 架构，微内核设计，trait 驱动扩展，RFC 流程 |

---

## 各仓库逐一分析

### 1. claw-code（Rust）

**架构**：`runtime` / `commands` / `api` / `tools` / `telemetry` 多 crate workspace。

**Run loop**：完整 request → stream response → 收集所有 delta → 解析 tool call（XML 包裹格式）→ 顺序执行 → 追加结果 → 循环。循环上限可配置（默认约 10 次），防止无限循环。

**值得注意的设计**：
- **Context compaction**：session token 超过阈值时，在下一轮开始前自动压缩旧消息（`compact_session()`）。压缩发生在 turn 边界，而不是 mid-loop，保持了循环本身的简洁。
- **Green contract**：执行 bash 前做 pattern validation，阻止危险命令。实质是 approval gate 的静态检查前置版本。
- **Hook system**：turn 前后插桩（telemetry / security auditing），不侵入核心 loop。
- **Prompt caching telemetry**：从 response header 追踪 cache hit/miss，这是生产 SDK 必须有的可观测性。

**取舍**：流式输出是半成品——SSE 事件在发出，但 tool call 检测到后会 buffer 整个 response 再分发，没有真正的 token 级 tool call streaming。我们在 spec 里 `ModelAdapter::stream()` 的设计已经比这更彻底。

---

### 2. hermes-agent（Python）

**架构**：`run_agent.py` + provider 抽象 + skills + memory engine，加上 6+ 平台 gateway。

**Run loop**：message → memory context 注入 → build chat request → stream LLM → parse tool calls（标准 OpenAI JSON 格式）→ 并行或顺序执行 → 结果追回 → 循环。

**值得注意的设计**：
- **闭合学习循环**：agent 在复杂任务完成后自动创建 skill，并在后续使用中自我优化。这是产品功能，但暗示了一个架构约束：**skill 的创建路径必须和执行路径分离**，skill 是数据，不是代码。Orchest 现在 SKILL.md 的设计天然满足这一点。
- **Terminal backend 抽象**：7 个后端（本地/Docker/SSH/Modal/Daytona 等）共用同一个 tool 接口。这印证了我们 `ScriptExecutor` trait 的必要性——执行后端可插拔是真实需求，不是过度设计。
- **Session search（FTS5）**：跨 session 全文搜索 + LLM re-ranking。这是 memory 层的能力，不是 runtime 层。对 Orchest 的启示是：memory 插件的接口需要支持 `search` 而不只是 `get`（v0.3 之后的事）。
- **Parallel tool call**：工具可以并行执行（`asyncio.gather`）。Orchest v0.1 刻意做成顺序以保持 approval gate 简单，这是正确的取舍，但 v0.2 的并行需要注意 approval 的并发语义。

**取舍**：Python 的启动开销（约 2s）和异步开销不适合 SDK 场景——用户在自己的 Python 进程里嵌入 SDK，不应该承担这个成本。我们 Rust core + PyO3 binding 的路线完全正确。

---

### 3. ironclaw（Rust）

**架构**：WASM sandbox per tool，PostgreSQL 作为状态中心，multi-tenant gateway。

**Run loop**：消息进入 → 加载 system prompt + tools + WASM runtime → 调用 LLM（`agentic_loop.rs`）→ WASM 容器执行 tool → 结果 → 循环 → 持久化到 DB。

**值得注意的设计**：
- **WASM capability-based sandbox**：每个 tool 在独立 WASM 容器中执行，capability token 阻止未授权 API 调用，secrets 在请求时注入、响应时做泄漏检测。这是我们在 v0.3 `ScriptExecutor` 抽象之后可以考虑的实现路线——`WasmExecutor` 作为 `BareSubprocessExecutor` 之外的选项，适合高安全场景。WASM 的优势是不依赖 OS-level 隔离（firejail/bubblewrap），跨平台沙箱。
- **Self-repair loop**（`self_repair.rs`）：监测卡住的操作，自动检测并恢复。对 Orchest 的启示：`WaitingForAsyncTool` 状态需要 watchdog，防止 job_handle 的 `poll` 闭包永久悬挂（budget guard 的 `max_duration` 是必要但不充分的）。
- **Cost guard**：token 用量 + USD 成本实时追踪，超预算时 block。我们 `BudgetConfig.max_cost_usd` 已经有这个设计，ironclaw 的实现可以作为参考。
- **Parallel tool execution**：`JoinSet` 并发执行多个 tool。

**取舍**：WASM tooling 的构建部署复杂度高，调试困难。对于 Orchest SDK 用户来说，强制要求 WASM 打包工具会大幅提高使用门槛。因此 `WasmExecutor` 只能是可选项，而不是默认实现。

---

### 4. nanoclaw（TypeScript）

**架构**：SQLite 作为 host/container 的 IPC 边界，Docker per session，Claude Agent SDK 原生调用。

**Run loop**：平台消息 → host 写入 `messages_in` → container 轮询 → Claude Agent SDK call → 结果写入 `messages_out` → host 轮询交付。

**值得注意的设计**：
- **DB as IPC boundary**：用 SQLite 把 host 进程和 agent container 彻底解耦，两侧互不持有对方的引用。这是一个优雅的隔离模式——在 Orchest 里，如果未来要支持 sub-agent 跨进程，可以用类似的思路（消息队列或数据库）而不是直接 FFI，保持清晰的边界。
- **"Small enough to understand" 哲学**：源代码故意保持极少，让用户通过修改源代码来定制。这和 Orchest 的定位正好相反——我们的 SDK 需要有稳定的 API，用户不应该需要 fork。但这个哲学提醒我们：**核心 loop 的代码必须保持可读**，不应该让抽象层堆叠到连实现者都看不懂的程度。

**取舍**：Docker per session 带来 1-2s 启动延迟，不适合高频调用场景。SQLite 并发写问题在多 session 时会显现。这个方案是为个人/小团队设计的，不适合作为 SDK 底层参考。

---

### 5. nullclaw（Zig）

**架构**：单一静态二进制，vtable 驱动的 channel/tool/provider，编译期 feature gate，嵌入式 SQLite。

**Run loop**：channel 消息 → session 查找 → 加载 system prompt → stream provider → 解析 tool output → 执行 → 结果追回 → 持久化。完全 streaming-first，token 逐一发出。

**值得注意的设计**：
- **Compile-time feature gates for binary size**：不同的部署目标（IoT 设备、边缘服务器、桌面）通过编译期 feature 选择功能集，而不是运行时插件加载。这与 Rust feature flags 非常类似——Orchest 的 `Cargo.toml` feature 设计可以借鉴这个思路，让 SDK 用户选择编译进去的能力（如 `feature = ["anthropic", "openai", "mcp"]`）。
- **可插拔 memory engine（10 种）**：SQLite / PostgreSQL / Redis / lancedb 等，统一 trait。这验证了我们 memory 插件化的方向。
- **Landlock/firejail/bubblewrap 沙箱**：和我们 `ScriptExecutor` 抽象的目标完全一致，而且已有实现可以参考。特别是 `landlock`（Linux 5.13+）是比 firejail 更现代的方案：纯内核机制，不需要额外 binary，适合在 `BareSubprocessExecutor` 之后实现一个 `LandlockExecutor`。
- **< 8 ms 启动**：Zig 二进制优化极致。Orchest 的 Rust core 在 release build 下也可以做到 < 100 ms，这对 PyO3/napi-rs 绑定用户的体验很重要（不要在 `import agent_runtime` 时卡 1 秒）。

**取舍**：Zig 生态不成熟，第三方库少，难以找到贡献者。编译期定制意味着运行时不可更改（想加一个频道必须重新编译）。这两点都不适合 Orchest 的场景，但架构思路值得借鉴。

---

### 6. openclaw（TypeScript）

**架构**：Plugin-based gateway，4 级路由（channel → account → agent → session），20+ 频道，20+ provider，14,593 源文件。

**值得注意的设计**：
- **Provider 抽象统一多种 tool call 格式**：OpenAI function-calling JSON、Anthropic tool use、Codex responses 全部通过 provider adapter 归一化。这印证了 Orchest `ModelAdapter` trait 的设计方向，但提醒我们：**不同模型的 tool call 格式差异是真实复杂度**，adapter 层不可能完全隐藏，需要 adapter 承担 format 转换责任，而不是 loop 层做条件分支。
- **Tool result truncation**：大输出自动截断 + 摘要。对 Orchest 的启示：`ToolOutput::Immediate(Value)` 的大小需要有限制机制，防止超大 tool 结果污染 context。可以在 `ToolMetadata` 里加 `max_output_tokens` 或在 `ToolContext` 里提供截断 helper。
- **Streaming fallback**：如果 provider 不支持 streaming，自动降级到 batch 模式。Orchest `ModelAdapter` 的 `call()` 和 `stream()` 的关系已经处理了这个——`call()` 是 `stream()` 的 convenience wrapper，反过来也应该是：`stream()` 可以 fallback 到 `call()` 再逐字符 emit，保持上层接口一致。

**取舍**：14k 文件、100+ npm 包、500 MB+ 内存、5s+ 启动——这是产品复杂度积累的结果，不是架构缺陷。对 SDK 而言，这恰恰是反模式：SDK 不应该拖累宿主应用的性能预算。

---

### 7. zeroclaw（Rust）

**架构**：微内核 + trait 驱动扩展点，Provider/Channel/Tool 都是稳定 ABI trait，14 个 workspace crate，RFC 驱动开发。

**Run loop**：inbound → memory + system prompt 加载 → `Provider.chat()` with tools → stream → 解析 tool call → `ToolDispatcher` 分发 → tool 结果 → 循环 → 更新 memory → emit response。

**值得注意的设计**：
- **`TurnEvent` enum**：粒度最细的事件系统——`TextDelta`、`ThinkStart/ThinkDelta/ThinkEnd`、`ToolUse`、`ToolResult`、`MessageStop`。比 Orchest 当前的 `RuntimeEvent` 更细粒度，特别是 `ThinkStart/ThinkDelta/ThinkEnd` 对 extended thinking 的支持值得直接参考——我们的 `ModelStreamChunk::Thinking { delta }` 应该也拆出 start/end 事件，让消费方知道 thinking block 的边界。
- **`ToolDispatcher` trait**：把 tool call 的**解析格式**（XML / JSON / provider-native）从 loop 中抽出。这是我们目前没有的抽象：Orchest 目前假设 model response 的 tool call 格式是固定的（从 `ModelResponse` 里解析），但接入 OpenAI 时格式不同。zeroclaw 的 `XmlToolDispatcher` / 可替换 dispatcher 是更灵活的设计，值得在 v0.2 OpenAI adapter 时一并考虑。
- **Autonomy levels**：`supervised → interactive → autonomous` 三级控制 approval 需求。这比我们 `requires_approval: bool` 更富表达力——某些 tool 在 supervised 模式需要 approval，在 autonomous 模式不需要。可以在 `ToolMetadata` 里加 `approval_policy: ApprovalPolicy` 枚举替代布尔值。
- **RFC-driven development**：重大变更走 RFC 流程，决策有文档可查。Orchest 的 `docs/polaris/` 目录在做同样的事，应该坚持。
- **Response cache**：语义级别的 response 缓存，避免重复执行相同请求。这是 SDK 层可以提供的能力，对重复性 agent task 有很大价值（prompt caching 只解决了 token 成本，response cache 解决的是完整重复调用）。

**取舍**：微内核迁移仍在进行中，部分 RFC 未完成。50+ feature flags 的组合测试几乎不可能穷举。这是 zeroclaw 主动选择灵活性的代价，Orchest 在 v0.x 阶段应该保持 feature flag 数量克制。

---

## 跨项目共同模式：对 Orchest 的直接参考价值

### 一、事件流粒度

**观察**：zeroclaw 的 `TurnEvent` 是 7 个仓库里粒度最细的，claw-code 次之，其余普遍粗糙。

**对 Orchest 的启示**：当前 `RuntimeEvent` 缺少 thinking block 的边界事件。建议在 `ModelStreamChunk` 中补充：

```rust
pub enum ModelStreamChunk {
    Text { delta: String },
    ThinkingStart,                            // extended thinking 开始
    Thinking { delta: String },
    ThinkingEnd,
    ToolCallArgsChunk { id: String, delta: String },
    Done { usage: TokenUsage },
}
```

`ThinkingStart` / `ThinkingEnd` 让消费方（如 TUI）能正确渲染 thinking block 的折叠/展开，不需要自己猜测边界。

---

### 二、Tool 结果大小控制

**观察**：openclaw 有 tool result truncation，ironclaw 有 output size limit per tool，nullclaw 在 `ToolMetadata` 里有 `max_output_bytes`。所有产品级实现都有这个机制。

**对 Orchest 的启示**：`ToolMetadata` 应加 `max_output_tokens: Option<u64>`。超出时 runtime 自动截断并附加截断说明，防止单个 tool 结果把整个 context 撑爆。这在 v0.1 就应该有，可以作为 issue 补进去。

---

### 三、Provider 的 tool call 格式差异

**观察**：zeroclaw 用 `ToolDispatcher` trait 隔离 XML/JSON 格式差异；openclaw 在 provider adapter 层做 normalization；claw-code 强制 XML wrapper（只支持 Anthropic，所以可以做这个假设）。

**对 Orchest 的启示**：v0.2 接入 OpenAI adapter 时，`ModelAdapter` 返回的 `ModelResponse` 里的 tool call 结构应该由 adapter 负责 normalize 成统一格式，而不是 loop 里做 `if provider == anthropic { ... } else { ... }`。在 `ModelAdapter` trait 上加一个 `parse_tool_calls(response: &RawResponse) -> Vec<ToolCall>` 方法，由 adapter 实现（Anthropic adapter 解析 JSON tool use block，OpenAI adapter 解析 function_call / tool_calls 字段）。

---

### 四、Approval policy 的表达力

**观察**：zeroclaw 的 `autonomy_level`（supervised/interactive/autonomous）、ironclaw 的 `ApprovalResponse`、claw-code 的 `requires_approval` 都在解决同一个问题，但 zeroclaw 的方案最灵活。

**对 Orchest 的启示**：`ToolMetadata.requires_approval: bool` 不够用，考虑替换为：

```rust
pub enum ApprovalPolicy {
    Never,                    // 从不需要（默认）
    Always,                   // 总是需要
    WhenAutonomous,           // 只在 autonomous 模式下需要
}
```

对应在 `AgentConfig` 里加 `autonomy_level: AutonomyLevel`（supervised/autonomous）。这在 v0.2 之前不急，但应该作为 v0.1 API 的已知演化方向记录在 polaris 文档里，避免将来 breaking change。

---

### 五、沙箱实现路线

**观察**：
- ironclaw：WASM capability-based（最彻底，最复杂）
- nullclaw：Landlock（Linux 内核机制，轻量）/ firejail / bubblewrap（按 OS 选择）
- claw-code：无沙箱，只有 green contract 静态检查
- zeroclaw：security policy + approval gate（应用层）

**对 Orchest v0.3 之后的参考**：实现 `ScriptExecutor` 的第二个 impl 时，**nullclaw 的 Landlock 路线性价比最高**：
- Landlock 是 Linux 内核原生机制（5.13+），无需外部依赖
- 限制文件系统访问路径，与 `SkillCapabilities.filesystem_read/write` 完全对应
- 不能限制网络（需要 seccomp 配合），但 capabilities 声明 `network: false` 可以记录意图
- macOS 可以用 sandbox-exec（沙盒配置文件）作为 fallback

WASM 路线（ironclaw 方案）更彻底，但对 skill 作者不友好（必须把脚本打包成 WASM 模块）。应该作为高安全档位，而不是默认实现。

---

### 六、Context compaction 的触发时机

**观察**：
- claw-code：在 turn 边界触发（turn 开始前检查）
- hermes：周期性 nudge（agent 主动决定摘要时机）
- openclaw：token 比例阈值（0.8）

**对 Orchest 的启示**：v0.2 的 context compaction 应该在 **每次 model call 之前**检查（而不是 tool call 或 step 结束），因为 model call 是 token 消耗的触发点。触发条件是 `current_tokens / model_context_limit > compaction_threshold`（类似 openclaw，放在 `AgentConfig` 而非 `BudgetConfig`，因为这是 context 管理策略）。

---

## 不值得参考的部分（产品层）

以下是 claw 类产品普遍有、但 Orchest SDK 刻意不做的东西，不需要在架构上为其预留位置：

- **多频道路由**（Telegram/Discord/Slack/WhatsApp）：这是产品层，用 Orchest SDK 的用户自己决定接什么
- **用户管理 + 配对码**：身份认证完全在 SDK 之外
- **Web Dashboard UI**：可观测性通过 `RuntimeEvent` 事件流暴露，UI 是消费方的事
- **Skill marketplace**：skill 的发现和安装是产品功能，SDK 只定义 skill 的加载协议
- **Per-session container 编排**：隔离策略通过 `ScriptExecutor` 插件实现，容器编排在 SDK 之外
- **Provider 账单管理**：token 成本追踪 SDK 可以提供，但账单功能不行

---

## 总结

七个仓库验证了 Orchest 现有设计的几个核心判断：
- `ScriptExecutor` trait 的必要性（hermes 的 7 个 terminal backend，nullclaw 的多沙箱实现，ironclaw 的 WASM）
- `ModelAdapter` 的格式 normalization 责任（所有多模型实现都有 adapter 层）
- Memory 插件化的真实需求（nullclaw 的 10 种引擎，zeroclaw 的 trait 化）
- Budget + cost guard 的标配地位（几乎所有仓库都有）

最值得直接借鉴的具体点（按优先级）：
1. **zeroclaw 的 `TurnEvent` thinking block 边界事件** → 补充到 `ModelStreamChunk`
2. **ToolMetadata 加 `max_output_tokens`** → 防止 context 被大 tool 结果撑爆
3. **ModelAdapter 加 `parse_tool_calls()`** → v0.2 OpenAI adapter 时防止 format 逻辑污染 loop
4. **`requires_approval: bool` 演化为 `ApprovalPolicy` 枚举** → 记录为已知演化方向
5. **nullclaw 的 Landlock 方案** → v0.3 之后 `LandlockExecutor` 的实现参考
