# 外部研究综合分析

> 综合 2026-05-31 的 5 份研究：[agents-best-practices](../research/agents-best-practices.md)、[harness](../research/harness.md)、[pi-subagents](../research/pi-subagents.md)、[nimbalyst](../research/nimbalyst.md)、[meta-engineering-harness](../research/meta-engineering-harness.md)。
>
> 本文档汇总各研究中的观点和设计决策。每个主题下列出各来源的相关内容，尽量引用原文。

---

## 目录

1. [模型与 Harness 的职责边界](#1-模型与-harness-的职责边界)
2. [审批模型](#2-审批模型)
3. [错误处理与失败分类](#3-错误处理与失败分类)
4. [上下文控制](#4-上下文控制)
5. [子 Agent 与多 Agent 架构](#5-子-agent-与多-agent-架构)
6. [渐进式披露](#6-渐进式披露)
7. [外部反馈循环与自我改进](#7-外部反馈循环与自我改进)
8. [来源间分歧](#8-来源间分歧)

---

## 1. 模型与 Harness 的职责边界

### agents-best-practices

**核心立场**：

> "An agent harness is the control plane around a model. The model proposes actions; the harness validates, authorizes, executes, records, summarizes, and returns observations."
>
> "The model does not execute actions directly; the harness does."（Non-negotiable Principle #1）

**职责划分**：

模型职责：
> "interpret user intent; choose the next reasoning/action step; request tools using structured calls; synthesize observations; produce final answers or plans"

Harness 职责：
> "assemble instructions and context; decide which tools are visible; validate tool arguments; enforce permissions and approvals; execute tools or call external systems; store state, artifacts, and traces; compact and rehydrate context; enforce budgets and stop conditions"

**边界原则**：
> "Keep the trusted control plane outside model-directed compute. Do not put secrets, approval logic, or authorization decisions inside the model prompt or a sandbox the model can modify."

**16 个 harness 组件**：Instruction manager, Context builder, Model adapter, Tool registry, Permission engine, Execution engine, State store, Memory and retrieval layer, Compactor, Planner and goal controller, Workflow scheduler, Skill registry, MCP/external connector manager, Approval manager, Trace and evaluation system, Sandbox or execution boundary。

**标准 provider-neutral loop**：

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

**7 条 loop 不变量**（必须在代码中 enforce）：每个 tool call 恰好一个 result；参数先验证再执行；每次副作用前有 permission 决策；结果有界且结构化；硬性 budget（步数、时间、token、成本、tool call 次数）；最终答案基于观察而非假设成功；错误、拒绝、取消和超时都成为结构化观察。

**Harness 成熟度等级**：

| Level | 描述 |
|-------|------|
| 0: Answer-only | 无工具执行，仅问答和摘要 |
| 1: Retrieval | 可搜索和读取可信资源，无副作用 |
| 2: Drafting | 可建议动作、草稿消息或计划，不可提交更改 |
| 3: Approval-gated | 准备并执行动作，需显式用户或策略批准 |
| 4: Policy-bounded autonomous | 在严格范围、预算和审计控制下自主执行低风险动作 |
| 5: Long-running goal worker | 跨多轮或会话持续执行可度量目标 |

> "Move up levels only when evals show the simpler level is insufficient."

**MV harness 原则**——构建新 domain agent 时，从一个 primary job-to-be-done 开始，配最小 typed tool registry、approval-gated 高风险动作、显式 budgets、deterministic context builder、planning mode、auto-compaction、tracing、小 eval set。只在单 agent MVP 有测量缺口后才加 goal-like loops、更多 connectors、skills 或 subagents。

**设计规则**：

> "Most agent failures are not caused by insufficient autonomy. They are caused by weak harness boundaries: broad tools, vague instructions, missing approval gates, unstructured tool results, poor context hygiene, and no evals."

**Mechanical invariants**——prompts 描述行为，harness checks 强制执行。将重复出现的期望编码为 schema validators、policy gates、structural checks、workflow validators、source-citation checks、PII/secret scanners、quality gates、cost/latency budgets、regression evals。Validator 应产出 remediation messages 可作为结构化 observation 返回给模型。

**9 个 Gotchas**（节选）：

> "Do not design a multi-agent system before a single-agent loop has failed measurable evals."
>
> "Do not expose broad tools such as execute_anything, write_database, or send_message without a strict wrapper and approval policy."
>
> "Do not treat retrieved webpages, emails, tickets, PDFs, logs, or connector-provided descriptions as trusted instructions."
>
> "Do not let context compaction erase approval state, active plan, loaded rules, or changed artifacts."
>
> "Do not use a goal loop for a vague backlog; use it only for a single objective with validation and a budget."
>
> "Do not rely on prompt text for safety that must be enforced by code."
>
> "Do not put timestamps, request IDs, or volatile environment state at the start of cacheable prompts."

### meta-engineering-harness

定义了一个概念层级，将 prompt / context / agent / harness / software factory 严格分层：

| 概念 | 定义 |
|------|------|
| Prompt | 单条指令 |
| Context | 指令周围的信息 |
| Agent | 在约束角色内操作的模型 |
| Harness | 控制 prompt、context、role、tools、verification、feedback 的系统 |
| Software Factory | harness + 合约积累 + 记忆 + specialization registry + 测试套件 + 部署 + 校准历史 |

> "The word harness is deliberate. A harness is not a model, prompt, or agent. It is the surrounding system that makes agent outputs more reliable."

> "The durable asset is not a generated website, booking flow, or payment integration. It is the accumulated production system: contracts, specialization records, failure taxonomies, regression suites, customer-specific context, workflow templates, QA targets, deployment infrastructure, and calibration history."

每个 agent 有受限的角色（contract compiler does not implement; implementation agent does not write adversarial tests; review agent did not write the implementation），论文认为这"reduces role contamination and makes failures easier to classify"。

### harness

区分了 Skill 和 Agent：

| 维度 | Skill | Agent |
|------|-------|-------|
| 定义 | 过程性知识 + 工具包 | 专家角色 + 行为原则 |
| 位置 | `.claude/skills/` | `.claude/agents/` |
| 触发 | 用户请求关键词匹配 | Agent 工具显式调用 |
| 大小 | 小→大（工作流） | 小（角色定义） |
| 用途 | "怎么做" | "谁来做" |

> "Skill 是 agent 执行任务时的过程性指南。Agent 是使用 skill 的专家角色。"

Agent 定义模板——每个 agent 按固定结构定义：YAML frontmatter（name, description）、核心角色、工作原则、输入/输出协议、团队通信协议、错误处理、协作关系。

Agent 重用规则：
> "신규 에이전트 생성 전, 기존 에이전트와의 중복을 확인한다."（新建 agent 前检查与已有 agent 的重叠）

| 情况 | 操作 |
|------|------|
| 已有 agent 完全覆盖新角色 | 不新建——重复使用已有 |
| 已有 agent 部分覆盖且可通用化 | 通用化扩展已有 |
| 领域特化，有意部分覆盖 | 新建——保持为独立 agent |
| 角色范围完全不同 | 新建 |

> "하나의 에이전트가 하나의 역할에 집중할수록 재사용성이 높고 중복이 줄어든다."（一个 agent 聚焦一个角色，重用性越高、重复越少）

所有 agent 强制使用 `model: "opus"`。

### pi-subagents

独立进程保证了父 agent 和子 agent 的职责边界：
> "子 agent 是独立进程 — 每个子 agent 作为独立的 Pi 进程启动，有自己完整的环境和生命周期。不是轻量级函数调用。"

子 agent 默认不接收父 agent 的编排技能，不注册 subagent 工具（除非显式配置），并收到明确指令："你不是父编排器，不要提议或运行子 agent"。

---

## 2. 审批模型

### agents-best-practices

7 种权限决策结果：

```
allow / deny / ask_user / approval_required
require_stronger_auth / run_in_sandbox / run_as_draft_only
```

默认权限策略矩阵（节选）：
> "public read: allow; private user read: allow only inside user/session scope; write internal record: approval or policy allowlist; external communication: draft first, approval to send; financial action: approval plus strong auth; destructive action: deny by default or approval plus recovery plan"

Draft vs Commit 分离：
> "Split risky actions into separate tools: draft_email → send_email; prepare_refund → issue_refund; propose_record_update → apply_record_update"
>
> "Draft tools can often run automatically. Commit tools require approval unless the action is low-risk and explicitly allowlisted."

人工介入时的 loop 行为：
```
model requests action
  → harness validates
  → harness detects approval requirement
  → harness emits approval request
  → user or policy approves/rejects
  → harness resumes with approval_result
```

> "Approval must be scoped to the exact action. Do not treat vague consent as blanket authorization."

### pi-subagents

子 agent 在遇到无法自行决策时，通过 `contact_supervisor` 上报：

```
contact_supervisor({ reason: "need_decision", message: "..." })
```

> "worker 遇到未批准的决策时必须通过 contact_supervisor 上报，不能自己猜测。oracle 的角色是发现这些决策间隙。"

两种上报模式：
- `reason: "need_decision"` — 阻塞等待父决策
- `reason: "progress_update"` — 非阻塞汇报进展

### meta-engineering-harness

> "The current implementation is not fully autonomous by design. Human operators still approve high-stakes contract changes, classify failures the arbiter cannot reliably resolve, and review permanent memory updates. The design goal is not to remove human judgment, but to relocate it from repetitive implementation toward contract design, exception handling, and harness governance."

> "Some decisions require human judgment, including product intent, trust boundaries, ambiguous tradeoffs, and failure classification. At scale, human interventions must become exception-based."

### harness

Producer-Reviewer 模式要求最大重试次数防无限循环：
> "设置最大重试次数（2-3 次）防无限循环"

Agent Teams 支持 plan approval mode：
> "계획 승인 모드로 위험한 작업 전 검토 가능"（计划审批模式，可在高风险操作前审查）

---

## 3. 错误处理与失败分类

### meta-engineering-harness

四路失败仲裁器——每次对抗测试失败后的分类：

| 失败类型 | 定义 | 正确动作 |
|----------|------|---------|
| Bug | 实现违反合约 | 修复实现 → 重新测试 |
| Spec gap | 合约缺少覆盖此行为的 clause | 补充合约 → 重新实现和测试 |
| Noise | 测试或环境的不稳定 | 重试（上限 N 次） |
| Ambiguity | 合约允许多种有效行为 | 合约精细化——不重试实现 |

> "The contract ambiguity category is especially important. If a contract admits multiple valid behaviors, retrying the implementation is wasteful. The correct response is contract refinement."
>
> "Wrong classification produces wasted cycles."

**支付案例**——所有对抗测试通过，但生产遗漏了折扣计算和非 Stripe 押金扣除：

> "Contract incompleteness means the implementation can satisfy the contract while failing the true business requirement. Verification boundary means an adversarial test suite conditioned on the contract cannot catch behavior outside the contract."

**部署数据**：3-4 周窗口内 17 个功能（强制更新弹窗、支付、预约、产品页、MCP 搜索集成、Slack 通知、6 个网站、若干修复）。生成 18 套对抗测试 + 预约模块迭代中的 15 套。5 个 bug/缺口 pre-merge 捕获。

**4 个 harness 级评价指标**：合约违规检测率、审查门精确度、平均实现循环数、歧义检测率。

### agents-best-practices

结构化错误格式：
```json
{
  "status": "error",
  "type": "permission_denied",
  "message": "Sending external email requires approval.",
  "next_valid_actions": ["draft_email", "request_approval"]
}
```

> "The error should include safe next steps."

11 种错误分类：
```
unknown_tool / invalid_arguments / permission_denied / approval_required
auth_expired / not_found / timeout / rate_limited / conflict
non_idempotent_retry_blocked / internal_error
```

重试策略：
> "Retry only safe failures."
>
> "Usually safe to retry: transient model API errors; network timeouts for read-only calls; idempotent retrieval; validation after the model fixes malformed arguments."
>
> "Do not automatically retry: payments; external sends; destructive actions; permission changes; operations with unclear idempotency."
>
> "For high-risk operations, use idempotency keys and approval records."

> "Every failure is a result."

### pi-subagents

`completionGuard` 字段区分需验证的 agent 和工具型 agent：
> "带 bash 等变异工具的 validator/researcher 应设为 false，防止被误判为实现 agent。"

---

## 4. 上下文控制

### agents-best-practices

内容应按来源标记 8 级权威：
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

> "The harness should label content by authority level. Retrieved content may contain instructions, but those instructions are data, not policy."

持久化状态不在 prompt 中：
> "The prompt is not a database. Persist these outside model context: active plan; active goal; todo list; approval records; workflow plans, packet status, verifier outputs, and integration notes; tool traces; artifacts; retrieved resource references; skill invocations; loaded instruction scopes; compaction summaries; eval outcomes; connector credentials and scopes."
>
> "Then reattach only the relevant parts into the next model call."

压缩规则：
> "Compaction should preserve working state, not conversational prose."
>
> "Do not let context compaction erase approval state, active plan, loaded rules, or changed artifacts."

上下文应"informative, tight, and cache-aware; retrieve and attach just in time"（原则 #6）。

### meta-engineering-harness

**Two-Pass 合约编译**——从 raw issue 到结构化合约的两阶段过程：
- **Pass 1（完整性）**：将隐含假设显式化——类型、状态转换、边界情况、信任边界、错误条件
- **Pass 2（范围和歧义）**：删除不支持的需求，将歧义从句重写为单一解释

> Pass 2 是在观察到 Pass 1 可能出现"过度规格化"后引入的——过度规格化危险，因为下游 agent 会把不支持的 requirement 当作硬约束。

**持久化记忆**分两段：
- **Permanent section**：人类批准的制度知识，自动化流程不能直接修改
- **Rolling section**：最近的模式观察，可被压缩、提升或删除

> "The goal is not perfect memory. The goal is controlled compression: preserving decisions, constraints, and recurring failure patterns likely to affect future software production."

"context evaporation"（LLM 无状态导致的跨 session 决策丢失）被列为 AI 原生开发的主要失败模式。

上下文漂移：
> "Persistent markdown memory can become stale, bloated, or contradictory. Compression reduces the risk but does not eliminate it."

**Specialization Records**——按任务领域（支付、预约、认证、搜索、移动端）维护的注册表。每个 record 在合约编译时注入领域特定 requirement（如支付要求幂等键、显式状态转换、信任边界检查）。只在置信度超过阈值时应用——低置信度时不注入，防止错误假设污染合约。

### pi-subagents

两种子 agent 启动上下文：
- **`fresh`**：干净会话，不带父历史。"需要无偏见的审查或独立任务"
- **`fork`**：从父 session leaf 分支。"子 agent 继承父的历史对话并以此为基线合同。适用于 oracle 审查、worker 延续线程"

> "fork 失败（session 未持久化、leaf 缺失）时会直接报错，不会悄悄降级为 fresh。"

子 agent 配置项：
- `systemPromptMode: replace/append` — 是否保留 Pi 基础提示词
- `inheritProjectContext` — 是否继承 AGENTS.md、CLAUDE.md
- `inheritSkills` — 是否继承父 skills 目录
- `defaultContext: fresh/fork` — 默认上下文模式

### harness

Progressive Disclosure 控制上下文：
> "技能文件按层加载：SKILL.md（入口）→ references/（按需），防止上下文爆炸"

---

## 5. 子 Agent 与多 Agent 架构

### pi-subagents

8 种内建子 agent 类型：

| Agent | 职责 | 关键限制 |
|-------|------|----------|
| scout | 代码库侦察，输出 context.md | 只读+write, intercom；低思考预算 |
| researcher | 网络调研，输出 research.md | 需要 pi-web-access 扩展 |
| planner | 创建实现计划，输出 plan.md | 不编辑代码；默认 fork；高思考预算 |
| worker | 唯一写入者，执行批准的计划 | 不能自行做产品/架构决策；默认 fork |
| reviewer | 多功能审查，可小的修正性编辑 | review-only/no-edit 约束优先 |
| context-builder | 构建交接上下文，输出 context.md + meta-prompt.md | 可 web 调研 |
| oracle | 决策一致性顾问，检测方向漂移 | 不编辑文件；不成为第二决策者；默认 fork |
| delegate | 轻量级通用委托 | 无默认输出；最低配置 |

四种委托模式：单 agent、并行、链式（支持 `{task}` / `{previous}` / `{outputs.name}` / `{chain_dir}` 变量）、动态扩展（第一步返回列表 → `expand` 为并行实例）。

核心设计决策：
> "单写入者模式 — Keep the write path single-threaded even when the run is async。worker 是唯一允许进行写入的 agent；reviewer 只在需要修正时做小的编辑。"
>
> "父 agent 拥有完整的编排控制权；子 agent 是聚焦的、临时的、带有明确职责边界的工作者。"
>
> "链式优先于复杂提示 — 推荐的工作流程是明确分解为 scout → planner → worker → reviewer 等步骤，而不是在一个复杂的提示词中要求一个 agent 完成所有事。"

嵌套限制：最大深度 3、最大步骤 12、最大子 agent 数 16。

**工作区隔离**：`worktree: true` 为每个并行写入任务创建独立 git worktree 避免文件冲突。要求干净 git 状态。

**agentOverrides**：不通过复制 agent 文件改模型，用覆盖配置。支持 `model`、`thinking`、`fallbackModels`、`tools`、`skills` 等字段。

**intercom 桥作为可选依赖**：有桥时子 agent 可通过 `contact_supervisor` 做运行时双向通信；没有桥时仅通过正常工具调用结果返回。"有桥更好，没有也不影响基本功能"。

### harness

六种团队架构模式：

1. **Pipeline**：`[分析] → [设计] → [实现] → [验证]` — 强顺序依赖
2. **Fan-out/Fan-in**：`[分发] → [专家A|B|C] → [合并]` — 并行处理后合并。要求 Agent Teams 模式（agent 间需共享发现）
3. **Expert Pool**：`[路由器] → {专家A|B|C}` — 按上下文选择性调用
4. **Producer-Reviewer**：`[生成] → [审查] →（问题）→ [生成]` — 配对，最大重试 2-3 次
5. **Supervisor**：`[监督者] → [工人A|B|C]` — 动态分配。与 Fan-out 的区别：Fan-out 事先固定分配，Supervisor 运行中动态调整
6. **Hierarchical Delegation**：`[总监] → [组长] → [组员]` — 递归委派，建议 2 层以内

两个执行模式：
- **Agent Teams**（默认）：agent 间直接通信（SendMessage），共享任务列表（TaskCreate/TaskUpdate）。团队成员可互相挑战和验证。"에이전트 팀이 기본이다"（Agent Teams 是默认模式）
- **Subagents**（轻量）：子 agent 只向父返回结果，互不通信。适合一次性任务。

决策规则：
> "Agent 间需要通信吗？如果答案为'可能'，用 Teams。只有当通信'确实不需要'时才选 Subagents。"

复合模式：Fan-out + Producer-Reviewer、Pipeline + Fan-out、Supervisor + Expert Pool。

A/B 测试（n=15，作者自测）：+60% 质量得分（49.5→79.3），100% 胜率，-32% 输出方差。效果随复杂度增长（基础 +23.8，高级 +29.6，专家 +36.2）。

### meta-engineering-harness

独立验证：
> "One agent implements from the compiled contract while another agent writes tests from the same contract without seeing the implementation."
>
> "The harness enforces independence structurally through separate agents, separate job payloads, separate execution queues, no shared conversation history, and verifier access to the contract rather than implementation reasoning."

注意力验证：
> "The same or a similar model is directed into different reviewer roles: product reviewer, architecture reviewer, security reviewer, backend reviewer, frontend reviewer, QA tester, and shipping reviewer."

两者的区别：
> "Independence-based verification reduces implementation blindness. Attention-based verification reduces single-pass attentional blindness."

论文指出这不是形式化独立性："If both foundation models are trained on similar data or both receive an incomplete contract, they may share blind spots."

---

## 6. 渐进式披露

### agents-best-practices

工具可见性分 6 层：
```
base tools: always visible
task tools: visible after task classification
skill tools: visible after skill selection
connector tools: visible after connector authorization
deferred tools: discoverable by search
sensitive tools: hidden until needed and approved
```

> "Large tool surfaces confuse the model and waste context."
>
> "Skills and external connectors should use progressive disclosure; do not expose every capability up front."（原则 #7）
>
> "Do not show every tool all the time."

工具描述原则：
> "A good tool description says: when to use the tool; when not to use it; required prerequisites; side effects; important error behavior; examples of valid arguments."
>
> "Keep descriptions concise. If a tool requires extensive documentation, expose a small discovery tool or reference resource rather than putting all details in the tool description."

### harness

Skill 的三层加载：
> "SKILL.md（入口）→ references/（按需），防止上下文爆炸"

Skill 和 Agent 的三种连接方式：
- **Skill 工具调用**：agent prompt 中指定 invoke skill——"재사용성이 높으면 Skill 도구"
- **Prompt 内联**：50 行以下且此 agent 专用——"전용이면 인라인"
- **Reference 按需加载**：内容大且条件性需要——"대용량이면 레퍼런스 로드"

---

## 7. 外部反馈循环与自我改进

### meta-engineering-harness

**十八步 Pipeline**（Pre-Pipeline 6 步 → Pipeline 7 步 → Post-Pipeline 5 步）：

Pre-Pipeline：运营需求 → raw issue → contract compiler 生成结构化合约 → product review 检查是否该构建 → engineering review 检查状态转换/数据依赖/架构/失败模式 → 合约定稿。Pipeline：implementation agent 接收合约 → test agent 接收合约 → implementation 写代码 → test agent 写对抗测试 → CI 运行 → 失败路由仲裁器 → 四路分类决定下一步。Post-Pipeline：structural review → QA staging/browser/API → shipping → Retro agent 审查失败历史 → 人类批准永久记忆和 specialization 变更。

外层校准循环：
> "The pipeline does not stop at deployment. Each failure is an observation about the harness."
>
> "A recurring bug may require a new regression test. A recurring spec gap may require a contract template update. A recurring review failure may require a new checklist item. A recurring ambiguity may require a new compiler rule."
>
> "This is the calibration loop. The harness improves by converting failures into reusable process changes."

论文核心断言：
> "Contract incompleteness is the highest-leverage unsolved problem in the system. The harness is only as good as the contract. If a critical requirement is missing, the builder may not implement it and the verifier may not test it."

### agents-best-practices

Harness engineering loop：
```
agent fails or slows down
  → identify missing capability, context, validator, or permission rule
  → encode the fix into docs, tools, policies, schemas, or evals
  → rerun and measure
  → keep the improvement as part of the harness
```

> "Treat harness building as a feedback loop, not as a one-time prompt-writing exercise."
>
> "Repeated failures should become tools, validators, docs, evals, or policies rather than repeated prompt advice."（原则 #13）

熵管理：
> "Agentic systems accumulate entropy: stale docs, duplicated rules, weak examples, obsolete tools, and low-quality patterns that future runs imitate."

定期清理工作流：
```
doc freshness scans / tool inventory cleanup / quality score updates
technical debt tracker updates / stale plan archival / repeated-failure analysis
prompt/tool bundle review / regression eval additions
```

> "Continuous cleanup is cheaper than waiting until drift becomes systemic."

---

## 8. 来源间分歧

### 多 agent 通信：直接 vs 通过父路由

**harness** 的 Agent Teams 默认模式让 agent 间直接通信（SendMessage），认为这是"互相挑战、共享发现"的基础。决策规则是"agent 间需要通信吗？如果可能，用 Teams"。

**pi-subagents** 相反——子 agent 只和父 agent 通信，子 agent 间互不知道对方存在。没有 inter-agent 通信的概念。

这是最根本的架构分歧。harness 假设 agent 间横向通信是质量的关键驱动力。pi-subagents 假设父 agent 的编排控制权高于一切。两个系统在生产环境都有效，说明这个选择取决于场景——没有唯一正确答案。

### 团队模式：预设模板 vs 原语组合

**harness** 提供 6 种预设团队架构模式，用户从模式中选，harness 生成具体 agent 和 skill 文件。

**pi-subagents** 只提供委托原语（single/parallel/chain/dynamic-fanout），用户用这些原语在自己的代码中组合出任意模式。pi-subagents 的 8 种 agent 类型是角色定义（通过 prompt 实现），不是架构模式。

### 审批粒度：策略引擎 vs 上报机制

**agents-best-practices** 用 7 种权限决策结果 + 14 级风险分类构建完整的策略引擎。

**pi-subagents** 只提供 `contact_supervisor` 上报通道——子 agent 把决策推给父 agent，不自己判断风险等级。

两种思路各有利弊：策略引擎（agents-best-practices）更自动化但更复杂；上报机制（pi-subagents）更简单但需要父 agent 始终在线。

### 合约的来源：人工 + 编译 vs 人工

**meta-engineering-harness** 投入大量工程在合约编译上（Two-Pass Contract Compilation），认为合约质量是 harness 效果的上限。

**agents-best-practices** 的 MVP Builder Mode 也生成结构化蓝图，但更强调"先用最小的 spec 开始，不够再加"。

其他来源（harness、pi-subagents）没有合约编译概念——它们假设 prompt 或 agent 定义本身就是"合约"。
