# 004 · SDK ASR 流式接口重设计

GitHub issue: #345

## Background

Python 的 `start_asr_stream` 把 provider 和 model 合在一个 `provider` 参数里（`provider/model`），只有 `finish()`（关会话），没有「定稿但不关」；`AsrStreamEvent` 是 `dict[str, Any]`（`python/orchest/__init__.pyi`）。Node 侧同形。owner 已定：在 SDK 0.2 发布前按 ADR-0006 重设计，不保留旧形状。

## Goal

Python 与 Node 的 ASR 流式接口直接体现 Session / Draft / Stable / Turn / Finalize。

## Acceptance Criteria

- [ ] `start_asr_stream` 接受独立的 `provider` 与 `model` 参数；旧的 `provider/model` 合写不再接受。
- [ ] 流对象新增 `finalize()`，不关闭会话，错误分三条路径：
  - 静态：流启动时保存 `descriptor()`；`client_finalize` 为 `false` 时 `finalize()` 同步抛出带 `UnsupportedOption` 码的异常，不发送。
  - 入队：输入通道已关闭（会话已结束）时同步抛出异常。
  - 运行期：入队成功即返回；方言之后的拒绝以 `error` 事件送达，事件带 `operation: "finalize"`、错误码，`fatal: false`，流继续可用。文档写明 `finalize()` 返回只表示已提交，结果看随后的 `turn` 或 `error` 事件。
- [ ] 事件带 `kind`：`draft` / `stable` / `turn`（加 `error`、`session_started`、`session_closed` 等生命周期类）；`stable` 带 `update`（`append` / `snapshot`）与 `segment_id`；`turn` 带它结束的 `segment_id`。
- [ ] Python：`.pyi` 与运行时类型对齐（TypedDict 联合）；Node：`types.d.ts` 联合类型；`scripts/check-ts-event-variants.sh` 通过。
- [ ] 一次性 `transcribe` 同步拆出 `model` 参数。
- [ ] SDK 测试覆盖新参数、`finalize()` 的三条错误路径与每种事件形状（用 `orchest-provider` 的 `testing` fake，不依赖网络）。
- [ ] Python / TypeScript SDK 指南更新示例。

## Blocked by

001（`SessionInput::Finalize`）。002 合入后再做端到端核对。

## Notes

- 只改 ASR 相关接口；Agent、`complete`、`decide` 的形状不动（属 SDK 0.3）。
