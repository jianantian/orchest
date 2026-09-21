# v0.17 — System One Decisions API

状态：已完成（2026-09-22）；实现见 [001-system-one](./issues/001-system-one/spec.md)（PR [#296](https://github.com/jianantian/orchest/pull/296)）。live OpenRouter 调用未运行。

## 背景

Orchest 已有 Chat、ASR、TTS、Realtime 和 GenTask 能力。System One 回答
`state + questions` 中的窄问题，返回概率、分类和评分，适合作为应用中的原子判断。
本次建设可由远程或本地实现的通用 Decision 能力；OpenRouter Decisions 是首个 adapter，使用 `OPENROUTER_API_KEY`。公共契约不包含 `noul`、OpenRouter endpoint 或默认模型。

## 目标与范围

- 在 `orchest-protocol` 定义独立 `Decision` 能力，支持 Boolean、Choice、Score。
- 在 `orchest-provider-http` 实现 OpenRouter `/api/alpha/decisions`，通过
  `orchest-provider` registry 与公共工厂暴露。
- Rust、Python、TypeScript 均提供一次性 `decide` API，返回完整结构化结果。
- 支持字符串、对象、数组形式的 state 和 instructions；保留结构化 criteria。
- 提供离线契约测试、三种语言的客服分流示例和 SDK 文档。

应用自行决定阈值、分流、重试和副作用。此次不改变 agent loop，不引入自动
System One/System Two 路由，不接 TypeSafe 原生服务，不新增独立 provider crate。

## 方案比较与决定

1. **独立 Decision 能力（推荐）**：复用 protocol/registry/HTTP 分层，类型和错误
   可跨语言共享；增加一个能力契约与 registry bucket。
2. OpenRouter 专用 HTTP helper：代码较少，但能力发现、类型和多语言入口容易分叉。
3. 扩展 ChatModel：可复用聊天工厂，但 messages、文本生成、流事件与 Decisions
   的问答契约不一致，会让消费者依赖错误的抽象。

采用方案 1。实现保持现有 provider wall；runtime、bindings 和示例不直接依赖
`orchest-provider-http`。

## 成功标准

用户给出的三问题请求能通过三个 SDK 发往正确 endpoint，并保留问题 ID、
答案类型、概率、置信度、score legend 和 usage。非法请求在网络调用前失败，
上游 HTTP 错误与畸形成功响应通过结构化错误报告。离线测试及仓库检查通过。

## Issue

| 顺序 | Issue | 内容 |
|------|-------|------|
| 1 | [001-system-one](./issues/001-system-one/spec.md) | 契约、OpenRouter provider、三语言 API、测试与示例，作为一个完整接入提交 |

## 权威来源

- [OpenRouter OpenAPI](https://openrouter.ai/openapi.json)：2026-09-21 核对
  `/api/alpha/decisions` 及八个 `Decisions*` schema。
- [TypeSafe API](https://docs.typesafe.ai/api)：解释三种 primitive 的语义。
- [How to build with TypeSafe](https://docs.typesafe.ai/concepts/how-to-build-with-system-one)：
  保持代码控制流程，批量提出独立窄问题。
- [ADR-0001](../../../adr/0001-provider-unification.md)、
  [ADR-0002](../../../adr/0002-protocol-provider-decoupling.md)。

OpenRouter adapter 的字段必填规则优先于原生 TypeSafe 文档；这些 wire 规则不定义公共能力。Alpha API 的不兼容变化
通过 provider 契约测试发现；不声称离线 fixture 等价于 live 验证。
