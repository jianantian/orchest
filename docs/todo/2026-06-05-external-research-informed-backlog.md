---
name: project-external-research-informed-backlog
description: 基于外部研究综合分析（agents-best-practices、pi-subagents、harness、meta-engineering-harness）提炼的下一步迭代建议，按杠杆率排序
metadata:
  node_type: memory
  type: project
  originSessionId: orchest-external-research-review
---

> 状态: **高/中杠杆项与 L1 全部完成** —— H1/H2/M3 由 v0.9.4、H3 由 v0.9.5、M1/M2/L1 由 v0.9.7 落地;
> L2/L4 按本文结论不进 core(v0.9.9 提供了参考示例);L3 仍推迟、未排期 | 记录于 2026-06-05,
> 状态核对于 2026-10-02(逐项对照代码与 roadmap「已完成」表;各项下的「现状」是 2026-06 的原始分析)

基于 `docs/research/external-research-synthesis.md` 的五份外部研究，与 Orchest 现状做交叉分析后，提炼以下迭代建议。

分三档：**高杠杆（改动小、收益大）**、**中等杠杆（需要设计但方向明确）**、**低优先级（有价值但可推迟）**。每项标注来源研究和涉及的 Orchest 模块。

---

## 高杠杆

### H1. 工具重试策略：消费已有的 RetryHint

**状态**: ✅ 已完成(v0.9.4 Runtime Failure Semantics)—— tool dispatch 按 `RetryHint` 决定重试(`Safe` + `Transient` 自动重试、`Caution` 走审批、`Unsafe` 不重试),并发出 `ToolCallRetry` 事件;结构化错误完整返回模型。

**来源**: agents-best-practices (重试策略章节)
**现状**: `ToolError` 已有 `RetryHint::Safe / Caution / Unsafe`，但 tool dispatch 层没有消费它——工具失败一律直接返回模型。
**建议**:
- `RetryHint::Safe` + 瞬时错误（Transient ErrorKind）→ 自动重试，指数退避，上限 3 次
- `RetryHint::Unsafe` → 绝不自动重试，结构化错误直接返回模型
- `RetryHint::Caution` → 走 ApprovalBus 询问（复用现有审批流）
- 重试预算从当前 run 的 BudgetGuard 扣减

**额外注意**: 当前 actor.rs 在 tool 失败时只提取 `error.message` 返回模型，`kind`/`retry`/`next_step` 字段丢失。实现重试的同时，应将结构化错误的完整信息（至少 `kind` + `next_step`）传给模型，否则模型无法区分 Ambiguity 和 Transient。
**涉及模块**: `agent-runtime-core` 的 tool dispatch 路径（actor.rs 或 tool execution 层）
**预估范围**: 小，主要是在 dispatch 层加一个重试判断分支 + 错误返回格式调整

### H2. ToolError 增加 Ambiguity 和 SpecGap 分类

**状态**: ✅ 已完成(v0.9.4)—— `ErrorKind::Ambiguity` / `ErrorKind::SpecGap`。

**来源**: meta-engineering-harness (四路失败仲裁器)
**现状**: `ErrorKind` 有 `InvalidInput / NotSupported / Transient / Fatal`，覆盖了 Bug 和 Noise，但缺少 Ambiguity（规格允许多种行为，重试无意义）和 SpecGap（规格本身缺失，需要上报）。
**建议**:
- `ErrorKind` 新增 `Ambiguity` 和 `SpecGap` 两个变体
- `Ambiguity` 的 `next_step` 默认为 "ask_user" 或 "clarify"——提示模型不要盲目重试，而是向用户要求澄清
- `SpecGap` 的 `next_step` 默认为 "escalate"——提示模型上报父 agent 或用户
- 这两个分类不影响现有逻辑——只是给模型更精确的失败语义

**涉及模块**: `agent-runtime-core` 的 error types
**预估范围**: 极小，只是枚举扩展 + 文档

### H3. Sub-agent 上下文模式显式化：fresh vs fork

**状态**: ✅ 已完成(v0.9.5 Agent Control-Flow Hardening)—— `ContextMode::Fresh` / `Fork`。

**来源**: pi-subagents (上下文模式章节)
**现状**: `SubAgentBuilder` 通过 `inherit_context_count` 控制继承消息数量——0 等于 fresh，>0 等于部分 fork。语义隐含在数字里。
**建议**:
- 引入显式枚举 `ContextMode::Fresh | Fork { depth: usize }` 替代裸数字
- `Fresh` → 干净会话，不带父历史。适用于需要无偏见审查的 reviewer agent
- `Fork { depth }` → 从父继承最近 N 条消息。适用于需要延续上下文的 worker agent
- Fork 失败（无可继承消息）时明确报错，不静默降级为 Fresh（pi-subagents 的设计决策）
- 向后兼容：`inherit_context_count` 可保留为 `ContextMode` 的便捷构造器

**涉及模块**: `agent-runtime-core` 的 sub-agent 配置（agent_as_tool.rs）
**预估范围**: 小，类型层面的改进

---

## 中等杠杆

### M1. Draft/Commit 工具分离模式

**状态**: ✅ 已完成(v0.9.7 Tool Surface Extensions)—— `ToolExecutionMode::Draft { commit_tool }` / `Commit { draft_tool }`。

**来源**: agents-best-practices (Draft/Commit 分离章节)
**现状**: Approval 枚举有 `Never / WhenRisky / Always`，是对单个工具调用的审批。没有原生支持"预览 → 确认执行"的两步模式。
**建议**:
- 在 Tool metadata 中增加 `draft_mode: bool` 或 `commit_tool: Option<String>` 字段
- 当 `draft_mode = true` 时，工具执行返回预览结果但不产生副作用，模型需要显式调用对应的 commit 工具才真正执行
- Runtime 层面：draft 工具自动获得 `Approval::Never`（可自由调用），commit 工具自动获得 `Approval::Always`（必须审批）
- 这不是强制所有工具都拆分——只是为高风险工具提供一个标准模式

**涉及模块**: Tool trait / ToolMetadata / tool dispatch
**预估范围**: 中等，需要设计 metadata 扩展和 dispatch 逻辑

### M2. 工具动态发现（Deferred Tools / search_tools）

**状态**: ✅ 已完成(v0.9.7)—— `SearchToolsTool` + `AgentConfigBuilder::enable_tool_search()`。

**来源**: agents-best-practices (工具可见性分层第 5 层)
**现状**: 所有工具启动时注册，运行中不变。对小型 agent 够用，但 Skill 库增长后，启动时全部注册会浪费 token（所有 tool schema 进 system prompt）。
**建议**:
- 注册一个内建元工具 `search_tools(query: String) -> Vec<ToolDef>`
- 模型在需要时调用 `search_tools`，返回匹配的工具定义
- 匹配到的工具动态注入当前 run 的 ToolRegistry（不影响其他 run）
- 与现有 Skill 的 progressive disclosure 互补：Skill 控制知识的渐进披露，`search_tools` 控制工具的渐进发现

**涉及模块**: ToolRegistry, actor.rs（run 级别的动态注册）
**预估范围**: 中等，需要考虑 schema 注入时机和 tool 生命周期

### M3. 失败模式 Hook 点（on_repeated_failure）

**状态**: ✅ 已完成(v0.9.4)—— 重复失败 hook(`RepeatedFailureHookContext`、`repeated_failure_threshold`)。

**来源**: agents-best-practices (反馈循环 + 熵管理), meta-engineering-harness (校准循环)
**现状**: Hook 框架有 9 个生命周期点，但没有"模式检测"类 hook——重复失败只是一次次返回模型。
**建议**:
- 新增 `on_repeated_failure(tool_name, error_history, count)` hook 点
- 当同一工具 + 同类 ErrorKind 连续失败 N 次（可配置）时触发（不同 ErrorKind 的失败不算"重复"——先 InvalidInput 再 Transient 不应触发）
- Hook 实现可以：切换策略、降级到备选工具、上报用户、中止 run
- 这是 SDK 层面支持上层应用实现"校准循环"的最小接口

**涉及模块**: Hook trait, actor.rs（失败计数逻辑）
**预估范围**: 中等

---

## 低优先级（有价值但可推迟）

### L1. 并行工具执行

**状态**: ✅ 已完成(v0.9.7)—— 可选的并行 tool execution(`ToolExecutionPolicy`、`enable_parallel_tools()`),默认仍串行。

**来源**: harness (Fan-out/Fan-in 模式), pi-subagents (并行委托)
**现状**: 单线程 per run，工具串行执行。模型可能在一次回复中请求多个独立工具调用，但 runtime 依次执行。
**Why 推迟**: 并行执行引入并发复杂性（共享状态、错误聚合、budget 竞争），且大多数 provider 的 tool_use 响应是串行的。等实际瓶颈出现再做。
**备忘**: 如果实现，可参考 pi-subagents 的 worktree 模式——每个并行写入任务用独立 git worktree 避免文件冲突。

### L2. 完整权限策略引擎

**状态**: 按下文结论不进 core。v0.9.9 提供了应用层参考示例 `examples/rust/guardrails/authority_policy.rs`。

**来源**: agents-best-practices (8 级权威层级, 14 级风险分类, 7 种决策结果)
**现状**: Guardrail trait + Approval enum，简洁但表达力有限。
**Why 推迟**: Orchest 是最小核心 SDK。完整策略引擎是应用层关注点——上层可以通过 Guardrail trait 实现任意复杂的策略，不需要 runtime 内建。保持当前设计。

### L3. Agent 间直接通信

**状态**: 未做,仍推迟、未排期(尚无需要 peer-to-peer 通信的产品场景)。

**来源**: harness (Agent Teams 模式, SendMessage/TaskCreate)
**现状**: Sub-agent 只和父通信，互不感知。
**Why 推迟**: 直接通信适合紧耦合团队场景，但增加系统复杂度。Orchest 的 agent-as-tool + hooks 原语已足够组合出大部分模式。等出现明确需要 peer-to-peer 通信的产品场景再设计。

### L4. 预设团队架构模板

**状态**: 按下文结论不进 core。v0.9.9 以 `examples/` 提供了 guardrail / team pattern 参考示例。

**来源**: harness (6 种团队架构模式)
**现状**: 无预设模板，用户用原语组合。
**Why 推迟**: 模板是上层 SDK 或产品的事，不是 runtime 核心的事。可以作为 examples/ 或 skills/ 提供参考实现，但不进 core crate。

---

## 不做的事（有意识的选择）

| 外部建议 | 不做的原因 |
|---------|-----------|
| Two-Pass 合约编译 | 应用层关注点，不是 runtime SDK 的职责 |
| Retro agent / 自动校准 | SDK 提供 hook 点即可（见 M3），具体校准逻辑是产品层的事 |
| 8 级权威层级 | 过重，Guardrail trait 已提供足够的扩展点 |
| 可视化工作空间 (Nimbalyst) | 与 SDK 层无交集 |

---

## 建议实施顺序

> 以下六项均已落地(见各项「状态」),顺序保留作记录。

1. **H2** (ErrorKind 扩展) → 最小改动，立即可做
2. **H1** (RetryHint 消费) → 依赖 H2 的错误分类，紧接着做
3. **H3** (ContextMode 枚举) → 独立改动，可与 H1/H2 并行
4. **M1** (Draft/Commit) → 需要设计讨论，排在 H 系列之后
5. **M2** (search_tools) → 等 Skill 库规模增长后再做
6. **M3** (on_repeated_failure hook) → 等有实际校准需求时做

Related: [[project-supervised-delegation]] — H3（ContextMode）和 M3（重复失败 hook）与 Supervised Delegation 场景直接相关
