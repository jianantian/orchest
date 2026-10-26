# 004 · 阻塞请求通道（id + kind + payload）

GitHub issue: #334

## Background

宿主（Multivac）需要的不只是"批准/拒绝"：权限请求、提问、计划提案是三件不同的事，但它们在 runtime 侧都是"运行停下来等人"。现在 `ApprovalBus` 用 `RunId` 做键、存 `bool`（`crates/orchest/src/run/handle.rs:265`），因此：

- 同一 run 只能挂一个待决请求（键是 run id）；
- 只能表达同意/拒绝，无法携带"回答了哪个问题""接受的是哪份计划"；
- 无法按 id 回复、撤回或查询；`RunHandle::respond_approval(run_id, approved)` 的参数是 run id（`crates/orchest/src/run/handle.rs` 公开面），重连后无法定位到底在等谁。

业界做法：Claude Code 的 `can_use_tool` 回调带参数上下文，批准结果可携带"更新后的权限规则"（一次性 + 会话级）；Codex 的审批结果按序列化 key 缓存到会话（`core/src/tools/sandboxing.rs:43-72`）；dsh 的批准结果有四态（`allowed-once` / `rejected` / `cancelled` / `unavailable`）并 fail-closed。

ADR-0005 §4.1 已定：Orchest 提供**通用**阻塞请求机制，**kind 词汇由宿主定义**，Orchest 不认识"计划提案"这类产品语义。

## Goal

把阻塞请求从"按 run id 的 bool"升级为"带身份、带类型、带载荷的请求通道"：可按 id 回复、撤回、查询；载荷可承载宿主的任何语义；超时可配且 fail-closed。

## Acceptance Criteria

- [ ] 请求有稳定 id（run 内唯一、可跨重连定位）、`kind`（宿主定义的字符串或标签类型）与 `payload`（`serde_json::Value`）。
- [ ] 通道可同时挂多个待决请求（键是 request id，不再是 run id）；每个待决请求能查到它阻塞的是什么（工具调用、步骤、run）。
- [ ] 宿主可按 id 回复：回复携带决定（同意/拒绝/带内容的回答）与可选的"更新后的会话级规则"载荷。
- [ ] 宿主可撤回一个待决请求（runtime 侧按未批准处理，继续运行）。
- [ ] 超时可配（run 级），超时按**未批准**处理并以稳定错误码结束该次调用；不默认放行。
- [ ] 现有审批语义迁移到新通道：工具审批成为 kind 的一种，行为等价（拒绝 → 工具结果 `APPROVAL_DENIED`）。
- [ ] 事件面同步：`ApprovalRequested` 变体带上 request id / kind / payload（保持 `#[non_exhaustive]`），并新增请求已解决的事件。
- [ ] 子 run 的请求通过同一通道路由到父（沿用 `ChildRunRegistry` 的父子关系），父能按 id 回复。
- [ ] 测试覆盖：并发多个待决请求、按 id 回复、撤回、超时 fail-closed、子 run 请求路由。

## Blocked by

001（日志模型；请求的生命周期要进日志）。

## Notes

- 载荷用 `serde_json::Value` 而不是泛型：runtime 不该知道宿主的类型系统，序列化边界是干净的接缝。
- 旧 API `respond_approval(run_id, approved)` 属于公开面破坏性变更，硬切换删除（不保留别名）。
- 与 005 的关系：005 决定"要不要问"，004 决定"怎么问、怎么答"。
