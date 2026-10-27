# 001 · `SessionInput::Finalize` 与 Stable / Turn 契约

GitHub issue: #342

## Background

`SessionInput` 只有 `Audio / Text / ToolResult / Interrupt`（`crates/orchest-protocol/src/capability.rs:29`），客户端无法要求「尽快定稿但不关连接」。`SegmentRef` 的注释规定 `Append` 只给原生增量流（`crates/orchest-protocol/src/stream.rs:62-71`），和 ADR-0006 D3 冲突。`EndOfSpeech.segment` 没有约定必须指向哪一段。

## Goal

把 ADR-0006 的 Finalize、Stable 两种形态和 Turn 指向写进 `orchest-protocol`，作为方言实现与契约测试的依据。

## Acceptance Criteria

- [ ] `SessionInput::Finalize` 新增，文档写明语义：要求尽快定稿，并尽量随后给出 Turn；不关闭 Session。
- [ ] `SegmentRef` 注释改写：Append 与 Snapshot 都是合法的 Stable，具体用哪种由方言（后续由能力目录）声明；同段 Draft 一律 Snapshot。
- [ ] `LifecycleEvent::EndOfSpeech` 注释写明：`segment` 携带它所结束那段 Stable 的 `segment_id`；没有原生 Turn 的方言由映射层合成。
- [ ] 所有现有方言对 `SessionInput::Finalize` 至少有一个明确行为（001 内统一先返回 `UnsupportedOption`，002 再逐个接通）。
- [ ] 变更是增量的（`SessionInput` 为 `#[non_exhaustive]`），`CHANGELOG.md` 的 `## [Unreleased] → ### Added` 记录。

## Blocked by

无。

## Notes

- 只改协议与注释，不改方言映射逻辑；方言行为在 002。
- `UnsupportedOption` 沿用现有错误码，不新增错误类型。
