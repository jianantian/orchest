# 不做什么（Non-Goals）

明确不做什么，与明确要做什么同等重要。本文只记录 Orchest 的全局边界，不记录版本范围；具体迭代取舍写在对应的 `docs/iteration/` 或 `docs/hotfix/` 文档中。

## 不是完整 Agent 产品

Orchest 是底层 runtime SDK，不是 OpenClaw、Claude Code、Cursor、Devin 这类面向终端用户的完整 agent 产品。

- 不提供面向 C 端用户的完整 UI、账号、工作区、权限、计费或协作系统
- 不内置产品级 agent 角色、任务管理、会话管理或用户工作流
- 不替宿主应用决定交互模式、部署形态、数据治理或商业策略
- 不把 SDK 设计绑定到某个具体 agent 产品的用户体验

## 不绑定特定模型或厂商

Orchest 的目标是模型无关、供应商无关。

- 不绑定 Anthropic、OpenAI 或任何单一模型 provider
- 不假设某个 provider 的 tool calling、thinking、cache、streaming 语义是唯一标准
- 不假设用户使用特定云平台、数据库、队列、日志后端或部署环境
- 不把 provider-specific 能力泄漏成 core runtime 的唯一抽象

## 不做重型应用框架

Orchest 追求通用性、泛用性、性能、可扩展性和易用性，但这些目标服务于 runtime SDK，而不是把它扩张成全栈框架。

- 不内置复杂工作流引擎（DAG、条件分支、循环编排）
- 不提供预制业务模板、垂直行业 agent 套件或内置 skill marketplace
- 不试图替代 LangChain / LangGraph 的全部功能
- 不把 one-off 应用逻辑沉入 core；应用能力应通过 tool、skill、extension 或宿主应用实现

## 不偏离开放标准

Orchest 优先对齐 Anthropic Agent Skills 与 MCP 等开放生态边界。

- 不创造与 SKILL.md 标准并行的私有 skill 格式
- 不把 MCP 当成特殊 tool 类型；MCP 是 tool provider 的协议层
- 不把 Skill 简化成 tool 集合；Skill 的核心是过程性知识
- 不为了短期便利破坏 Tool / MCP / Skill 的分层边界

## 不内置运营平台

Orchest 必须可观测，但不内置观测、审计或运营平台。

- 不内置日志后端、metrics exporter、OpenTelemetry collector、Prometheus server 或 Web dashboard
- 不替宿主应用决定日志保留、告警、审计、trace 采样或数据脱敏策略
- SDK 只通过 `RuntimeEvent`、`tracing`、`metrics` 暴露统一观测信号；采集、存储、告警、展示由宿主应用负责
- 不为了 dashboard 便利而把高基数字段塞进默认 metric label

## 不牺牲核心质量目标

以下目标是设计约束，不是可选优化项：

- **通用性**：同一 runtime core 应支持多语言 SDK、多 provider、多 tool 来源和多种宿主应用形态
- **泛用性**：core 抽象要服务广泛 agent 产品，而不是只服务单一内部 demo
- **性能**：runtime 不能引入不必要的 IPC、后台服务或重型依赖；热路径应保持可预测
- **可扩展性**：新 provider、tool source、skill executor、observability sink 应能在既有边界内扩展
- **易用性**：SDK API 应直接、明确、可测试；复杂能力可以分层暴露，但不能依赖隐式魔法
