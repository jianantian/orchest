# AWS AgentCore Chat vs. Orchest — 对比研究

> 研究日期：2026-05-29  
> 对比对象：[aws-samples/sample-multi-agent-orchestration-chat-on-agentcore](https://github.com/aws-samples/sample-multi-agent-orchestration-chat-on-agentcore) vs. orchest (本仓库)

---

## 一、概述

两个项目解决的是同一领域（多智能体编排）的**不同层次**问题，几乎没有直接竞争关系：

| 维度 | Orchest | AWS AgentCore Chat |
|------|---------|-------------------|
| 定位 | 嵌入式 Agent 运行时 SDK（引擎） | 面向组织的完整多 Agent 应用平台（产品） |
| 核心语言 | Rust + PyO3/napi-rs 绑定 | TypeScript (Node.js 22) |
| 部署方式 | 库，宿主应用自行嵌入 | AWS CDK 一键部署（全托管云服务） |
| 云依赖 | 无（provider-agnostic） | 深度绑定 AWS（Bedrock、Cognito、DynamoDB、AppSync…） |
| LLM 支持 | Anthropic、OpenAI、DeepSeek、OpenRouter | 仅 Amazon Bedrock |
| 许可证 | 内部（未公开） | MIT-0 |

---

## 二、架构对比

### 2.1 Orchest 架构

Orchest 是一个 Rust workspace，共 6 个 crate，严格分层：

```
agent-runtime-model       — provider-agnostic 模型适配器 trait + 类型
agent-runtime-core        — 核心运行时：run loop、工具注册、skill 加载、事件流、
                            budget guard、compaction、sub-agent、webhook、MCP tool
agent-runtime-providers   — 模型适配器（Anthropic、OpenAI、DeepSeek、OpenRouter）
agent-runtime-aigc-providers — 图像生成 / AIGC 网关适配器
agent-runtime-py          — PyO3 绑定（无业务逻辑）
agent-runtime-node        — napi-rs 绑定（无业务逻辑）
```

**设计原则**：核心运行时（`agent-runtime-core`）完全不依赖任何云服务。Python/TypeScript SDK 是 Rust native binary 的纯 FFI 包装。没有 server、没有基础设施。

### 2.2 AWS AgentCore Chat 架构

TypeScript monorepo（~8 个 npm workspace 包），完整的无服务器 AWS 架构：

```
React SPA          → CloudFront + S3
Auth               → Amazon Cognito
Backend API        → Lambda + API Gateway (Express.js)
Agent 执行容器      → AgentCore Runtime (Docker)
工具分发            → AgentCore Gateway → Lambda 工具函数
持久化             → DynamoDB + S3
实时流              → AppSync Events (WebSocket)
定时/事件触发        → EventBridge Scheduler
```

**设计原则**：产品优先，全托管，运维零负担。每个工具是独立的 Lambda 函数，统一注册在 `@moca/tool-definitions` 包中。

---

## 三、编排模型对比

### 3.1 Orchest 的编排模型

单 Agent run loop + 可选 sub-agent 嵌套调用：

```
model call → stream events → tool execution → compaction (if needed) → repeat
```

- run loop 在 `run/loop_.rs`，所有逻辑显式可见
- sub-agent 通过嵌套 `AgentRun` 实例在进程内通信（`run/sub_agent.rs`）
- 无工作流引擎，无 DAG——有意保持最小化
- budget guard 和 approval gate 是一等公民

### 3.2 AWS AgentCore Chat 的编排模型

依赖 Amazon Bedrock AgentCore Runtime 管理 Agent 容器生命周期：

- 工具调用是从 AgentCore 容器发出的 HTTP 请求 → Lambda 工具函数
- 会话状态流向：DynamoDB → DynamoDB Streams → AppSync Events → WebSocket → 浏览器
- EventBridge 负责定时/事件驱动的自主 Agent 触发
- Cognito Developer Authenticated Identities 将前端 JWT 会话关联到后端事件

**关键差异**：Orchest 的编排逻辑在自己的代码里，完全可审计；AWS AgentCore Chat 的编排是托管服务，逻辑不可见。

---

## 四、工具与技能系统对比

### 4.1 Orchest 的三层模型

| 层 | 定义 | 特点 |
|----|------|------|
| **Tool** | 原子能力（名称 + JSON schema + `execute` impl） | 进程内或 MCP，通过 `Tool` trait 统一处理 |
| **MCP** | 协议层（stdio 或 HTTP transport） | 不是独立工具类型，是 tool 的通信方式 |
| **Skill** | `SKILL.md` 文件组织的过程性知识包 | 提供"如何做"的说明 + 可选脚本；运行时动态加载；与 Anthropic Agent Skills 开放标准对齐 |

Skill 系统的**渐进式披露**（progressive disclosure）是核心设计目标：Agent 按需加载能力，而不是在 system prompt 里堆砌所有知识。

### 4.2 AWS AgentCore Chat 的工具系统

- 每个工具是独立的 Lambda 函数
- 工具定义集中在共享库 `@moca/tool-definitions`
- 内置工具：命令执行、Web 搜索（Tavily）、图像生成、GitHub CLI、外部服务
- 内存：短期（会话历史）+ 长期（AgentCore Memory 持久化）
- **没有对应的 Skill 层**——知识只能通过 system prompt 注入，无法动态加载

---

## 五、LLM 提供商支持

**Orchest** 通过 `ModelAdapter` trait 抽象出所有提供商：

```rust
trait ModelAdapter {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    fn stream(&self, request: CompletionRequest) -> impl Stream<Item = StreamEvent>;
}
```

当前实现：Anthropic、OpenAI、DeepSeek、OpenRouter——切换提供商只需替换适配器。

**AWS AgentCore Chat** 硬绑定 Amazon Bedrock，模型列表在 CDK 配置中写死。切换提供商需要重写核心架构。

---

## 六、流式处理对比

**Orchest**：
- 事件通过 `tokio::sync::mpsc` channel 作为 `RuntimeEvent` 枚举变体流式传输
- 长时运行工具使用 `JobHandle` / poll loop 模式
- Webhook 支持外部触发的异步工具响应（`run/webhook.rs`）

**AWS AgentCore Chat**：
- AppSync Events over WebSocket 推送到浏览器
- Agent → 工具的通信是 HTTP 调用（AgentCore Gateway → Lambda）
- 实时性依赖 AWS 托管服务的 SLA

---

## 七、部署与运维

| 维度 | Orchest | AWS AgentCore Chat |
|------|---------|-------------------|
| 部署方式 | 嵌入库，宿主应用负责部署 | `npm run deploy`（CDK）|
| 云依赖 | 无 | AWS 全家桶 |
| 预估成本 | 无基础设施成本 | ~$84/月（100 session） |
| 可移植性 | 任何 Rust 编译目标 | 仅 AWS |
| 运维负担 | 极低（库） | 中等（Cognito 用户管理、Secrets Manager、DynamoDB 初始化） |

---

## 八、优势与局限性

### Orchest 的优势

- **最小、透明、可审计**——所有业务逻辑在 `agent-runtime-core`，绑定层是纯 FFI
- **Provider-agnostic**——4 个主流 LLM 提供商，单一 `ModelAdapter` trait
- **流式是一等公民**，不是后期补丁
- **Skill 系统**与 Anthropic Agent Skills 开放标准对齐，渐进式知识加载
- **工程纪律强**：core 层无 `unsafe`，全面使用 `thiserror`，测试外禁用 `unwrap()`
- **零云锁定**

### Orchest 的局限性（当前阶段）

- 无 UI、无 auth、无持久化——需宿主应用提供
- 无长期记忆系统
- 无代码执行沙箱（v0.3 规划中）
- 许可证未公开，暂为内部项目

### AWS AgentCore Chat 的优势

- **端到端可部署**，单命令上线
- **完整的产品体验**：React SPA、实时 WebSocket 流、认证、Agent 目录、组织内共享
- **托管基础设施**：无服务器自动扩展，无运维负担
- **内置自动化**：EventBridge 定时/事件触发 Agent
- **长期记忆**：AgentCore Memory 原生支持

### AWS AgentCore Chat 的局限性

- **深度 AWS 绑定**——Bedrock、AgentCore、Cognito、DynamoDB、AppSync 缺一不可
- Strands Agents SDK 是唯一执行框架，无插件适配器
- 无 Skill 层——知识只能通过 system prompt 堆砌
- 编排逻辑不可见，依赖托管服务黑盒
- 组织部署复杂度较高（Cognito 用户管理、Secrets Manager 配置、DynamoDB 初始化脚本）

---

## 九、对 Orchest 的启示

以下是 AWS AgentCore Chat 中值得参考的设计点：

### 9.1 组织级 Agent 共享与发现
AWS 有一个 Agent 目录（预置了 developer、data analyst、physicist 等角色），支持组织内发现和共享。Orchest 目前没有 Agent registry 概念，Skill 系统部分覆盖了这个需求，但缺少 Agent-level 的元数据和发现机制。

### 9.2 长期记忆
AWS AgentCore Memory 提供跨会话的持久化记忆。Orchest 目前只有 in-context 的状态，无长期记忆层。这是在多轮、多会话场景中的明显短板。

### 9.3 定时/事件驱动 Agent
EventBridge 让 Agent 可以被调度或被外部事件触发，不依赖用户发起会话。Orchest 的 webhook 支持提供了部分基础，但没有内置的调度语义。

### 9.4 Agent 间路由的显式化
AWS 的 multi-agent 模式有清晰的路由配置（哪个 Agent 处理哪类任务）。Orchest 的 sub-agent 是运行时动态嵌套，缺少声明式的路由层，这在大规模 Agent 网络中会变得难以管理。

---

## 十、总结

两个项目在栈的不同位置解决问题，**互补多于竞争**。

- **Orchest** ≈ 可移植、provider-agnostic 的 Agent 运行时引擎，类似于 AWS AgentCore Runtime 的内部实现——但 Orchest 是可嵌入、可审计、无云锁定的
- **AWS AgentCore Chat** ≈ 在 AgentCore Runtime 之上构建的完整 SaaS 应用，提供了 UI、auth、存储、调度等所有产品层能力

如果 Orchest 的目标是成为下一层产品（Agent 应用平台）的基础，那么 AWS AgentCore Chat 提供了一个很好的参考：它展示了在运行时引擎之上，还需要哪些产品层的能力。核心差距在于：**长期记忆、Agent 发现/目录、声明式路由、以及调度触发**。
