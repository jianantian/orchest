# 外部研究综合分析

> 综合 2026-05-31 的 5 份外部研究。
>
> 本文档汇总各研究中的观点和设计决策，按来源逐一整理，最后做跨来源的交叉观察。

---

## 目录

1. [agents-best-practices](#1-agents-best-practices)
2. [pi-subagents](#2-pi-subagents)
3. [harness](#3-harness)
4. [meta-engineering-harness](#4-meta-engineering-harness)
5. [nimbalyst](#5-nimbalyst)
6. [交叉观察](#6-交叉观察)

---

## 1. agents-best-practices

**来源**：[DenisSergeevitch/agents-best-practices](https://github.com/DenisSergeevitch/agents-best-practices) (v1.2.0)，一个可安装的 Agent Skill（SKILL.md + 16 个 references/ 文件）。

### 核心立场

> "An agent harness is the control plane around a model. The model proposes actions; the harness validates, authorizes, executes, records, summarizes, and returns observations."
>
> "The model does not execute actions directly; the harness does."

Harness 的 16 个组件：Instruction manager, Context builder, Model adapter, Tool registry, Permission engine, Execution engine, State store, Memory and retrieval layer, Compactor, Planner and goal controller, Workflow scheduler, Skill registry, MCP/external connector manager, Approval manager, Trace and evaluation system, Sandbox or execution boundary。

### Loop 不变量

7 条必须在代码中 enforce 的规则：每个 tool call 恰好一个 result；参数先验证再执行；每次副作用前有 permission 决策；结果有界且结构化；硬性 budget（步数、时间、token、成本、tool call 次数）；最终答案基于观察而非假设工具成功；错误、拒绝、取消和超时都成为结构化观察。

### 权限体系

8 级权威层级（provider policy → organization → product → workspace → domain → user task → runtime reminders → tool observations → untrusted content）。内容应按来源标记权威级别，检索到的内容可能包含指令，但那些指令是数据而非策略。

7 种权限决策结果：`allow / deny / ask_user / approval_required / require_stronger_auth / run_in_sandbox / run_as_draft_only`。

14 级风险分类：`read_only / search_only / compute_only / draft_only / write_local / write_internal / write_external / financial / communication / identity_access / security_sensitive / process_execution / network_open_world / destructive / privileged_admin`。

Draft/Commit 分离：高风险动作拆分为独立工具对（`draft_email → send_email`、`prepare_refund → issue_refund`）。Draft 工具通常可自动运行，Commit 工具需要审批。

审批范围：审批必须精确到具体动作，不能将模糊同意视为全面授权。

### 错误处理

结构化错误格式包含 `status`、`type`、`message`、`next_valid_actions`。11 种错误分类：`unknown_tool / invalid_arguments / permission_denied / approval_required / auth_expired / not_found / timeout / rate_limited / conflict / non_idempotent_retry_blocked / internal_error`。

重试策略：只重试安全失败。安全重试包括瞬时模型 API 错误、只读调用的网络超时、幂等检索、模型修正参数后的验证。不自动重试支付、外部发送、破坏性操作、权限变更、幂等性不明确的操作。高风险操作用幂等键和审批记录。

### 上下文与状态

Prompt 不是数据库。在模型上下文之外持久化：当前计划、目标、todo 列表、审批记录、工作流计划、工具追踪、工件、检索资源引用、技能调用、压缩摘要、评估结果、连接器凭证。然后在下一轮只重新附加相关部分。

压缩应保留工作状态和决策记录，而非对话散文。禁止压缩的内容包括审批状态、当前计划、已加载规则、已变更工件。

### 成熟度等级

| Level | 描述 |
|-------|------|
| 0: Answer-only | 无工具执行，仅问答 |
| 1: Retrieval | 可搜索和读取可信资源，无副作用 |
| 2: Drafting | 可建议动作或计划，不可提交 |
| 3: Approval-gated | 执行动作需显式用户或策略批准 |
| 4: Policy-bounded autonomous | 在严格范围、预算和审计控制下自主执行低风险动作 |
| 5: Long-running goal worker | 跨多轮或会话持续执行可度量目标 |

升级原则：仅在评估显示更简单级别不够用时才升级。

### 工具可见性分层

6 层：base tools（始终可见）→ task tools（任务分类后可见）→ skill tools（技能选择后可见）→ connector tools（连接器授权后可见）→ deferred tools（通过搜索发现）→ sensitive tools（直到需要且批准后才可见）。

### 13 条非协商原则

1. 模型不直接执行动作；harness 执行
2. 每个工具调用必须收到工具结果，即使是拒绝、超时或错误
3. 每个有风险的副作用都需要模型外的运行时策略执行
4. 外部、金融、破坏性、安全或受监管动作的 draft 和 commit 应分开
5. 工具 schema 必须窄、类型化、本地验证、可审计
6. 上下文应信息充分、紧凑且缓存感知
7. 技能和外部连接器应使用 progressive disclosure
8. 压缩应保留工作状态而非对话散文
9. 长期目标需要预算、检查点和可度量的完成条件
10. 工作流编排需要持久的 packet 状态、独立验证、集成规则和总预算执行
11. Harness 必须追踪操作事件而不暴露隐藏推理
12. 持久知识应存放在 agent 可读取的真实来源工件中
13. 重复失败应成为工具、验证器、文档、评估或策略，而非重复的 prompt 建议

### 反馈循环

> "Treat harness building as a feedback loop, not as a one-time prompt-writing exercise."

Harness engineering loop：agent 失败或变慢 → 识别缺失的能力/上下文/验证器/权限规则 → 将修复编码为文档/工具/策略/schema/评估 → 重新运行并测量 → 将改进保留为 harness 的一部分。

> "Most agent failures are not caused by insufficient autonomy. They are caused by weak harness boundaries: broad tools, vague instructions, missing approval gates, unstructured tool results, poor context hygiene, and no evals."

### 熵管理

Agent 系统会积累熵：陈旧文档、重复规则、弱示例、废弃工具和低质量模式。建议定期运行清理工作流：文档新鲜度扫描、工具清单清理、质量评分更新、技术债务更新、过期计划归档、重复失败分析、prompt/tool bundle 审查、回归评估新增。持续清理比等到漂移成为系统性问题再处理更便宜。

---

## 2. pi-subagents

**来源**：[nicobailon/pi-subagents](https://github.com/nicobailon/pi-subagents)，Pi Coding Agent 的 npm 扩展（v0.26.0），实现父 agent 委托给子 agent 的架构。

### 核心哲学

父 agent 拥有完整的编排控制权；子 agent 是聚焦的、临时的、带有明确职责边界的工作者。每个子 agent 作为独立的 Pi 进程启动，有自己完整的环境和生命周期，不是轻量级函数调用。

### 8 种内建子 Agent 类型

| Agent | 职责 | 关键限制 |
|-------|------|----------|
| scout | 快速代码库侦察，输出 context.md | 只读+write；低思考预算 |
| researcher | 自主网络调研，输出 research.md | 需要 pi-web-access 扩展 |
| planner | 创建实现计划，输出 plan.md | 不编辑代码；默认 fork 上下文；高思考预算 |
| worker | 唯一写入者，执行批准的计划 | 不能自行做产品/架构决策；默认 fork 上下文 |
| reviewer | 多功能审查，可小的修正性编辑 | review-only 约束优先 |
| context-builder | 构建交接上下文，输出 context.md + meta-prompt.md | 可 web 调研 |
| oracle | 决策一致性顾问，检测方向漂移 | 不编辑文件；不成为第二决策者；默认 fork |
| delegate | 轻量级通用委托 | 无默认输出；最低配置 |

### 四种委托模式

1. **单 agent**：`subagent({ agent: "scout", task: "..." })`
2. **并行**：`subagent({ tasks: [{...}, {...}] })`
3. **链式**：`subagent({ chain: [{...}, {...}, {...}] })`，支持变量 `{task}`、`{previous}`、`{outputs.name}`、`{chain_dir}`
4. **动态扩展**：第一步返回列表 → `expand` 展开为并行子 agent

### 上下文模式

两种子 agent 启动上下文：
- `fresh`：干净会话，不带父历史。适用于需要无偏见的审查或独立任务
- `fork`：从父 session leaf 分支，子 agent 继承父的历史对话。适用于 oracle 审查、worker 延续线程

fork 失败（session 未持久化、leaf 缺失）时会直接报错，不会悄悄降级为 fresh。

子 agent 的上下文继承配置：`systemPromptMode: replace/append`（是否保留基础提示词）、`inheritProjectContext`（是否继承 AGENTS.md/CLAUDE.md）、`inheritSkills`（是否继承父 skills 目录）。

### 写入与决策

单写入者模式：worker 是唯一允许写入的 agent，reviewer 只在需要修正时做小的编辑。worker 遇到未批准的决策时必须通过 `contact_supervisor` 上报，不能自己猜测。oracle 的角色是发现这些决策间隙。

`contact_supervisor` 有两种模式：`need_decision`（阻塞等待父决策）和 `progress_update`（非阻塞汇报进展）。通信桥（pi-intercom）是可选依赖——有桥时可以在运行时双向通信，没有桥时子 agent 通过正常工具调用结果返回。

### 嵌套限制

最大深度 3 层、最大步骤 12、最大子 agent 数 16。

### 其他设计

`agentOverrides`：不通过复制 agent 文件改模型，用配置覆盖 `model`、`thinking`、`fallbackModels`、`tools`、`skills` 等字段。`worktree: true` 为每个并行写入任务创建独立 git worktree 避免文件冲突。`completionGuard` 字段区分需验证的 agent 和不会被误判为实现 agent 的工具型 agent。

---

## 3. harness

**来源**：[revfactory/harness](https://github.com/revfactory/harness) (v1.2.0)，Claude Code 插件，将领域描述自动转化为 agent 团队架构和技能文件，从六种预定义模式中选择。

**生态定位**：L3 Meta-Factory 层的 Team-Architecture Factory 子层。同一层还有 Archon（Runtime-Configuration Factory）和 meta-harness（Codex 移植版）。

### Skill 与 Agent 的区分

Harness 将 Skill 和 Agent 严格分离：

| 维度 | Skill | Agent |
|------|-------|-------|
| 定义 | 过程性知识 + 工具包 | 专家角色 + 行为原则 |
| 位置 | `.claude/skills/` | `.claude/agents/` |
| 触发 | 用户请求关键词匹配 | Agent 工具显式调用 |
| 大小 | 小到大（工作流） | 小（角色定义） |
| 用途 | "怎么做" | "谁来做" |

Skill 是 agent 执行任务时的过程性指南。Agent 是使用 skill 的专家角色。

### 六种团队架构模式

1. **Pipeline**：`[分析] → [设计] → [实现] → [验证]`。每个阶段强依赖前一阶段的产出。瓶颈会延迟整个流水线。

2. **Fan-out/Fan-in**：`[分发] → [专家A|B|C] → [合并]`。并行处理后合并结果。适用于同一输入需多角度分析。Harness 认为此模式必须用 Agent Teams（而非 Subagents），因为 agent 间需要共享发现和互相挑战。

3. **Expert Pool**：`[路由器] → {专家A|B|C}`。按上下文选择性调用合适的专家。路由器分类准确度是核心。适合 Subagents 模式（只需调用需要的专家，无需常驻团队）。

4. **Producer-Reviewer**：`[生成] → [审查] →（问题）→ [生成]`。成对工作，必须设最大重试次数（2-3 次）防无限循环。适合 Agent Teams 模式（生成者和审查者间的实时反馈）。

5. **Supervisor**：`[监督者] → [工人A|B|C]`。中央 agent 管理任务状态，运行时动态分配。与 Fan-out 的区别：Fan-out 是事前固定分配，Supervisor 是运行中根据进度动态调整。适合 Agent Teams 模式（共享任务列表天然匹配）。

6. **Hierarchical Delegation**：`[总监] → [组长] → [组员]`。递归委派，复杂问题逐步分解。建议 2 层以内，超过 3 层延迟和上下文损失过大。

复合模式在实际中更常见：Fan-out + Producer-Reviewer、Pipeline + Fan-out、Supervisor + Expert Pool。

### 两个执行模式

- **Agent Teams**（默认）：agent 间直接通信（SendMessage），共享任务列表（TaskCreate/TaskUpdate）。团队成员可以互相挑战和验证。基本原则是：Agent Teams 是默认模式，只有当 agent 间通信"确实不需要"时才选 Subagents。
- **Subagents**（轻量）：子 agent 只向父返回结果，互不通信。适合一次性任务。

### Agent 定义模板

每个 agent 按固定结构定义：YAML frontmatter（name + description）→ 核心角色 → 工作原则 → 输入/输出协议 → 团队通信协议 → 错误处理 → 协作关系。所有 agent 必须有 `.claude/agents/{name}.md` 文件——即使只用内置类型，也生成定义文件以确保跨 session 重用。所有 agent 强制使用 `model: "opus"`。

### Agent 重用规则

新建 agent 前必须先检查已有 agent 的重叠：完全覆盖则不新建（重用已有），部分覆盖且可通用化则扩展已有，领域特化则新建但保持独立，完全不同的则正常新建。一个 agent 聚焦一个角色，重用性越高、重复越少。

### Skill 的三种连接方式

Skill 与 Agent 的连接方式：Skill 工具调用（高重用场景）、Prompt 内联（50 行以下且专用场景）、Reference 按需加载（内容大且条件性需要）。

### A/B 测试数据

15 个软件工程任务的对照实验（作者自测）：+60% 平均质量得分（49.5 → 79.3），100% 胜率，-32% 输出方差。效果随任务复杂度增长：基础任务 +23.8，高级 +29.6，专家 +36.2。

---

## 4. meta-engineering-harness

**来源**：Briggs & Myshakivskyi (2026), [arXiv 2605.25665](https://arxiv.org/abs/2605.25665)。论文描述了一个合约驱动的对抗验证架构，在 CTO-as-a-service 场景（17 个功能，3-4 周部署窗口）中验证。

### 概念层级

论文将 prompt / context / agent / harness / software factory 严格分层：

| 概念 | 定义 |
|------|------|
| Prompt | 单条指令 |
| Context | 指令周围的信息 |
| Agent | 在约束角色内操作的模型 |
| Harness | 控制 prompt、context、role、tools、verification、feedback 的系统 |
| Software Factory | harness + 合约积累 + 记忆 + specialization registry + 测试套件 + 部署 + 校准历史 |

> "The word harness is deliberate. A harness is not a model, prompt, or agent. It is the surrounding system that makes agent outputs more reliable."

### 七层架构

1. **Contract Layer**：原始需求 → 结构化合约（状态转换、不变量、业务规则、错误分类、认证授权、排除范围、验收条件）
2. **Context Layer**：持久化 markdown 记忆分 Permanent section（人类批准，不可自动修改）和 Rolling section（可压缩/提升/删除的模式观察）
3. **Specialization Layer**：按领域（支付、预约、认证、搜索）的 specialization registry，在合约编译时注入领域特定 requirement。只在置信度超阈值时应用——低置信度时不注入，防止错误假设污染合约
4. **Agent Layer**：每个 agent 有受限角色。合约编译器不做实现，实现 agent 不写对抗测试，审查 agent 没写过实现
5. **Verification Layer**：两种互补机制——Independence-based（实现和测试是不同的 agent，各自只看合约）和 Attention-based（同一模型被赋予不同 reviewer 角色检查不同 surface）
6. **Execution & Review Layer**：CI 运行对抗测试，失败路由到四路仲裁器
7. **Calibration Layer**：将失败转化为可复用的流程改进

### Two-Pass 合约编译

- **Pass 1（完整性）**：将隐含假设显式化——类型、状态转换、边界情况、信任边界、错误条件
- **Pass 2（范围和歧义）**：删除不支持的需求，将歧义从句重写为单一解释。此阶段是在观察到 Pass 1 的过度规格化（over-specification）后引入的——过度规格化危险，因为下游 agent 会把不支持的 requirement 当作硬约束

### 四路失败仲裁器

| 失败类型 | 定义 | 正确动作 |
|----------|------|---------|
| Bug | 实现违反合约 | 修复实现，重新测试 |
| Spec gap | 合约缺少覆盖此行为的 clause | 补充合约，重新实现和测试 |
| Noise | 测试或环境不稳定 | 重试（上限 N 次） |
| Ambiguity | 合约允许多种有效行为 | 合约精细化——不重试实现 |

论文特别强调歧义类：错误分类导致浪费的循环。如果合约允许多种有效行为，重试实现是浪费时间。

### 支付案例的关键教训

所有对抗测试通过后，生产环境仍遗漏了两个行为（折扣计算、非 Stripe 押金扣除），因为合约没有编码这些业务逻辑。根因被归类为 Spec gap：

> "Contract incompleteness means the implementation can satisfy the contract while failing the true business requirement. Verification boundary means an adversarial test suite conditioned on the contract cannot catch behavior outside the contract."

论文认为合约不完备是最高杠杆的未解决问题——harness 只能和合约一样好。

### 十八步 Pipeline

Pre-Pipeline（6 步：需求 → issue → 合约编译 → product review → engineering review → 定稿）→ Pipeline（7 步：实现 agent + 测试 agent 各收合约 → 各自写代码/测试 → CI → 仲裁器分类）→ Post-Pipeline（5 步：structural review → QA → shipping → Retro agent 审查失败历史 → 人类批准记忆和 specialization 变更）。

### 校准循环

Pipeline 不止于部署。每次失败都是对 harness 的观察：重复 bug → 新回归测试，重复 spec gap → 合约模板更新，重复审查失败 → 新 checklist 条目，重复歧义 → 新 compiler 规则。Post-Pipeline 中的 Retro agent 负责审查失败历史并提出 harness 更新建议。

### 部署数据与局限

3-4 周内实现 17 个功能，生成 18 套对抗测试 + 15 套预约模块迭代测试。5 个 bug/缺口 pre-merge 捕获。论文承认：部署证据来自单一组织和私有代码库，无随机基线；多 agent 工作流比直接 model call 更贵更慢；有工具访问权的 agent 创造新攻击面（恶意 issue、被污染的 specialization record）。

---

## 5. nimbalyst

**来源**：[nimbalyst/nimbalyst](https://github.com/nimbalyst/nimbalyst)，面向 Codex 和 Claude Code 开发者的可视化桌面应用（Electron + React）。

Nimbalyst 是一个 AI 原生工作空间，提供可视化编辑器（Markdown、Mermaid、Excalidraw、CSV、代码的 WYSIWYG 编辑器）、session 管理（看板、并行 session、搜索和恢复）、任务追踪、端到端加密团队协作（Yjs CRDT + Cloudflare Durable Objects）、扩展系统（通过 EditorHost 契约接入自定义编辑器）、原生 iOS 和 Android 移动端。

AI 提供商采用双层抽象：AIProvider（模型访问）+ AgentProtocol（agent 会话），支持 Codex、Claude Code、OpenCode、Copilot。数据持久化使用 PGLite（WebAssembly PostgreSQL），状态管理使用 Jotai。

与 SDK 层没有交集——Nimbalyst 是 SDK 的消费者（桌面应用层），不是 SDK 的设计参考。

---

## 6. 交叉观察

### 各来源的共识

**模型与 harness 分离**：所有 4 份相关来源（agents-best-practices、meta-engineering-harness、harness、pi-subagents）在最根本的问题上一致——模型提出动作，harness 执行和裁决。agents-best-practices 最为显式，将其列为 13 条非协商原则的第一条。

**渐进式披露**：agents-best-practices（6 层工具可见性）和 harness（Skill 三层加载）同时强调不要一次性把所有定义暴露给模型。

**上下文控制**：agents-best-practices（8 级权限层级、状态在 prompt 外存储）、meta-engineering-harness（permanent/rolling 分段记忆）、pi-subagents（fresh/fork 上下文模式）都认为上下文管理是一等设计问题。

**子 agent 的上下文继承**：pi-subagents 的 fork 模式和 meta-engineering-harness 的 "reduced role contamination" 都指向同一个方向：子 agent 应该能从父 agent 继承语境，但同时也需要干净上下文的选项。

**失败需要结构化分类**：meta-engineering-harness 的四路仲裁器和 agents-best-practices 的 11 种错误分类 + next_valid_actions 都要求错误携带足够的元数据——不只是 message string。

**反馈循环**：agents-best-practices（harness engineering loop）、meta-engineering-harness（outer-loop calibration）、harness（A/B 测试对比）都在不同层面强调"构建 harness 是持续改进的循环，不是一次性 prompt 编写"。

### 各来源的分歧

**多 agent 通信模式**：harness 的 Agent Teams 默认模式让 agent 间直接通信，认为互相挑战是质量驱动力。pi-subagents 相反——子 agent 只和父通信，互不知道对方存在。两个系统在生产中都有证据支持（harness 的 A/B 数据 +60%，pi-subagents 的 8 种 agent 类型在生产中使用），说明没有唯一正确答案——取决于场景。

**团队模式：预设模板 vs 原语组合**：harness 提供 6 种预设模式让用户选择。pi-subagents 只提供委托原语（single/parallel/chain），用户在自己的代码中组合。前者适合"不知道怎么搭"的场景，后者适合"知道要什么，只需底层能力"的场景。

**审批粒度**：agents-best-practices 用 7 种决策结果加 14 级风险分类构建完整策略引擎。pi-subagents 只给 `contact_supervisor` 上报通道——把决策推给父 agent。两种思路各有利弊：策略引擎更自动化但更复杂，上报机制更简单但要求父 agent 始终在线。

**合约的角色**：meta-engineering-harness 以合约为一切中心（Two-Pass 编译），把合约质量等同于 harness 效果上限。agents-best-practices 也有 MVP Builder 模式但更务实地从最小 spec 开始。harness 和 pi-subagents 没有合约概念——prompt 或 agent 定义就是合约。
