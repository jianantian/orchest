# Polaris 总入口

> **文档状态：长期参考，不是实现规格。**
>
> Polaris 记录 Orchest 跨版本不轻易变化的定位、边界和设计约束。具体版本范围、API 形状、实施计划和验收条件，以 [iteration/](../iteration/) 与 [hotfix/](../hotfix/) 下的 PRD / issue spec 为准。

## Orchest 是什么

Orchest 是一个低层 Rust agent runtime SDK，用于支撑 agent 应用的核心运行时能力：agent loop、状态管理、事件流、tool dispatch、skill loading、预算与审批控制。

它不是完整 agent 产品，也不是工作流平台。Orchest 的定位是提供一个轻量、透明、语言无关、对齐开放标准的 runtime core，让上层产品把能力、界面、业务流程和部署策略放在自己可控的层里。

## 核心判断

Orchest 的设计围绕几个长期判断：

- **Skill-first**：对齐 Anthropic Agent Skills 开放标准，把渐进式披露作为一等抽象
- **Tool / MCP / Skill 分层**：Tool 是能力层，MCP 是协议层，Skill 是知识层
- **极简 core**：runtime 只做 loop、state、event stream 和安全边界相关的调度，不内置复杂产品能力
- **可观测优先**：agent 的关键行为必须能通过事件、日志、指标和 token 归因被宿主应用观察
- **模型无关、语言无关**：Rust core 通过 binding 服务 Python / Node 等语言，不把业务决策放进 binding 层
- **AI 时代友好**：偏好显式约束、强类型、清晰错误和较低魔法度，让人和 AI 都能读懂系统行为

## 阅读顺序

Polaris 文档按职责拆分：

1. [concept-boundaries.md](./concept-boundaries.md)：Tool / MCP / Skill 的权威边界。任何相关设计分歧先看这里。
2. [design-principles.md](./design-principles.md)：设计原则、决策启发式和长期取舍。
3. [non-goals.md](./non-goals.md)：全局 non-goals、产品边界和核心质量目标。
4. [observability.md](./observability.md)：RuntimeEvent、tracing、metrics、token 归因和错误可观测性规范。

## 权威顺序

当文档之间出现冲突时，按以下顺序处理：

1. 当前工作对应的 `docs/iteration/*/issues/*/spec.md` 或 `docs/hotfix/*/issues/*/spec.md`
2. 对应的 iteration / hotfix PRD
3. Polaris 中的长期约束文档
4. 研究、评审、历史记录和外部说明

Polaris 不应该承载短期任务清单或当前迭代细节。需要改变当前实现契约时，先更新对应 issue spec / PRD；只有当变化属于跨版本长期约束时，才同步修改 Polaris。
