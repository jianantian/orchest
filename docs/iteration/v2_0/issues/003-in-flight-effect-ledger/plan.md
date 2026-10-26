# 003 implementation plan

## Files to read

- `crates/orchest/src/session/log.rs`（事件种类与投影）
- `crates/orchest/src/run/tool_exec.rs`（工具派发与结果落地的实际路径）
- `crates/orchest/src/run/actor.rs`（工具阶段：`run_tool_and_handoff_phase`、`apply_tool_phase_results`）
- `crates/orchest/src/tool/mod.rs`（`ToolOutput`、`ToolError`、`isError` 语义）
- `crates/orchest/src/tool/error.rs`（`ErrorKind` 与稳定错误码的既有写法）

## Files to change

- `crates/orchest/src/session/log.rs`：账本事件类型与账本投影
- `crates/orchest/src/run/tool_exec.rs`：派发前写"将派发"、结算后写"已结算"
- `crates/orchest/src/run/actor.rs`：恢复流程里调用闭包合成
- `crates/orchest/src/run/mod.rs`：`resume` 的闭包合成入口
- `crates/orchest/src/tool/error.rs`：三个稳定错误码的定义位置

## Steps

1. 定义账本事件（将派发 / 已结算）与账本投影：`dispatched_but_unsettled(session_id) -> Vec<CallRecord>`。
2. 在工具派发路径上写"将派发"事件（调用 id、工具名、参数结构摘要，不含凭据值），在结果落地后写"已结算"。
3. 定义三个稳定错误码与面向模型的说明文案；文案要能让模型判断"是否可安全重试"（参考 dsh：只对只读/幂等操作重试）。
4. 在恢复流程中，对每个未结算调用合成一条工具结果（`isError` + 错误码 + 文案）写进日志，并进入消息历史。
5. 合成发生在**恢复的第一次写入之前**，且先 flush（002 的耐久屏障），避免"恢复又崩"。
6. 取消路径：未派发的调用写 aborted-before-dispatch，不进 unknown 集合。
7. 写测试：三种崩溃/取消场景 + 正常路径无噪声。
8. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`；在 `CHANGELOG.md` 记录事件形态与错误码。
