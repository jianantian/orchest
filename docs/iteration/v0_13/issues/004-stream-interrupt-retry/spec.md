# 004 — 流中断可重试 + 一行重试配置

## 背景

两处叠加(SDK-C4):

1. 模型重试默认关:`AgentConfig.retry_policy` 默认 None(`crates/orchest/src/run/config.rs:517`),Py/Node 绑定硬编码 None——429/5xx/timeout 本就配了也不重试,除非使用方显式配。
2. SSE 流中断归类 NoRetry:流式中途断开报 `stream_error`/`stream_interrupted`(`crates/orchest-provider-http/src/sse/mod.rs:300-302` 附近),因无 HTTP status、code ≠ "timeout" 被 `retry.rs` 归类为 NoRetry——已流出的部分内容丢弃,run 直接 RunFailed。

## 目标/范围

1. 流中断(网络层 SSE 中断)归类为**可重试**——安全边界:重试发生在 `ChatModel::complete()` 单次调用层面,部分流事件已转发但未对 state 产生任何提交,丢弃重发是安全的。
2. builder 提供一行可开的推荐重试配置(如 `RetryPolicy::recommended()` 之类的构造 + 绑定透传),覆盖 429/5xx/timeout/流中断;**不改 None 默认值**(不静默改变现有使用方行为)。
3. 绑定层(Py/Node)透出重试配置入口。

## 验收标准

- [x] 流中断 + 有 retry policy:重试成功,run 正常完成
- [x] 流中断 + 无 retry policy(默认):行为与现状一致(RunFailed)
- [x] 429/5xx/timeout 的既有重试路径不回归
- [x] 推荐配置一行可开,rustdoc 写明覆盖的错误类与默认次数/退避
- [x] Py/Node 绑定透出配置入口
- [x] 测试:流中断重试 / 次数上限 / 默认不重试 / 既有路径不回归
- [x] 四件套 + cargo doc 全绿

## 备注

- demo(countdown/review/chat)随后即可一行开启重试,属 demo 侧后续。
