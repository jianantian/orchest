# Harness — The Team-Architecture Factory for Claude Code

**仓库**: https://github.com/revfactory/harness (v1.2.0)
**一句话描述**: 一个 Claude Code 插件，将领域描述自动转化为 agent 团队架构和技能文件——从六种预定义团队架构模式中选择。

## 概述

Harness 是 Claude Code 生态中 **L3 Meta-Factory** 层的 **Team-Architecture Factory** 子层。用户说一句"build a harness for this project"，插件就把领域描述转化为协调的 agent 团队（`.claude/agents/`）和技能文件（`.claude/skills/`）。

Harness 不生成运行时配置——那是相邻子层 Archon（Runtime-Configuration Factory）的事。Harness 生成的是团队结构、消息协议、审查门和编排规则。

## 生态位

| 层级 | 定位 |
|------|------|
| **L3 Meta-Factory（Harness）** | 领域描述 → agent 团队 + 技能（通过 6 种团队模式生成） |
| L3 Meta-Factory（Archon） | 确定性、可重复的运行时配置 |
| L3 Meta-Factory（meta-harness） | 同一概念，Codex 运行时移植版 |
| L2 Cross-Harness（ECC） | 跨 harness 的技能/规则/hook 标准化 |

## 六种团队架构模式

### 1. Pipeline（流水线）
顺序执行。上一个 agent 的输出是下一个 agent 的输入。

```
[分析] → [设计] → [实现] → [验证]
```

- **适用**：每个阶段强依赖前一阶段的产出
- **示例**：小说创作——世界 → 角色 → 情节 → 写作 → 编辑
- **风险**：瓶颈延迟整个流水线

### 2. Fan-out/Fan-in（扇出/扇入）
并行处理后合并结果。独立任务同时执行。

```
         ┌→ [专家A] ─┐
[分发] → ├→ [专家B] ─┼→ [合并]
         └→ [专家C] ─┘
```

- **适用**：同一输入需多角度/多领域分析
- **示例**：综合研究——官方/媒体/社区/背景同时调研 → 综合报告
- **要求**：必须用 Agent Teams 模式，因为 agent 间需要共享发现、互相挑战

### 3. Expert Pool（专家池）
根据上下文选择性调用合适的专家。

```
[路由器] → { 专家A | 专家B | 专家C }
```

- **适用**：输入类型决定需要不同的处理
- **示例**：代码审查——安全/性能/架构专家中按需调用
- **注意**：路由器分类准确度是关键

### 4. Producer-Reviewer（生产者-审查者）
生成 agent 和审查 agent 成对工作。

```
[生成] → [审查] →（不合格）→ [生成] 重做
```

- **适用**：产出质量保证至关重要，且有客观验证标准
- **示例**：漫画创作——画师生成 → 审查者检查 → 问题页重绘
- **必须**：设置最大重试次数（2-3 次）防无限循环

### 5. Supervisor（监督者）
中央 agent 管理任务状态，动态分配任务给下属 agent。

```
         ┌→ [工人A]
[监督者] ─┼→ [工人B]    ← 监督者根据状态动态分发
         └→ [工人C]
```

- **适用**：工作量可变，需运行时决定分配
- **与 Fanout 的区别**： Fanout 是事前固定分配，Supervisor 是运行中动态调整
- **示例**：大规模代码迁移——监督者分析文件列表，给工人分配批次

### 6. Hierarchical Delegation（层级委派）
上层 agent 向子 agent 递归委派，将复杂问题逐步分解。

```
[总监] → [组长A] → [组员A1]
                 → [组员A2]
       → [组长B] → [组员B1]
```

- **适用**：问题自然形成层级结构
- **示例**：全栈应用——总监 → 前端组长 → (UI/逻辑/测试) + 后端组长 → (API/DB/测试)
- **注意**：3 层以上延迟和上下文损失过大，建议 2 层以内

### 复合模式

实际使用中单模式少见，复合模式是常态：

| 复合模式 | 构成 | 示例 |
|----------|------|------|
| Fan-out + Producer-Reviewer | 并行生成后各自审查 | 多语翻译 → 各语言独立翻译 + 母语审查 |
| Pipeline + Fan-out | 顺序阶段中部分并行化 | 分析(顺序) → 实现(并行) → 集成测试(顺序) |
| Supervisor + Expert Pool | 监督者动态调用专家 | 客户咨询处理 → 分类后分配对应专家 |

## 两个执行模式

| 模式 | 机制 | agent 间通信 | 适用 |
|------|------|-------------|------|
| **Agent Teams**（默认） | TeamCreate + SendMessage + TaskCreate | agent 间直接通信、相互挑战 | 2+ agent 需协作 |
| **Subagents** | Agent 工具直接调用 | 子 agent 只向父返回结果 | 一次性任务，无需 agent 间通信 |

**决策规则**：Agent Teams 是默认选择。只有当 agent 间通信"确实不需要"时才选 Subagents。

## 七阶段工作流

```
Phase 1: Domain Analysis           领域分析
    ↓
Phase 2: Team Architecture Design  团队架构设计（Agent Teams vs Subagents 选择）
    ↓
Phase 3: Agent Definition Gen      生成 .claude/agents/ 文件
    ↓
Phase 4: Skill Generation          生成 .claude/skills/ 文件（Progressive Disclosure）
    ↓
Phase 5: Integration               编排、agent 间数据传递、错误处理
    ↓
Phase 6: Validation                验证、dry-run 测试、有 skill 无 skill 对比测试
```

## 生成产物结构

```
your-project/
├── .claude/
│   ├── agents/          # agent 定义文件（每个 agent 一个 .md）
│   │   ├── analyst.md
│   │   ├── builder.md
│   │   └── qa.md
│   └── skills/          # 技能文件
│       ├── analyze/
│       │   └── SKILL.md
│       └── build/
│           ├── SKILL.md
│           └── references/
```

## Agent 定义结构

每个 agent 按固定模板定义：

```markdown
---
name: agent-name
description: "1-2 句角色概述。触发关键词列表。"
---

# Agent Name — 角色一句话总结

你是 [领域] 的 [角色] 专家。

## 核心角色
1. 角色1
2. 角色2

## 工作原则
- 原则1
- 原则2

## 输入/输出协议
- 输入: [从哪里获取什么]
- 输出: [写到哪里、格式]

## 团队通信协议（Agent Teams 模式）
- 消息接收: [接收来自谁的什么消息]
- 消息发送: [向谁发送什么消息]
- 任务请求: [从共享任务列表请求什么类型的任务]

## 错误处理
- [失败时的行为]
- [超时时的行为]

## 协作
- 与其他 agent 的关系
```

## Skill vs Agent 区分

| 维度 | Skill | Agent |
|------|-------|-------|
| 定义 | 过程性知识 + 工具包 | 专家角色 + 行为原则 |
| 位置 | `.claude/skills/` | `.claude/agents/` |
| 触发 | 用户请求关键词匹配 | Agent 工具显式调用 |
| 大小 | 小→大（工作流） | 小（角色定义） |
| 用途 | "怎么做" | "谁来做" |

Skill 是 agent 执行任务时的过程性指南。Agent 是使用 skill 的专家角色。

## 关键设计决策

1. **Agent Teams 是默认模式** — "Agent 间需要通信吗？"如果答案为"可能"，用 Teams
2. **Progressive Disclosure** — 技能文件按层加载：SKILL.md（入口）→ references/（按需），防止上下文爆炸
3. **所有 agent 使用 `model: "opus"`** — 坚持使用最强模型以保证输出质量
4. **所有 agent 必须有 `.claude/agents/{name}.md` 文件** — 即使只用内置类型，也生成定义文件以确保跨 session 重用和团队通信协议显式化
5. **Agent 重用优先于新建** — 定义 agent 前检查现有 agent 的重叠度，避免角色重复累积
6. **依赖 Claude Code Agent Teams API** — 需要 `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`

## A/B 测试证据

Harness 团队在 15 个软件工程任务上做了对照实验：

| 指标 | 无 Harness | 有 Harness | 提升 |
|------|:---:|:---:|:---:|
| 平均质量得分 | 49.5 | 79.3 | **+60%** |
| 胜率 | — | — | **100%** (15/15) |
| 输出方差 | — | — | **-32%** |

关键发现：效果随任务复杂度增长——越难的任务，改善越大（基础 +23.8，高级 +29.6，专家 +36.2）。

**注意**：n=15，作者自测，第三方复现待进行。

## 局限

1. **Claude Code 独占** — 官方运行时只支持 Claude Code。Codex 移植版（meta-harness）在另一个仓库
2. **Agent Teams 不支持嵌套** — 团队成员不能创建自己的团队
3. **团队 Leader 固定** — 无法在运行中切换 leader
4. **单 session 只能有一个活跃团队** — 虽然可以在 phase 间解散重建，但不能同时跑多个团队
5. **依赖实验性 API** — Agent Teams 是 Claude Code 的实验功能，未来 API 可能变化
6. **opus-only 模型选择** — 所有 agent 强制 opus，成本较高
7. **生成产物需要验证** — 自动生成的 agent 定义和 skill 需要人工审查和调优
8. **A/B 测试样本小** — n=15，作者自测，外部有效性待验证
