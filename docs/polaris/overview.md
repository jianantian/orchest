# Skill-First Agent Runtime: 核心概念与定位

> **文档状态：Polaris 总体参考。**
>
> 本文用于说明 Orchest 的产品定位、核心概念和长期设计哲学，不作为当前实现规格或验收标准。涉及 Tool / MCP / Skill 的永久边界时，以 [concept-boundaries.md](./concept-boundaries.md) 为准；涉及具体版本范围、API、验收条件和实施计划时，以 [iteration/](../iteration/) 与 [hotfix/](../hotfix/) 下的 PRD / issue spec 为准。
>
> 本文中的实现示例用于解释概念，不应单独作为开发任务的 source of truth。

## 引言

这是一个为 AI agent 设计的 runtime——核心简单、可观测、语言无关。它服务于一个具体的判断：在 AI 大量参与代码生成的时代，agent 系统的设计重心从"runtime 多聪明"转向"context 多可塑"，从"框架封装多少能力"转向"用户和 AI 能否清晰看到每一步发生了什么"。

这个 runtime 的核心特征：

- **Skill-first**：完整对齐 Anthropic Agent Skills 开放标准，把渐进式披露作为一等抽象
- **MCP-native**：把 MCP 作为 tool 提供方的标准协议，而不是独立的 tool 类型
- **模型无关**：通过 model adapter 接入任意 LLM provider，不绑定特定厂商
- **语言无关**：核心用 Rust 实现，通过 PyO3 和 napi-rs 提供 Python 和 TypeScript 的原生 SDK
- **极简核心**：runtime 只做"循环 + 状态管理 + 事件流"，所有能力外移到 tool 和 skill
- **AI 时代友好**：设计假设是 AI 写大部分代码，因此偏好显式约束、强类型、清晰的反馈回路

## 核心概念：Tool / MCP / Skill 的明确边界

这三个概念在 Anthropic 生态中常被混淆，本 runtime 严格按照官方定义区分：

### Tool

**定义**：模型可以调用的最小原子能力单元。

Tool 是 agent 与外部世界交互的接口抽象。一个 tool 包含：
- 名称、描述、输入 schema、可选的输出 schema
- 实现（execute 函数）
- 元数据（side_effect、requires_approval、timeout 等）

Tool 关心的是"能做什么"，不关心"怎么提供给 runtime"。

### MCP（Model Context Protocol）

**定义**：tool 提供方的标准协议。

MCP 不是 tool 的替代品或某种特殊 tool 类型——它是把 **tool 的发现、定义、执行**从应用代码中解耦出来的协议层。

引用官方定义："Function calling 是模型表达想做什么的方式。MCP 是让这些请求在不同系统间可移植、可发现、可执行的基础设施。"

在本 runtime 中，MCP 是连接外部服务的标准传输方式。通过 MCP 接入的 tool 和用户应用直接注册的 in-process tool，在 runtime 内部通过同一个 `Tool` trait 统一对待——区别只在 `execute` 的实现，不在接口。

### Skill

**定义**：通过文件系统组织的过程性知识包，按渐进式披露的方式向 agent 暴露。

Skill 的核心**不是 tool 的集合**，而是 **how-to 知识**——告诉 agent 如何完成某类任务的指令、最佳实践、参考材料。Skill 可以选择性 bundle 脚本（实现为 tool），但很多 skill 完全由 markdown instructions 构成，调用的是当前会话已有的 tool。

Skill 的物理形态是一个目录：

```
my-skill/
├── SKILL.md           # 必需：metadata + 指令
├── references/        # 可选：详细文档（按需读取）
├── scripts/           # 可选：可执行脚本
└── assets/            # 可选：模板、资源
```

SKILL.md 的 frontmatter 至少包含 `name` 和 `description`。Orchest 还支持 runtime 扩展字段，用于声明 bundled script 的依赖和执行能力：

```yaml
---
name: web_extract
description: Extract structured data from web pages
dependencies:
  python:
    - requests>=2.31
    - beautifulsoup4
  node:
    axios: "^1.6"
capabilities:
  network: true
  filesystem:
    read: []
    write: []
  env:
    - OPENAI_API_KEY
  max_memory_mb: 256
bundled_tools:
  - name: extract_page
    description: Extract page metadata
    executable: python
    script: scripts/extract_page.py
    input_schema:
      type: object
---
```

`dependencies.python` 会在 skill 首次执行前创建独立 venv；`dependencies.node` 会创建该 skill 独立的 `node_modules`。`capabilities.env` 是传入脚本进程的环境变量白名单，未声明的父进程环境变量不会被继承。

### 三者的关系

按 Anthropic 官方表述：
- **MCP connects Claude to external services and data sources**（MCP 提供能力）
- **Skills provide procedural knowledge—instructions for how to complete specific tasks**（Skill 提供过程性知识）
- **You can use both together: MCP connections give Claude access to tools, while Skills teach Claude how to use those tools effectively**

具体到我们的 runtime：
- Tool 是模型调用的接口
- MCP 是连接外部 tool 提供方的传输协议
- Skill 是组织过程性知识 + 可选 bundled tool 的文件系统包

三者不是并列的"功能模块"，而是不同抽象层次的设施：MCP 在协议层，tool 在能力层，skill 在知识层。

## 渐进式披露：核心设计原则

借鉴 Anthropic Agent Skills 的三阶段设计，扩展到整个 runtime：

### Skill 的三层渐进披露

**Discovery（启动时）**：所有已注册 skill 的 `name` + `description` 进入 system prompt。每个 skill 大约 80 token 的 footprint，可以同时注册数百个 skill 而不爆 context。

**Activation（按需）**：当 agent 判断某个 skill 相关时，通过 file read tool 读取完整 SKILL.md（typical 500-2000 tokens）。

**Execution（深入）**：如果任务需要更详细的信息，agent 进一步读取 skill 目录里的 `references/` 文件，或调用 `scripts/` 里的脚本。

### Tool 的渐进披露（v0.2 考虑）

Anthropic 已经推出 Tool Search Tool，把渐进披露原则应用到 tool 本身——agent 不需要看到所有 tool 的完整定义，只看到名称索引，需要时再加载详细 schema。这对接入大量 MCP server 的场景很有价值。

v0.1 不实现，但 tool registry 的设计要为后续扩展留口。

## 与 Anthropic 官方 Skills 的兼容性

设计目标是 SKILL.md 格式与 Anthropic Agent Skills 开放标准（agentskills.io）兼容，意味着：

- 用户写的 skill 可以同时在本 runtime 和 Claude Code、Claude.ai 中使用
- Anthropic 官方开源 skill 可以直接 drop-in 使用
- skill 作者只需要学一套规范，不需要为本 runtime 单独适配

不兼容的部分（需要明确）：
- Anthropic 的某些 skill 假设有 code execution 环境（Python REPL），本 runtime 通过 spawn 子进程模拟
- bundled scripts 的 schema 声明需要扩展到 SKILL.md 的 frontmatter 中（Anthropic 的脚本是约定式的，没有 schema 声明）

Code Execution as MCP 在 Orchest 中是可信代码执行能力：启用 `AgentConfig.code_execution_enabled` 后，runtime 注册 `execute_python` 和 `execute_javascript` 两个内置 tool。v0.3 不提供完整文件系统或网络沙箱，不适合执行来自外部不可信来源的代码。

## 这个 Runtime 不会变成什么

- 一个"什么都能做"的 agent 框架
- 内置复杂工作流引擎
- 提供预制 agent 模板和 skill marketplace
- 试图替代 LangChain / LangGraph 的全部功能
- 偏离 Anthropic Agent Skills 开放标准

它的定位非常窄：**给真正想做 agent 应用的开发者一个轻量、透明、AI 时代友好、对齐开放标准的底层 runtime**。

## 总结

这个 runtime 的价值不在于功能多，而在于它的几个核心约束：

- 严格区分 tool / MCP / skill，不混淆它们的抽象层次
- 对齐 Anthropic Agent Skills 开放标准，与社区生态互通
- 让 agent 的每一步推理可见（包括 token 级流式输出）
- 让用户的 Python/TS 代码能直接调用，不需要管理额外进程
- 原生处理长时异步 tool，不把等待成本转嫁给用户代码
- 让 Rust 的严格性帮 AI 写出更可靠的实现

它是为那些**真正想做 agent 产品、希望对齐开放标准、且不希望被 framework 绑架**的人设计的工具——足够薄，让你能看清楚每一个抽象的代价；足够稳，让你能在它上面长期演化你自己的产品。
