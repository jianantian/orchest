# Orchest — Agent 工作指南

## 这是什么

Orchest 是一个**底层 Rust SDK**，为开发者提供构建 AI agent 应用的运行时核心。它不是一个完整的 agent 产品，而是其他 agent 产品的发动机：负责 agent loop、状态管理、事件流、tool 调度、skill 加载。

当前阶段：**纯文档，无实现代码**。所有工作在 `docs/` 目录下进行。

---

## 术语边界（最重要，不得混淆）

这三个概念在 Anthropic 生态中常被混淆，Orchest 严格区分：

| 概念 | 定义 | 层次 |
|------|------|------|
| **Tool** | 模型可以调用的最小原子能力单元，包含名称、schema、execute 实现 | 能力层 |
| **MCP** | Tool 提供方的传输协议，不是特殊的 tool 类型 | 协议层 |
| **Skill** | 通过文件系统组织的过程性知识包（SKILL.md + 可选脚本），按渐进式披露暴露给 agent | 知识层 |

**MCP 不是 tool 的替代品**，它是把 tool 的发现和执行从应用代码中解耦出来的协议层。通过 MCP 接入的 tool 和直接注册的 in-process tool 在 runtime 内部通过同一个 `Tool` trait 统一对待。

**Skill 的核心不是 tool 的集合**，而是 how-to 知识。很多 skill 完全由 markdown 构成，调用的是当前会话已有的 tool。

遇到任何关于这三者边界的疑问，以 `docs/polaris/concept-boundaries.md` 为准。

---

## 文档地图

```
docs/
├── overview.md              # 产品定位、核心概念、设计哲学（面向外部读者）
├── spec.md                  # 原始技术设计参考（已部分过时，见下方权威规则）
├── polaris/
│   ├── concept-boundaries.md  # Tool/MCP/Skill 边界定义（权威）
│   ├── design-principles.md   # 设计原则和决策参考问题
│   └── non-goals.md           # 硬性边界 + 无沙箱环境的最低安全建议
├── iteration/
│   ├── v0_1/                  # 最小可用：Rust core + 双语言 SDK
│   ├── v0_2/                  # MCP 集成 + OpenAI adapter + context compaction
│   └── v0_3/                  # 生产完整度：skill 依赖 + code exec + sub-agent + 沙箱架构
└── research/
    └── claw-landscape.md      # 7 个同类产品的架构研究（提炼 SDK 层可参考的设计）
```

### 权威规则（重要）

**迭代文档 > spec.md**。`docs/spec.md` 是原始设计，部分内容已被迭代文档取代。如有冲突，以 `docs/iteration/` 为准。`spec.md` 保留作原始参考，不应在其中做"权威"修改。

每个迭代有两层文档：
- `prd.md`：迭代目标、成功指标、范围、不在范围内
- `issues/*.md`：拆解成可执行的实现单元，含验收标准

---

## 迭代状态

| 迭代 | 状态 | 核心内容 |
|------|------|---------|
| **v0.1** | 文档完成，待实现 | Rust core run loop、skill 加载、异步 job、budget guard、approval gate、Python/TS SDK |
| **v0.2** | 文档完成，待实现 | MCP stdio/HTTP、Tool Search Tool、OpenAI adapter、context compaction、webhook 异步 tool |
| **v0.3** | 文档完成，待实现 | Skill 依赖管理、Code Execution MCP、sub-agent、ScriptExecutor 抽象 + capability 声明 |

---

## 锁定的设计决策（不要试图改变）

以下决策已经过充分讨论，不需要重新论证：

- **Rust 核心 + PyO3/napi-rs**：跨语言 SDK 需要 in-process 嵌入而非 IPC，Rust 是唯一合理选择
- **Skill-first**：完整对齐 Anthropic Agent Skills 开放标准，SKILL.md 格式不得与官方标准不兼容
- **MCP 是传输协议而非 tool 类型**：通过 MCP 接入的 tool 在 runtime 内部通过 `Tool` trait 统一对待
- **极简 core**：runtime 只做"循环 + 状态管理 + 事件流"，所有能力外移到 tool 和 skill
- **流式输出是 v0.1 核心**：不是可选项，`ModelAdapter::stream()` 是主路径
- **v0.1 顺序执行 tool call**：保持 approval gate 简单，并行是 v0.2 优化项
- **沙箱留到 v0.3 之后**：但 v0.3 必须完成 `ScriptExecutor` trait 抽象和 `capabilities` 声明

---

## 做文档变更时的规范

### 新增 issue

1. 放在对应迭代的 `issues/` 目录下，文件名格式：`NNN-slug.md`（三位数字前缀）
2. 必须包含：背景、目标、验收标准（checkbox 列表）、说明（可选）
3. 验收标准要具体可测，不能写"实现 X"，要写"当 Y 时，Z 成立"

### 修改 spec.md

spec.md 里的类型定义（`ToolMetadata`、`ModelStreamChunk`、`RunStatus` 等）是 v0.1 的实现合同。修改时：
- 同步更新受影响的 issue 验收标准
- 在 `## 设计决策记录` 末尾补充决策理由

### 修改 polaris 文档

polaris 文档记录的是**不随迭代变化的约束**。修改要谨慎，改之前先确认这真的是永久边界而不是当前迭代的取舍。

### 不要做的事

- 不要在 spec.md 里直接做"权威"变更而不更新对应 issue
- 不要在 overview.md 里加实现细节（overview 面向外部读者）
- 不要把多频道路由、用户管理、Web UI 等产品层需求带进 SDK 设计
- 不要在没有对应 polaris 依据的情况下新增 Non-Goal

---

## Commit 规范

前缀：`docs:`（文档）、`feat:`（功能，实现阶段）、`fix:`（修复）、`refactor:`（重构）

Subject 示例：
- `docs: add v0.2 issue for webhook async tool`
- `docs: clarify ScriptExecutor trait in spec`
- `docs: rename v1.0 to v0.3, add sandbox-ready architecture`

Subject 长度控制在 72 字符以内。Body 说明变更原因和影响范围（特别是跨多个文件的连锁变更）。

---

## 常用检索命令

```bash
# 在设计文档中搜索关键词
rg "术语或类型名" docs/

# 列出所有文档文件
find docs -maxdepth 4 -name "*.md" | sort

# 查看所有未完成的验收标准
rg "\- \[ \]" docs/iteration/

# 确认没有遗漏的 v1.0 引用（应为空）
rg "v1\.0|v1_0" docs/
```
