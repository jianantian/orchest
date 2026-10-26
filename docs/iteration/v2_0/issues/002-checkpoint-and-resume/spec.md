# 002 · checkpoint 落点、耐久屏障与恢复契约

GitHub issue: #332

## Background

ADR-0005 D1 定下：执行态权威是 Orchest 的会话契约，checkpoint 粒度是**每个 step**，并且**每次工具副作用派发前额外落一个标记**——否则"工具已执行、结果未落盘"的窗口无法判定。

对照业界（调研 §2）：dsh 的落点是语义边界（每次模型请求前、每次顶层工具副作用前、每次 step 前，`packages/session/session-checkpoint-policy/src/index.ts:46-66`）；Codex 每 item 追加；jcode 每消息/每工具结果后保存并按 512KB 阈值转快照；Kocoro 每工具批次后；Claude Code 每 turn 一条转录、文件快照每 prompt 一个。

Orchest 当前只在 run 终止时保存（`crates/orchest/src/session/persistence_hook.rs`），且没有显式的耐久屏障概念。

## Goal

给会话日志定义**明确的落盘点、明确的耐久屏障、明确的恢复边界**，让"崩溃后从哪继续、哪些东西不重跑"有确定答案。这是 003 的前提。

## Acceptance Criteria

- [ ] 落盘点定义并实现：step 起点（模型请求前）、**工具副作用派发前**、step 终点、run 终止（成功/失败/中止）。
- [ ] `SessionStore::flush` 是显式耐久屏障：调用返回后，此前接受的事件在进程崩溃后仍可读；`append` 只保证顺序接受（后端可以更强，但契约以 flush 为准）。
- [ ] checkpoint 策略可配：至少 `every_step`（默认）与 `every_tool`（更细，代价是更多写入）；策略值记录在会话元数据里，恢复时可知。
- [ ] 恢复入口 `resume` / `resume_with_input` 从日志读取并投影，重建与崩溃前一致的消息历史、step 与预算。
- [ ] 恢复边界明确：**只恢复到最后一个完整 step 边界**；崩溃时未闭合的 step 被显式标记为中断，而不是当作已完成继续。
- [ ] 恢复时配置由宿主重新提供（hooks / retry / handoffs / store / 模型 / 工具注册）；运行时不从日志恢复这些，也不假设它们与崩溃前一致。
- [ ] 恢复后第一次写入之前，先完成一次 flush，避免"恢复即崩溃"再次丢状态。
- [ ] 中止（abort）也落一条终止事件：中止的 run 同样可被读取和投影。
- [ ] 测试覆盖：step 中途崩溃 → 恢复到上一个 step 边界；工具派发前崩溃 → 该调用被标记为未开始；run 结束后 resume → 与结束前投影一致。

## Blocked by

001（会话事件日志与派生投影）。

## Notes

- 「工具副作用派发前」的标记依赖 003 的效果分类词汇；本 issue 先用工具名 + 调用 id 落标记，003 补上效果维度。
- flush 的成本是 fsync 级别；`every_step` 策略下每 step 一次。是否提供 `every_tool` 取决于宿主对丢失窗口的容忍度，指标上应由宿主显式选。
