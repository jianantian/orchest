# 概念边界：Tool / MCP / Skill

这三个概念在 Anthropic 生态中常被混淆。本 runtime 严格按照官方定义区分，任何设计决策都不应模糊这三者的边界。

## Tool

**定义**：模型可以调用的最小原子能力单元。

Tool 是 agent 与外部世界交互的接口抽象，包含：
- 名称、描述、输入 schema、可选的输出 schema
- 实现（execute 函数）
- 元数据（side_effect、requires_approval、timeout 等）

Tool 关心的是"能做什么"，不关心"怎么提供给 runtime"。

## MCP（Model Context Protocol）

**定义**：tool 提供方的标准协议，不是 tool 的替代品或某种特殊 tool 类型。

MCP 是把 **tool 的发现、定义、执行**从应用代码中解耦出来的协议层。

> 官方定义："Function calling 是模型表达想做什么的方式。MCP 是让这些请求在不同系统间可移植、可发现、可执行的基础设施。"

通过 MCP 接入的 tool 和用户直接注册的 in-process tool，在 runtime 内部通过同一个 `Tool` trait 统一对待——区别只在 `execute` 的实现，不在接口。

## Skill

**定义**：通过文件系统组织的**过程性知识包**，按渐进式披露的方式向 agent 暴露。

Skill 的核心**不是 tool 的集合**，而是 how-to 知识——告诉 agent 如何完成某类任务的指令、最佳实践、参考材料。Skill 可以选择性 bundle 脚本（实现为 tool），但很多 skill 完全由 markdown instructions 构成。

物理形态是一个目录：
```
my-skill/
├── SKILL.md           # 必需：metadata + 指令
├── references/        # 可选：详细文档（按需读取）
├── scripts/           # 可选：可执行脚本
└── assets/            # 可选：模板、资源
```

## 三者的关系

按 Anthropic 官方表述：
- **MCP connects Claude to external services and data sources**（MCP 提供能力）
- **Skills provide procedural knowledge—instructions for how to complete specific tasks**（Skill 提供过程性知识）
- **You can use both together: MCP connections give Claude access to tools, while Skills teach Claude how to use those tools effectively**

具体层次：
- Tool 在**能力层**——模型调用的接口
- MCP 在**协议层**——连接外部 tool 提供方的传输协议
- Skill 在**知识层**——组织过程性知识 + 可选 bundled tool 的文件系统包

三者不是并列的"功能模块"，而是不同抽象层次的设施。

## 与可观测性的关系

可观测性不改变 Tool / MCP / Skill 的定义，也不是第四种 capability。

- `RuntimeEvent`、`tracing`、`metrics` 只描述 runtime 中发生了什么，不提供新的模型可调用能力
- MCP tool 和 in-process tool 的观测字段可以不同（例如 `mcp_transport`），但它们在能力层仍然都是 Tool
- Skill 的 discovery、activation、script execution 可以被观测，但 Skill 仍然是过程性知识包，不因为有日志或指标而变成 Tool 集合

日志、指标、token 归因和错误可观测性的统一规范见 [observability.md](./observability.md)。
