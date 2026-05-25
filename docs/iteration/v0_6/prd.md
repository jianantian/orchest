# v0.6 PRD：架构健壮性改造

## 背景

v0.1–v0.5 完成了 runtime 核心、多 provider、MCP 集成、sub-agent、context compaction 等功能。
v0.6 是专项架构债务清偿迭代，不新增对外可见特性，目标是让代码库在后续功能扩展时不再因结构性问题积累压力。

审查发现的主要问题：

1. `run.rs` 4040 行单文件，10 个独立职责混杂，无法并行维护
2. `BudgetGuard` 硬编码 Anthropic 定价，OpenAI / DeepSeek 成本计算完全错误
3. Approval 路由依赖父子手动映射，多层 sub-agent 下有竞争窗口；且存在 `__sub_agent_request` 魔法字段和 `ToolOutput::AgentDelegate` 两条并行路径
4. `AgentConfig` 14 个扁平字段无分组，随功能增长失控
5. MCP stdio 全局锁导致同一 server 的工具调用串行，v0.2 并行 tool call 上不去
6. `truncate_output` 用 `tokens × 4` 估算字节，对 CJK 等多字节字符严重低估
7. 每个 provider adapter 独立创建 `reqwest::Client`，连接池碎片化
8. Python SDK 缺少同步入口，Jupyter / 脚本场景不友好

## 目标

v0.6 结束时：

1. `crates/agent-runtime-core/src/run/` 目录取代 `run.rs`，每个文件单一职责，最大文件不超过 700 行
2. `BudgetGuard` 不含任何 provider 定价常量；成本由 adapter 通过 `TokenUsage.cost_usd` 上报
3. 所有 sub-agent 路径统一走 `ToolOutput::AgentDelegate`；`ApprovalBus` 替代 `active_children` HashMap
4. `AgentConfig` 按 model / budget / skills / runtime 四组嵌套，提供 builder API
5. `McpStdioClient` 支持同一 server 并发调用，无全局 mutex
6. `truncate_output` 改用 tiktoken-rs 做真实 token 计数
7. 所有 provider adapter 共用一个全局 `reqwest::Client`
8. Python SDK 提供 `Agent.run_sync()` 同步方法，PyO3 层保留结构化错误 code 字段

## 成功指标

- `cargo test --workspace` 全绿
- `cargo clippy --workspace -- -D warnings` 全绿
- `run/` 目录下每个文件不超过 700 行
- `budget.rs` 中不含 `*_PER_MILLION` 常量
- `run.rs` / `run/` 中不含 `__sub_agent_request` 字符串
- `McpStdioClient` 中不含 `Mutex<McpStdioInner>`（整体大锁）
- Python `agent_runtime.Agent` 暴露 `run_sync()` 方法

## 范围

### 在范围内

- `run.rs` → `run/` 目录模块化拆分
- `AgentConfig` 分组重构 + builder
- `BudgetGuard` 定价解耦（`ModelPricing` 类型 + `TokenUsage.cost_usd` 字段）
- `ApprovalBus` + `AgentDelegate` 路径统一（消除 `__sub_agent_request`）
- `McpStdioClient` 并发化（reader task + ID dispatch）
- tiktoken-rs 集成（`count_tokens` / `truncate_to_tokens`）
- `reqwest::Client` 全局共享
- Python SDK `run_sync()` + 结构化异常类型

### 不在范围内

- 新的 agent 功能（新 tool 类型、新 provider 等）
- Provider 注册表模式（允许运行时注入自定义 provider）—— 留 v0.7
- Tool 中间件 / 拦截器框架 —— 留 v0.7
- Skill 环境并行预热 —— 留 v0.7
- 补充 examples —— 留 v0.7（依赖本迭代 API 稳定后再写）
- crates.io 发包

## Issues 拆解

| Issue | 标题 | 优先级 |
|-------|------|--------|
| [001](./issues/001-run-module-split/spec.md) | run.rs 模块化拆分 | P0 |
| [002](./issues/002-agent-config-refactor/spec.md) | AgentConfig 分组重构 + Builder | P0 |
| [003](./issues/003-budget-pricing/spec.md) | BudgetGuard 定价解耦 | P0 |
| [004](./issues/004-approval-bus/spec.md) | ApprovalBus + AgentDelegate 路径统一 | P1 |
| [005](./issues/005-mcp-concurrent/spec.md) | McpStdioClient 并发化 | P1 |
| [006](./issues/006-tiktoken/spec.md) | tiktoken-rs 集成 | P2 |
| [007](./issues/007-http-client/spec.md) | reqwest::Client 全局共享 | P2 |
| [008](./issues/008-python-sdk/spec.md) | Python SDK 易用性改进 | P2 |

## 推荐执行节奏

001 和 002 必须优先完成，因为几乎所有后续 issue 都修改 `run/` 目录下的文件或 `AgentConfig` 的字段。

003 可以和 001/002 并行（只改 `budget.rs` + providers types）。

004 依赖 001 完成（需要 `run/handle.rs` 文件存在）。

005、006、007 互相独立，可并行推进。

008 依赖所有 Rust 改动稳定后推进（不想在 FFI 层改两次）。

### 依赖图

```
001 ──┬──> 004
      └──> (所有改 run/ 的 issue)

002 ──> 004

003     (独立)

005     (独立)

006     (独立)

007     (独立)

008 ──> (001~007 全部完成)
```

## v0.6 权威顺序

1. `docs/iteration/v0_6/issues/*/spec.md` 是实施与验收的第一权威
2. `docs/iteration/v0_6/prd.md` 约束范围和优先级；与 issue 细节冲突时，以 issue spec 为准
3. 上方架构评审讨论是背景参考，不作为验收依据
