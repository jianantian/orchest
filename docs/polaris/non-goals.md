# 不做什么（Non-Goals）

明确不做什么，与明确要做什么同等重要。以下是本 runtime 的硬性边界，遇到需求冲动时应回来核对。

## 永久 Non-Goals

**"什么都能做"的 agent 框架**
- 不内置复杂工作流引擎（DAG、条件分支、循环编排）
- 不提供预制 agent 模板和角色系统
- 不提供 skill marketplace 或内置 skill 库
- 不试图替代 LangChain / LangGraph 的全部功能

**绑定特定厂商**
- 不绑定 Anthropic 作为唯一模型 provider（v0.1 只实现 Anthropic 是务实选择，不是设计约束）
- 不假设用户使用特定云平台或基础设施

**偏离 Anthropic Agent Skills 开放标准**
- SKILL.md 格式不得与官方标准不兼容
- 不创造与标准并行的私有 skill 格式

## v0.1 明确不做（留给 v0.2+）

| 功能 | 原因 |
|------|------|
| MCP server 集成 | 独立功能模块，v0.1 先验证核心架构 |
| Tool Search Tool（渐进式 tool 加载） | tool 数量不是 v0.1 的实际瓶颈 |
| Context compaction | 超长 session 是 v0.2 才面对的问题 |
| 多 model adapter（OpenAI 等） | 一个 adapter 足以验证架构 |
| Multi-agent 协作 | sub-agent 的 budget 继承和事件嵌套是独立复杂度 |
| Skill 沙箱（firejail/bubblewrap） | v0.1 要求用户审核 skill 来源，sandboxing 是安全增强 |
| 并行 tool call | 顺序执行保持审批门简单，并行是 v0.2 优化项 |
| Webhook 模式异步 tool | polling 模式先验证，webhook 是补充 |
| Persistent script mode | 只在冷启动成为实测瓶颈后才值得做 |

## v1.0 之前的开放问题（不是 Non-Goals，是未决定）

- **Skill 依赖管理**：skill 的 Python/Node 脚本需要特定依赖时，runtime 怎么准备环境
- **Code Execution as MCP**：是否支持 agent 写代码调用 tool（Anthropic 在推的高级模式）
- **Skill sub-agent**：skill 是否能在内部启动 sub-agent，budget 怎么继承、event 怎么嵌套
