# Provider-neutral Atomic Completion

## Background

Python/TypeScript 下游目前只能通过完整 `Agent` loop 调用模型。单轮结构化抽取不需要 tools、
budget、steps 或 runtime events，但仍应复用同一 provider adapter、option lowering 与 retry taxonomy。

## Goal / Scope

- 在协议层增加 `ResponseFormat::{Text, JsonObject}`，并纳入 `RequestOptions`。
- Chat wire 支持 `json_object`；不支持的 Messages wire 在 preflight 明确拒绝。
- 在 `orchest` 提供 provider-neutral 的 atomic completion helper：构造 system/user messages、调用
  `ChatModel`、按需重试、提取完整文本并校验 stop reason。
- helper 是 Python/Node 共用业务逻辑；绑定不复制重试或 response 解析。

## Acceptance Criteria

- [x] `ResponseFormat::Text` 是 wire-compatible 默认值，旧序列化 payload 缺字段仍可反序列化。
- [x] `JsonObject` 在 Chat request 中精确生成 `response_format:{"type":"json_object"}`。
- [x] 不支持 JSON mode 的方言在发送 HTTP 前返回稳定错误，不静默降级。
- [x] atomic completion 只调用一次 adapter（无 retry 时），tools 为空，不启动 agent run。
- [x] system 为空时不发送 system message；user message 始终存在且保持原文。
- [x] 返回响应中所有 Text block 的有序拼接；Thinking/tool block 不进入文本。
- [x] `JsonObject` 响应文本在返回前解析并验证为 JSON object；非法 JSON 或 array/scalar 顶层值报协议错，
      成功时仍返回 provider 原始文本。
- [x] `retry=true` 只重试 recommended policy 覆盖的 transient 类别，默认不重试。
- [x] 只有 `EndTurn` 与 `StopSequence` 返回成功文本；`ToolUse`、`MaxTokens`、`ContentFilter`、`Refusal`、
      `ContextWindowExceeded`、`Pause`、`Interrupted` 与 `Other` 全部返回稳定错误。
- [x] provider unit tests、core unit tests、serde compatibility tests 全部通过。

## Notes

- 本 issue 不公开语言绑定；绑定在 Issue 003。
- 不实现 JSON Schema，避免提前扩大 provider option contract。
