# Meta-Engineering Harness：合约驱动的对抗验证架构

> 论文：[Meta-Engineering Harnesses for AI-Native Software Production (arXiv 2605.25665)](https://arxiv.org/abs/2605.25665)
>
> 作者：Tamunokorite Briggs, Ivan Myshakivskyi（HireNimbus）

---

## 论文目标

论文研究的问题是：**不是 AI 能否生成一个网站、支付集成或工作流一次，而是一个系统能否持续地生产、验证、部署、维护和升级这些基础设施**——跨越多个运营上相似但结构不同的业务。

部署场景是 CTO-as-a-service：为没有 CTO、PM、工程团队、QA、DevOps 的小型服务企业提供持续运营的技术基础设施。

---

## 核心概念层级

论文定义了一个严格的概念层级，将通常混在一起的几个概念区分开：

| 概念 | 定义 |
|------|------|
| **Prompt** | 单条指令 |
| **Context** | 指令周围的信息 |
| **Agent** | 在约束角色内操作的模型 |
| **Harness** | 控制 prompt、context、role、tools、verification、feedback 的系统 |
| **Software Factory** | 更大的生产系统：harness + 合约积累 + 记忆 + specialization registry + 测试套件 + 部署 + 校准历史 |

---

## 七层架构

论文提出了七层 meta-engineering harness：

1. **Contract Layer**（合约层）：原始需求 → 结构化合约。合约包含模块名、用户角色、API/UI 表面、期望行为、输入输出、状态转换、不变量、业务规则、错误分类、认证授权、数据依赖、排除范围、QA 目标、回归风险、验收条件。

2. **Context Layer**（上下文层）：持久化 markdown 记忆，分两段：
   - **Permanent section**：人类批准的制度知识，自动化流程不能直接修改
   - **Rolling section**：最近的模式观察，可被压缩、提升或删除

3. **Specialization Layer**（专业化层）：按任务领域（支付、预约、认证、搜索、移动端）维护的 specialization registry。每个 specialization record 在合约编译时注入领域特定的 requirement（如支付 specialization 要求幂等键、显式状态转换、信任边界检查）。Specialization 只在置信度超过阈值时应用。

4. **Agent Layer**（代理层）：每个 agent 有受限的角色——合约编译器不做实现、实现 agent 不写对抗测试、审查 agent 没写过实现代码。论文认为这减少了角色污染，使分类失败更容易。

5. **Verification Layer**（验证层）：两种互补机制：
   - **Independence-based**：实现 agent 和测试 agent 是**不同 agent**，各自只看合约不看对方的产出，减少实现偏见
   - **Attention-based**：同一个模型被赋予不同 reviewer 角色（产品审查、架构审查、安全审查、后端审查、前端审查、QA、发布审查），各自检查不同 surface，减少单次注意力的盲区

   论文强调这不是形式化的独立性——两个 agent 可能共享训练数据偏差或对不完整合约的共享盲点。

6. **Execution & Review Layer**（执行与审查层）：CI 运行对抗测试，失败路由到四路仲裁器。

7. **Calibration Layer**（校准层）：每次失败都是对 harness 自身的观察——重复的 bug → 新回归测试、重复的 spec gap → 合约模板更新、重复的审查失败 → 新 checklist 条目、重复的歧义 → 新 compiler 规则。

---

## Two-Pass 合约编译

合约从原始 issue 经过两次编译：

- **Pass 1（完整性）**：将原始 issue 转化为结构化草稿，将隐含假设显式化——类型、状态转换、边界情况、信任边界、错误条件
- **Pass 2（范围和歧义）**：删减草稿，移除不支持的需求，将歧义从句重写为明确单一解释

Pass 2 是在观察到 Pass 1 的合约可能出现"过度规格化"后引入的——过度规格化危险，因为下游 agent 会把不支持的 requirement 当作硬约束。

---

## 四路失败仲裁器

每次对抗测试失败后，仲裁器将失败分类为四种类型：

| 失败类型 | 定义 | 正确动作 |
|----------|------|---------|
| **Bug** | 实现违反合约 | 修复实现 → 重新测试 |
| **Spec gap** | 合约缺少覆盖此行为的 clause | 补充合约 → 重新实现和测试 |
| **Noise** | 测试或环境的不稳定 | 重试（上限 N 次） |
| **Ambiguity** | 合约允许多种有效行为 | 合约精细化——不重试实现 |

论文强调合约歧义类特别重要：错误分类导致浪费的循环。

---

## 三阶段 Pipeline

### Pre-Pipeline
1. 运营需求出现
2. 草拟原始 issue
3. Contract compiler 生成结构化合约
4. Product review 检查是否是该构建的东西
5. Engineering review 检查状态转换、数据依赖、架构、失败模式
6. 合约定稿

### Pipeline
7. Implementation agent 接收合约
8. Test agent 接收合约
9. Implementation agent 写代码
10. Test agent 写对抗测试
11. CI 运行测试
12. 失败路由到仲裁器
13. Bug/spec/noise/ambiguity 分类决定下一步动作

### Post-Pipeline
14. Structural review 检查架构、repo 模式、信任边界、性能、可维护性
15. QA 检查 staging、浏览器行为、API 行为、移动端行为
16. Shipping workflow 部署或发起 merge request
17. Retro agent 审查失败历史 → 提出 harness 更新建议
18. 人类批准永久记忆和 specialization 变更

---

## 部署证据

在 3-4 周的部署窗口内，harness 实现了 17 个功能：强制更新弹窗、应用内支付、预约模块、产品落地页、MCP 搜索工具集成、Slack 通知工作流、6 个服务商网站、若干 bug 修复。

生成了 18 套对抗测试，外加预约模块迭代校准期间的 15 套。5 个 bug 或实现缺口在合并前被捕获。

---

## 支付案例研究（关键教训）

**任务**：实现应用内支付（Stripe PaymentIntents + Stripe Connect），后端 Lambda + React Native 前端。

**结果**：后端实现在两轮 CI 后通过所有对抗测试。从合约和测试角度看，实现行为上正确。

**但两个生产遗漏**：
1. 最终发票支付没有正确扣除通过 Stripe 外部手动标记的押金——实现只用 Stripe payments 表计算押金总额
2. 折扣计算没有正确应用到最终 Stripe 金额

**根因**：合约不完备。合约正确约束了无折扣、无押金的基本支付流程，但没有编码完整的业务逻辑。

**教训**：
- **合约不完备**：实现可以满足合约，但无法满足真实业务需求
- **验证边界**：以合约为条件的对抗测试无法捕获合约之外的行为

这两点直接推动了两个 harness 改进：合约编译和精细化流程，以及结构化审查门。

---

## 其他失败模式

- 一个 bug 修复尝试无法完成，因为相关代码文件过大且文档不足，超出合约和上下文层可可靠覆盖的范围
- 一个实现需要手动编辑缓存键来完成 harness 部分解决的修复
- 2-3 个服务商网站实现生成的代码在通过部署检查前需要手动重构

---

## 评价指标

论文定义了 harness 级别的评价指标（而非模型级别）：

- **合约违规检测率**：合并前捕获的实现违规比例
- **审查门精确度**：审查失败中对应真实问题的比例
- **平均实现循环数**：每个功能的平均尝试次数
- **歧义检测率**：歧义合约被正确路由到合约精细化而非实现重试的频率

---

## 论文自述的局限

1. **合约不完备**是最高杠杆的未解决问题——harness 只能和合约一样好。如果关键需求缺失，builder 可能不实现，verifier 可能不测试
2. **共享模型盲点**：对抗独立性是结构性的，不是形式化的——不同 agent 可能共享训练数据偏差、分布假设或系统性误读
3. **验证覆盖**：测试采样行为，不能证明正确性。以合约为条件的测试不能捕获合约之外的失败
4. **人类瓶颈**：部分决策需要人类判断——产品意图、信任边界、歧义 tradeoff、失败分类。大规模下人类介入必须变为例外驱动
5. **上下文漂移**：持久化 markdown 记忆可能过时、膨胀或矛盾。压缩降低风险但不消除
6. **成本和延迟**：多 agent 工作流比直接 model call 更贵更慢。完整运行可能需数分钟，并行化并不总是可行
7. **安全**：有工具访问权的 agent 创造了新攻击面——恶意 issue、被污染的 specialization record、不安全的工具调用都可能影响下游行为
8. **评估困难**：很难把 harness 的效果与模型改进、人类专业度、任务选择、团队熟悉度、创始人/运营者参与度等因素隔离开。部署证据来自单一组织和私有代码库，没有随机对照基线。结果应视为早期运营证据，而非外部验证的性能声明

---

## 论文的核心主张

> "可靠性应该在整个 harness 层评估，而不是单次 model call。模型能生成 artifact。Harness 能让生产过程可度量、可审计、可持续改进。"
>
> "对于 CTO-as-a-service，这种架构标记了短期技术套利和可扩展运营模式之间的区别——后者建立在自适应生产系统之上。目标不是一次性构建一个网站或工作流。目标是构建一个能反复构建、验证、运营和改进跨多个企业技术基础设施的生产系统。"
>
> "持久的资产不是生成的网站、预约流程或支付集成。而是积累的生产系统本身：合约、specialization records、失败分类、回归套件、客户特定上下文、工作流模板、QA 目标、部署基础设施、校准历史。"
