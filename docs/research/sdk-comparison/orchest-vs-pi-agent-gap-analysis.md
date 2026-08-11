# orchest vs pi-agent-core 差距分析

> 日期: 2026-05-23
> 基线版本: orchest `c7bf76a` (feat: add core agent tools) / pi-agent-core `v0.75.5`
>
> **历史基线说明（2026-08-11）：** 本文保留当时的比较结论，不代表当前
> Prime Agent。Prime Agent 后续加入 RLM、retained child agents、agent-to-agent
> messaging、daemon continuity、persistent goals、heartbeats、schedules 与
> continual harness。当前研究入口见
> [`orchest-vs-prime-agent-runtime-lessons.md`](./orchest-vs-prime-agent-runtime-lessons.md)。

## 1. 代码规模概览

| 维度 | orchest (Rust) | pi-agent-core (TypeScript) |
|------|---------------|---------------------------|
| 核心运行时 | `run.rs` 3802 行 | `agent-loop.ts` 742 行 + `agent.ts` 557 行 |
| 总源代码 | ~13200 行 (core) + 1182 行 (SDK) | ~4780 行 |
| 模型抽象 | `model/` 918 行 | 依赖外部 `@earendil-works/pi-ai` |
| 工具系统 | `tool/` 2005 行 (8 模块) | 内联在 agent-loop + types |
| 技能系统 | `skill/` 1478 行 | `harness/skills.ts` 375 行 |
| 会话持久化 | 无 | `harness/session/` 6 个文件 |
| 压缩系统 | `maybe_compact_context` ~100 行 | `harness/compaction/` 1161 行 |
| 测试 | 100 (81 core + 12 e2e + 5 v03 + 2 node) | 3 个测试文件 |

## 2. 架构对比

### 2.1 分层

**orchest** 采用扁平化架构:

```
AgentRun::start() → run_loop()
  ├── model.stream()
  ├── registry.get() → tool.execute()
  ├── execute_sub_agent_request()    // JSON 协议
  ├── execute_agent_delegate()       // 原生 AgentTool
  ├── poll_async_job()
  └── maybe_compact_context()
```

核心逻辑集中在 `run.rs` (3802 行), 包含运行循环、子 agent、审批、预算、压缩和全部单元测试。

**pi-agent-core** 采用三层架构:

```
AgentHarness (会话管理、技能、压缩、持久化)
  └── Agent (状态管理、事件订阅、队列)
       └── agentLoop() (LLM 调用、工具执行)
```

职责按层隔离, 每层 500-1000 行。

### 2.2 运行模型

| 维度 | orchest | pi-agent-core |
|------|---------|---------------|
| 调用方式 | `AgentRun::start()` → tokio task | `agent.prompt()` / `agent.continueRun()` |
| 事件传递 | `mpsc::channel<RuntimeEvent>` | `subscribe()` 回调 + `EventStream` |
| 并发模型 | tokio async runtime | 单线程 Node.js event loop |
| 取消机制 | 无（需手动 drop handle） | `AbortController` / `AbortSignal` |
| 中途干预 | 无（除 approval） | steering queue + follow-up queue |

## 3. 逐项差距分析

### 3.1 LLM 提供商覆盖

| 提供商 | orchest | pi-agent-core |
|--------|---------|---------------|
| Anthropic | ✅ `anthropic.rs` 570 行 | ✅ (via pi-ai) |
| OpenAI | ✅ `openai.rs` 543 行 | ✅ (via pi-ai) |
| Google Gemini | ❌ | ✅ |
| Mistral | ❌ | ✅ |
| AWS Bedrock | ❌ | ✅ |
| Azure OpenAI | ❌ | ✅ |
| xAI (Grok) | ❌ | ✅ |
| Groq | ❌ | ✅ |
| DeepSeek | ❌ | ✅ |
| **合计** | **2** | **9** |

**差距等级: 🔴 关键差距**

orchest 的 `ModelAdapter` trait 已有良好抽象 (`stream` / `call` 方法对), 扩展新提供商只需实现 trait, 不需要修改核心。pi-agent-core 通过外部包 `@earendil-works/pi-ai` 集中管理提供商, 包含 streaming、retry、transport 选择等通用逻辑。

**建议**: 优先实现 Google Gemini 和 AWS Bedrock adapter, 再考虑 Azure OpenAI。每个 adapter 预估 400-600 行。可参考 pi-ai 的 transport 抽象增加一个 `ProviderRegistry` 来动态注册。

### 3.2 会话持久化

| 能力 | orchest | pi-agent-core |
|------|---------|---------------|
| 会话存储 | ❌ 纯内存 | ✅ JSONL 追加式存储 |
| 存储后端 | 无 | `JsonlStorage` + `MemoryStorage` |
| 会话树结构 | ❌ | ✅ 树状分支 (parent-child entry) |
| 分支导航 | ❌ | ✅ `navigateTree()` |
| 断点续跑 | ❌ | ✅ `buildSessionContext()` 重放 |
| 模型切换记录 | ❌ | ✅ `model_change` entry |
| 标签系统 | ❌ | ✅ `LabelEntry` |
| 自定义消息 | ❌ | ✅ `CustomEntry` / `CustomMessageEntry` |

**差距等级: 🔴 关键差距**

pi-agent-core 的会话系统由以下组件构成:

- **SessionStorage**: 抽象接口, 支持 JSONL 文件和内存两种实现
- **SessionTreeEntry**: 联合类型, 包含 `message` / `model_change` / `thinking_level_change` / `compaction` / `branch_summary` / `label` / `custom_message` / `info` 等 8 种 entry
- **Session**: 有状态包装, 提供 `append` / `navigateTree` / `buildPath` 等操作
- **buildSessionContext()**: 从 entry 序列重建完整对话上下文

orchest 的 `RunState` 有 `Serialize`/`Deserialize` 派生, 但 `available_tools: Vec<Arc<dyn Tool>>` 标注了 `#[serde(skip)]`, 工具状态无法序列化。实际上从未写入磁盘。

**建议**: 设计一个轻量 SessionStore trait:
```rust
#[async_trait]
trait SessionStore: Send + Sync {
    async fn append(&self, session_id: &str, entry: SessionEntry) -> Result<(), Error>;
    async fn load_path(&self, session_id: &str) -> Result<Vec<SessionEntry>, Error>;
    async fn navigate(&self, session_id: &str, entry_id: &str) -> Result<Vec<SessionEntry>, Error>;
}
```
先实现 JSONL 和 Memory 两个后端。Entry 枚举参考 pi-agent 的设计但简化为 `Message` / `Compaction` / `Metadata` 三类。

### 3.3 上下文压缩

| 能力 | orchest | pi-agent-core |
|------|---------|---------------|
| 触发条件 | token 占比超阈值 | 自定义 `transformContext` |
| 压缩粒度 | 全量：旧消息 → 单条摘要 | 分支级：每个分支独立摘要 |
| 分支摘要 | ❌ | ✅ `branch-summarization.ts` 262 行 |
| 压缩工具 | `model.call()` 直接摘要 | 专用 compaction 模块 755 行 |
| 防振荡 | 5 步最小间隔 | 基于 token 精确计算 |
| 实现复杂度 | ~100 行 | ~1161 行 |

**差距等级: 🟡 中等差距**

orchest 的压缩是可用的, 但粗粒度。pi-agent-core 的 `compaction/` 模块支持:
- 精确 token 估算 (`utils.ts` 144 行)
- 保留第一条被保留消息的 ID (`firstKeptEntryId`) 用于重建
- 分支摘要：切换分支时生成分支级摘要而非丢弃

**建议**: 短期内保持现有压缩逻辑, 但增加 token 估算精度。中期引入分支摘要需要先有会话持久化的树状结构作为前提。

### 3.4 工具系统

| 能力 | orchest | pi-agent-core |
|------|---------|---------------|
| 工具注册 | `ToolRegistry` HashMap | 数组 `AgentTool[]` |
| 工具权限 | ✅ `allowed_tools` / `allowed_skills` | ❌ |
| 执行时拦截 | ✅ 未注册工具返回 error | ❌ |
| MCP 支持 | ✅ stdio + HTTP 双传输 | ❌ |
| 审批流程 | ✅ oneshot channel 路由 | ❌ |
| Hook 系统 | ❌ | ✅ `beforeToolCall` / `afterToolCall` |
| 工具搜索 | ✅ `SearchToolsTool` | ❌ |
| 异步工具 | ✅ `AsyncJob` polling + webhook | ❌ |
| Agent-as-tool | ✅ `AgentTool` + `AgentDelegate` | ❌ |
| 代码执行 | ✅ `CodeExecutionMcpServer` | ❌ |
| 工具执行模式 | 顺序 | ✅ `sequential` / `parallel` 可选 |
| 参数校验 | 委托给 LLM | ✅ `validateToolArguments` (TypeBox schema) |
| 输出截断 | ✅ `truncate_output` (UTF-8 safe) | ❌ |

**差距等级: 🟢 orchest 显著领先**

orchest 在工具安全、MCP、异步工具、agent-as-tool 方面全面领先。pi-agent-core 的优势在 hook 系统和并行执行。

**新增能力 (最近更新)**:
- `AgentTool`: 原生 agent-as-tool 抽象, 通过 `ToolOutput::AgentDelegate` 返回值触发子 agent 执行, 避免 JSON 协议开销
- `execute_agent_delegate()`: 专用执行路径, 复用子 agent 事件循环和预算传播
- Python SDK `register_agent_tool()`: 一行代码注册子 agent 为工具
- `WriteFileTool`: 内置文件写入工具, 支持 `requires_approval` 配置

**建议**: 
1. 增加 `beforeToolCall` / `afterToolCall` hook 点。在 `run_loop` 的工具执行段落前后增加两个可选回调
2. 增加并行工具执行模式: 当多个工具调用出现在同一个 assistant response 中时, 默认顺序执行, 可选并行
3. 增加工具参数 schema 验证: 在执行前用 `jsonschema` crate 验证 input 是否符合 `input_schema`

### 3.5 子 Agent 编排

| 能力 | orchest | pi-agent-core |
|------|---------|---------------|
| 子 Agent 系统 | ✅ 完整 | ❌ 无 |
| JSON 协议路径 | ✅ `execute_sub_agent_request` | ❌ |
| 原生路径 | ✅ `AgentTool` → `execute_agent_delegate` | ❌ |
| 权限继承 | ✅ `narrow_permission_list` 交集收窄 | ❌ |
| 预算传播 | ✅ 实时增量 (`record_external_usage`) | ❌ |
| 深度限制 | ✅ `run_depth >= 3` 阻止 | ❌ |
| 事件包装 | ✅ `ChildRunEvent { child_run_id, run_depth }` | ❌ |
| 审批路由 | ✅ `active_children` → 子 run 的 `ApprovalSlot` | ❌ |
| SDK 集成 | ✅ Python `register_agent_tool()` | ❌ |

**差距等级: 🟢 orchest 核心竞争优势**

这是 orchest 最大的结构性优势。pi-agent-core 是单 agent 循环, 没有子 agent 概念, 也没有 agent-as-tool 模式。

### 3.6 SDK 表面

| 维度 | orchest | pi-agent-core |
|------|---------|---------------|
| Rust | ✅ 原生 | ❌ |
| Python | ✅ pyo3 绑定 (710 行) | ❌ |
| TypeScript/Node | ✅ napi-rs 绑定 (472 行) | ✅ 原生 TS |
| 类型定义 | ✅ `.pyi` stub 308 行 | TS 原生类型 |
| 装饰器注册 | ✅ `@agent.tool` | ❌ |
| Agent-as-tool SDK | ✅ `register_agent_tool()` | ❌ |
| 事件格式 | snake_case JSON dict | camelCase TS 联合类型 |
| 异步支持 | ✅ coroutine 自动检测 | 原生 async/await |

**差距等级: 🟢 orchest 领先**

orchest 的 Python SDK 现在包含完整的 agent-as-tool 注册、write_file 工具注册、事件流消费。deep_research_agent.py 示例展示了实际的多 agent 编排模式。

### 3.7 运行时控制

| 能力 | orchest | pi-agent-core |
|------|---------|---------------|
| 预算限制 | ✅ token/工具调用/时长/费用 四维 | ❌ |
| 取消/中止 | ❌ (需 drop handle) | ✅ `AbortController` |
| 中途注入消息 | ❌ | ✅ steering queue |
| 后续追加消息 | ❌ | ✅ follow-up queue |
| 动态切换模型 | ❌ | ✅ `prepareNextTurn` |
| 动态切换思考级别 | ❌ | ✅ `ThinkingLevel` 6 级 |
| 轮次后停止条件 | ❌ | ✅ `shouldStopAfterTurn` |
| Webhook | ✅ 异步工具 webhook 回调 | ❌ |

**差距等级: 🟡 各有优劣**

orchest 的预算和 webhook 机制更适合后台自主 agent。pi-agent-core 的 steering/follow-up queue 和 abort 机制更适合交互式 agent。

**建议**: 
1. **P0**: 增加 `AbortController` 等价物 — 在 `RunHandle` 上增加 `abort()` 方法, 通过 `CancellationToken` 传入 `run_loop`
2. **P1**: 增加 steering 机制 — 在 `RunHandle` 上增加 `inject_message()` 方法, 通过 `mpsc` channel 将消息注入到下一个 LLM 调用前

### 3.8 事件系统

| 维度 | orchest | pi-agent-core |
|------|---------|---------------|
| 事件类型 | 26 种 `RuntimeEvent` 变体 | 11 种 `AgentEvent` 变体 |
| 子 agent 事件 | ✅ `ChildRunEvent` 递归包装 | ❌ |
| 流式 chunk | ✅ `ModelStreamChunk` (text/thinking/tool) | ✅ `message_update` |
| 工具更新 | ✅ `ToolCallUpdate` | ✅ `tool_execution_update` |
| 审批事件 | ✅ Requested/Granted/Denied | ❌ |
| 预算事件 | ✅ `BudgetWarning` | ❌ |
| 技能事件 | ✅ `SkillContentRead` / `MissingCapabilities` | ❌ |
| 序列化 | serde JSON (snake_case) | TS 联合类型 |

**差距等级: 🟢 orchest 领先**

orchest 的事件系统更丰富, 覆盖审批、预算、技能、子 agent 等维度。pi-agent-core 的事件更精简, 专注于 UI 消费。

### 3.9 测试基础设施

| 维度 | orchest | pi-agent-core |
|------|---------|---------------|
| 总测试数 | 100 | ~3 个测试文件 |
| 确定性 mock | `MockModelProvider` 固定响应 | Faux provider (脚本化序列) |
| 集成测试 | `e2e_validation.rs` 12 个 | `e2e.test.ts` |
| 单元测试覆盖 | registry, builtin, mcp, budget, run | agent-loop, agent |
| CI | GitHub Actions (fmt/clippy/test/ts-naming) | 外部 CI |

**差距等级: 🟡 量上领先, 质需提升**

orchest 的测试数量更多, 但 `MockModelProvider` 只支持固定单响应。pi-agent-core 的 faux provider 支持脚本化响应序列, 可以测试多轮对话中的复杂行为。

**建议**: 扩展 `MockModelProvider` 支持响应序列:
```rust
struct SequentialMockProvider {
    responses: Mutex<VecDeque<ModelResponse>>,
}
```

## 4. 优先级建议

### P0 — 架构级差距 (阻碍产品化)

| 项目 | 预估工作量 | 依赖 |
|------|-----------|------|
| 会话持久化 (SessionStore trait + JSONL 实现) | 5-8 天 | 无 |
| 运行取消 (AbortController 等价物) | 1-2 天 | 无 |
| Google Gemini provider | 2-3 天 | 无 |
| AWS Bedrock provider | 2-3 天 | 无 |

### P1 — 功能性差距 (提升竞争力)

| 项目 | 预估工作量 | 依赖 |
|------|-----------|------|
| run.rs 拆分 (3802→多模块) | 2-3 天 | 无 |
| beforeToolCall / afterToolCall hook | 1-2 天 | 无 |
| steering message 注入 | 1-2 天 | 无 |
| 并行工具执行模式 | 1-2 天 | 无 |
| 工具参数 schema 验证 | 1 天 | 无 |
| Azure OpenAI provider | 2-3 天 | 无 |

### P2 — 锦上添花

| 项目 | 预估工作量 | 依赖 |
|------|-----------|------|
| 分支级压缩 | 3-5 天 | 会话持久化 |
| SequentialMockProvider | 1 天 | 无 |
| 动态模型切换 | 1-2 天 | 无 |
| ThinkingLevel 支持 | 1-2 天 | 无 |
| Mistral / xAI / Groq provider | 各 1-2 天 | 无 |

## 5. orchest 独有优势 (无需追赶)

以下能力是 pi-agent-core 完全没有的, 也是 orchest 的核心差异化:

1. **多 Agent 编排**: `AgentTool` + `execute_agent_delegate` + `ChildRunEvent` — 完整的子 agent 生命周期管理
2. **工具权限边界**: `allowed_tools` / `allowed_skills` 双层过滤 + 执行时拦截
3. **审批系统**: oneshot-per-request 路由, 支持跨 agent 审批
4. **MCP 协议**: stdio + HTTP 双传输, retry 安全 (tools/call 不重试)
5. **异步工具**: poll + webhook 双模式, 支持长时间运行的工具
6. **预算控制**: 四维限制 (token/工具调用/时长/费用), 实时增量传播
7. **进程安全**: `kill_on_drop`, 超时杀子进程, MCP stdio 进程回收
8. **多语言 SDK**: Rust + Python + Node 三语言, Python 有 `@agent.tool` 装饰器
9. **Agent-as-tool SDK**: `register_agent_tool()` 一行代码注册
10. **代码执行**: 内置 JS 代码执行沙箱

## 6. 总结

orchest 在安全性 (权限、审批、预算)、多 agent 编排、MCP 协议、多语言 SDK 方面显著领先。最大的结构性差距在于**会话持久化**和**提供商覆盖**: 前者影响产品可用性 (长任务、断点续跑), 后者影响市场覆盖 (企业客户需要 Bedrock/Azure)。

最近新增的 `AgentTool` / `execute_agent_delegate` 路径进一步巩固了 orchest 在多 agent 编排方面的优势, 使得 Python SDK 可以通过简洁的 `register_agent_tool()` API 实现子 agent 注册, 这是 pi-agent-core 完全不具备的能力。

建议下一阶段聚焦: **会话持久化 → 运行取消 → Provider 扩展 → run.rs 拆分**。
