# Agents Best Practices — Provider-Neutral Agent Harness Design

**仓库**: https://github.com/DenisSergeevitch/agents-best-practices (v1.2.0)
**一句话描述**: 一个 provider-neutral 的 Agent Skill，用于设计、生成 MVP 蓝图、审计、重构和解释任何领域下的 agentic harness 架构。

## 概述

这是一个可安装的 Agent Skill（SKILL.md + references/），不是应用或框架。它面向所有使用 OpenAI、Anthropic 或 OpenAI-compatible API 构建 agent 的开发者，覆盖范围超出编码 agent：研究、金融、法律、支持、运营、销售、数据分析、采购、医疗、教育工作流自动化 agent 都需要相同的核心运行时规范。

**核心立场**：agent harness 是模型周围的控制面。模型提出动作；harness 验证、授权、执行、记录、摘要并返回观察。保持循环简单，让运行时严格。

默认架构：

```
user/task
  → instruction and context builder
  → model call
  → tool/action proposal
  → schema validation
  → permission decision
  → execution or approval pause
  → structured observation
  → context update
  → repeat within budget or finish
```

## 核心概念

### Harness 定义与边界原则

Harness 是一个 provider-neutral 运行时，让模型安全、可重复地行动。它不是模型，也不只是 prompt。它是拥有模型调用、工具路由、权限、内存、上下文压缩、审批、追踪和恢复的控制面。

**模型职责**：
- 解释用户意图
- 选择下一步推理/动作
- 使用结构化调用请求工具
- 综合观察结果
- 生成最终答案或计划

**Harness 职责**：
- 组装指令和上下文
- 决定哪些工具可见
- 验证工具参数
- 执行权限和审批
- 执行工具或调用外部系统
- 存储状态、工件和追踪
- 压缩和恢复上下文
- 执行预算和停止条件

**边界原则**：将受信任的控制面保持在模型导向的计算之外。不要在模型 prompt 或模型可修改的沙箱中放置 secrets、审批逻辑或授权决策。

### 组件模型

一个完整的 harness 包含 16 个组件：

1. Instruction manager
2. Context builder
3. Model adapter
4. Tool registry
5. Permission engine
6. Execution engine
7. State store
8. Memory and retrieval layer
9. Compactor
10. Planner and goal controller
11. Workflow scheduler
12. Skill registry
13. MCP/external connector manager
14. Approval manager
15. Trace and evaluation system
16. Sandbox or execution boundary

### 权限层级

```
provider/system policy
  → organization policy
  → product/developer policy
  → workspace/project policy
  → domain or directory policy
  → user task
  → model-visible runtime reminders
  → tool observations
  → untrusted retrieved content
```

Harness 应按权限级别标记内容。检索到的内容可能包含指令，但这些指令是数据，不是策略。

### Agentic Loop（标准循环）

provider-neutral 循环：

```
while not done:
  build context
  call model with visible tools
  receive final answer or tool requests
  validate every tool request
  check permission and approval policy
  execute or deny each tool request
  append structured tool results
  compact or retrieve context if needed
  stop on completion or budget
```

**循环不变量**（必须在代码中执行）：

1. 每个工具调用恰好收到一个对应的结果
2. 工具参数在解析并在执行前验证
3. 每个副作用之前都有权限决策
4. 工具结果有界、结构化、可追踪
5. 循环有硬性的步数、时间、token、成本和工具调用预算
6. 最终答案基于观察，不是假设工具成功
7. 错误、拒绝、取消和超时都成为结构化观察

### 工具设计原则

工具是模型和 harness 之间的合约。模型看到合约，harness 拥有执行权。

每个工具应定义：name、purpose、input schema、output schema、risk class、side-effect class、resource scope、permission policy、timeout、result-size limit、retry policy、audit policy、error format。

**避免宽泛工具**。偏好具有领域语义的窄工具：

```
Bad:  execute_anything(command), call_api(url, method, body), update_database(sql)
Good: search_policy_docs(query, max_results), read_customer_account(account_id),
      draft_customer_email(case_id, tone), request_refund_approval(order_id, amount, reason)
```

### 风险分类

```
read_only / search_only / compute_only / draft_only
write_local / write_internal / write_external
financial / communication / identity_access / security_sensitive
process_execution / network_open_world / destructive / privileged_admin
```

### 权限决策

权限引擎返回以下之一：

```
allow / deny / ask_user / approval_required
require_stronger_auth / run_in_sandbox / run_as_draft_only
```

### Draft vs Commit 分离

将高风险动作拆分为独立工具：

```
draft_email → send_email
prepare_refund → issue_refund
propose_record_update → apply_record_update
prepare_contract_change → submit_contract_change
```

Draft 工具通常可自动运行。Commit 工具需要审批，除非低风险且显式允许列表。

### 工具结果格式

结构化返回（而非原始文本）：

```json
{
  "status": "success",
  "summary": "Found 3 matching cases.",
  "items": [...],
  "next_valid_actions": ["read_case", "draft_response"]
}
```

错误：

```json
{
  "status": "error",
  "type": "permission_denied",
  "message": "Sending external email requires approval.",
  "next_valid_actions": ["draft_email", "request_approval"]
}
```

### 上下文与内存

持久化状态**不在 prompt 中**存储，包括：当前计划、目标、todo 列表、审批记录、工作流计划、工具追踪、工件、检索资源引用、技能调用、压缩摘要、评估结果、连接器凭证。

然后在下一个模型调用中只重新附加相关部分。

### 自动压缩

在模型调用前需要压缩的时机：token 预算即将耗尽、上下文不相关、内存膨胀。压缩应保留工作状态和决策记录，而非对话散文。

### 规划模式（Planning Mode）

规划是循环的可选模式。进入规划模式时，模型产生步骤序列。Harness 在执行前必须审查并批准计划。

### Goal-like Loop（目标式循环）

目标式循环是标准循环的长期运行版本，需要额外的持久化状态：

```
objective / done condition / budget / checkpoints
current plan / progress log / validation method / stop rules
```

循环定期检查：
1. 目标是否仍然有效？
2. 什么证据证明进展？
3. 预算内吗？
4. 完成条件是否满足？
5. 下一步需要人类审批吗？
6. 现在需要压缩或交接吗？

**目标循环不应**用于模糊的积压或不相关的任务。

### 事件模型

将 agent 状态存储为类型化事件而非仅聊天消息：

```
user_message / assistant_message / tool_call / tool_result
approval_request / approval_result / plan_update / goal_update
skill_invocation / memory_load / context_compaction / connector_call
workflow_plan / workflow_packet_started / workflow_packet_result
workflow_verification_result / workflow_integration_result / error / final_answer
```

类型化事件改善重放、审计、压缩、评估和调试。

### Harness 成熟度等级

| Level | 描述 |
|-------|------|
| **0: Answer-only** | 无工具执行，仅问答和摘要 |
| **1: Retrieval** | 可搜索和读取可信资源，无副作用 |
| **2: Drafting** | 可建议动作、草稿消息或计划，不可提交更改 |
| **3: Approval-gated** | 准备并执行动作，需显式用户或策略批准 |
| **4: Policy-bounded autonomous** | 在严格范围、预算和审计控制下自主执行低风险动作 |
| **5: Long-running goal worker** | 跨多轮或会话持续执行可度量目标 |

逐级上升，仅在评估显示更简单的级别不够用时才升级。

### 工作流编排

可选层，用于大型可分解任务：

```
objective
  → workflow plan
  → permission and budget check
  → work packets
  → worker contexts
  → verifier contexts
  → integration
  → final result with evidence
```

仅在单 worker 循环可测量不足时使用。

### 技能（Skills）与连接器（Connectors）

- **Skill**：过程性知识 + 工具包，通过 progressive disclosure 加载
- **MCP**：外部工具的传输协议
- 原则：不要一次性暴露所有能力；使用工具搜索让模型发现需要的工具

### Prompt 缓存

核心实践：在可缓存 prompt 开头保持稳定内容（系统指令、策略文件）；在末尾放置动态内容（历史、最新用户消息）。使用确定性序列化格式。

### 可观察性与评估

trace 事件、指标、评估用例和启动门应在 harness 中内置。不应在部署后添加。

## 非协商原则（Non-negotiable Principles）

1. 模型不直接执行动作；harness 执行
2. 每个工具调用必须收到工具结果，即使结果是拒绝、超时、错误或中止
3. 每个有风险的副作用都需要模型外的运行时策略执行
4. 外部、金融、破坏性、安全或受监管动作的 draft 和 commit 应分开
5. 工具 schema 必须窄、类型化、本地验证、可审计
6. 上下文应信息充分、紧凑且缓存感知：在需要时检索和附加
7. 技能和外部连接器应使用 progressive disclosure
8. 自动压缩应保留工作状态而非对话散文
9. 长期目标需要预算、检查点和可度量的完成条件
10. 工作流编排需要持久的 packet 状态、独立验证、集成规则和总预算执行
11. Harness 必须追踪操作事件而不暴露隐藏推理
12. 持久知识应存放在 agent 可读取的真实来源工件中
13. 重复失败应成为工具、验证器、文档、评估或策略

## MVP 蓝图

当用户要求构建一个 domain agent 时，默认使用 MVP Builder 模式——生成具体的领域 MVP harness 蓝图，而非仅提供建议。蓝图包含：

- 目标和 MVP 范围
- 自主性和风险级别
- 核心循环
- 指令架构（system/developer/user 层级）
- 工具注册表（schemas、风险类、权限）
- 规划和目标行为
- 上下文和内存（检索、持久化状态、压缩、恢复）
- 技能和连接器（策略、发现、工具搜索）
- 安全与审批（guardrails、injection 处理、沙箱）
- 可观察性与评估（trace 事件、评估用例、启动门）
- 最小实现路径

## 局限性

1. **纯知识文档** — 不是可运行的框架或库，只是指导和参考
2. **Provider-neutral** — 不做 provider 特定优化，需要用户自己实现 adapter 层
3. **无代码示例** — 提供架构指南和伪代码，但没有生产级实现
4. **无沙箱实现** — 讨论沙箱抽象但不提供具体沙箱
5. **渐进式披露的局限性** — 15 个独立 reference 文件，用户需自行选择哪些相关
6. **无多 provider 自动 fallback** — 建议了架构但未处理实现
7. **假定人工判断** — 审批和评估架构假设有人类在循环中
8. **无性能数据** — 未提供延迟、吞吐量或成本基准
