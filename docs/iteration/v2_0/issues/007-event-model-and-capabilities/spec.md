# 007 · 事件模型补齐与能力探测面

GitHub issue: #337

## Background

宿主需要两样东西，现在都没有：

1. **消息生命周期**。`RuntimeEvent` 有 `ModelStreamChunk`（`crates/orchest/src/events.rs:60`）与 `ModelCallCompleted`，但没有"消息开始/增量/结束"这类消息级事件，宿主只能自己按模型调用聚合文本。
2. **能力声明**。宿主无法知道 runtime 是否支持恢复、抢占取消、会话级批准、某类请求、并行工具、委派、效果分类、注入的隔离等级，只能靠 try/catch 或版本猜测。cindy 这类多 harness 宿主把"显式 `NotSupportedError`"当成刚需（调研 §8）。

业界对照：Pi 的事件面是 `message_start` / `message_update` / `message_end` 三段式；Claude Code 的类型面把 turn / session / model 事件都列出来（`mods/types/claude-code.d.ts`）；dsh 的事件是 merge-extensible 且带 `ignorable` 标记。

同时要守住的边界（ADR-0005 §4.1）：**产品事件不进 Orchest**——计划提案、任务计划是宿主概念，Orchest 只提供通用请求事件。

## Goal

补齐宿主消费事件流所需要的最小面：消息生命周期、通用请求事件、账本事件（003）、终止原因（006）；并提供一个可查询的**能力探测面**，让宿主按能力分支而不是靠猜。

## Acceptance Criteria

- [ ] 新增消息级事件：消息开始、带增量的消息更新、消息结束；文本与思考增量区分清楚。
- [ ] 新增通用请求事件：请求发出（带 id / kind / 载荷）、请求已解决（带决定）；不引入任何产品语义事件。
- [ ] 账本事件（派发/结算）与终止原因事件进入同一事件面。
- [ ] 所有新增事件变体保持 `#[non_exhaustive]`，加变体不构成破坏性变更（ADR-0003）。
- [ ] 提供能力探测面：runtime 明确声明支持/不支持——恢复、抢占取消、批准作用域（一次性/会话级）、请求 kind（宿主可扩展）、并行工具、委派、效果分类集合、注入的隔离等级、事件零丢失保证。
- [ ] 能力声明可查询且**可验证**：声明支持的路径有对应测试；声明不支持的路径返回显式的不支持错误，而不是静默降级。
- [ ] 事件序列化在 Rust / Python / TypeScript 三端保持一致，`scripts/check-ts-event-variants.sh` 通过。
- [ ] 事件文档（`docs/guide/` 或事件参考页）列出全部变体与语义，并写明"哪些是执行态、哪些是观察流"。

## Blocked by

001（日志与事件面的边界）、004（请求事件）。

## Notes

- 事件面同时被 SDK 消费；本次 Rust 侧新增变体后，bindings 的事件类型要同步（TS 侧有 `scripts/check-ts-event-variants.sh` 这样的漂移防线）。
- 能力探测面是一个结构体 + 一组查询方法，不是 feature 宏；宏适合编译期裁剪，宿主需要运行时可见。
- 能力探测与"软/硬隔离强度"绑定：注入沙箱后端后，隔离等级能力要从 soft 变 hard，探测面必须能反映运行时事实而非编译期常量。
