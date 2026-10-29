# 001 · `SessionInput::Finalize` 与 Stable / Turn 契约

GitHub issue: #342

## Background

`SessionInput` 只有 `Audio / Text / ToolResult / Interrupt`（`crates/orchest-protocol/src/capability.rs:29`），客户端无法要求「尽快定稿但不关连接」。`SegmentRef` 的注释规定 `Append` 只给原生增量流（`crates/orchest-protocol/src/stream.rs:62-71`），和 ADR-0006 D3 冲突。`EndOfSpeech.segment` 没有约定必须指向哪一段。

## Goal

把 ADR-0006 的 Finalize、Stable 两种形态和 Turn 指向写进 `orchest-protocol`，作为方言实现与契约测试的依据。

## Acceptance Criteria

- [ ] `SessionInput::Finalize` 新增，文档写明语义：要求尽快定稿，并尽量随后给出 Turn；不关闭 Session。
- [ ] 静态能力：每个流式 ASR 方言在 `descriptor()` 的 `ext` 中声明 `CapabilityExt::Asr({"client_finalize": <bool>})`；001 内全部为 `false`。调用方在发送前据此判断，不支持时不发送。
- [ ] 运行期回执：`SessionInput::Finalize` 文档写明，方言收到后必须在事件流上产出且只产出一个结果，二选一：
  - 一个 Turn（`EndOfSpeech`，之前可有 Stable）；
  - 一个非致命错误 `StreamEvent::Error { fatal: false }`，其 `ProtocolError.code` 为 `UnsupportedOption` 或上游错误映射的码，`diagnostic_metadata.operation = "finalize"`。
  两种情况下 Session 都保持打开。`input` 发送成功只表示入队，不表示被接受。
- [ ] `SegmentRef` 注释改写：Append 与 Snapshot 都是合法的 Stable，具体用哪种由方言（后续由能力目录）声明；同段 Draft 一律 Snapshot。
- [ ] `LifecycleEvent::EndOfSpeech` 注释写明：`segment` 携带它所结束那段 Stable 的 `segment_id`；没有原生 Turn 的方言由映射层合成。
- [ ] 所有现有方言收到 `SessionInput::Finalize` 时按上面的回执产出结果：001 内统一发出 `operation = "finalize"`、`code = UnsupportedOption` 的非致命错误，002 再逐个接通。每个方言一条单元测试覆盖该事件。
- [ ] 变更是增量的（`SessionInput` 为 `#[non_exhaustive]`），`CHANGELOG.md` 的 `## [Unreleased] → ### Added` 记录。

## Blocked by

无。

## Notes

- 只改协议与注释，不改方言映射逻辑；方言行为在 002。
- `UnsupportedOption` 沿用现有错误码，不新增错误类型。
- SemVer：`RealtimeHandle`、`Asr` trait 与 `CapabilityDescriptor` 的字段都不动。新增的只有 `SessionInput` 变体（`#[non_exhaustive]`）和 `CapabilityExt::Asr` 里的数据，均为增量，符合 ADR-0003 的 minor。
- 不采用的方案：在 `SessionInput::Finalize` 里放 ack 通道（`SessionInput` 派生 `Clone` / `Serialize` / `PartialEq`，放不进 oneshot）；给 `RealtimeHandle` 加字段（它没有 `#[non_exhaustive]`，属 major）；给 `Asr` trait 加方法（`descriptor()` 已是静态能力通道）。
- `client_finalize` 在语音 V2 进入有类型的 `AsrCatalogExt`；本迭代只用 `Value` 承载。
