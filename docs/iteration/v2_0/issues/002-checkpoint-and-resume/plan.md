# 002 implementation plan

## Files to read

- `crates/orchest/src/session/{log.rs,store.rs,sqlite.rs}`（001 的产出）
- `crates/orchest/src/run/actor.rs`（`run_one_step` 的阶段划分：模型调用前后、工具派发前后）
- `crates/orchest/src/run/tool_exec.rs`（工具派发与执行的实际位置）
- `crates/orchest/src/run/config.rs`（`AgentConfig` 的 session store / session id 字段）
- `crates/orchest/src/run/mod.rs`（`resume` / `resume_with_input` 现状）
- `crates/orchest/src/run/supervisor.rs`（监督重启与恢复的关系）

## Files to change

- `crates/orchest/src/session/log.rs`：落点事件类型、checkpoint 策略枚举
- `crates/orchest/src/session/store.rs`：`flush` 契约与实现
- `crates/orchest/src/session/sqlite.rs`：`flush` 实现（事务提交）与元数据里的策略字段
- `crates/orchest/src/run/actor.rs`：在四个落点追加事件并按策略 flush
- `crates/orchest/src/run/tool_exec.rs`：工具派发前的标记写入点
- `crates/orchest/src/run/mod.rs`：`resume` / `resume_with_input` 走日志恢复
- `crates/orchest/src/run/config.rs`：checkpoint 策略配置项

## Steps

1. 在 `SessionEventKind` 上补落点相关事件（`Checkpoint`、step 起止、终止原因），并给事件加 `seq` 连续性测试。
2. 定义 `CheckpointPolicy`（`every_step` / `every_tool` / `manual`）与默认值，写进会话元数据。
3. 在 actor 的四个落点写事件 + 按策略 `flush`；工具派发前的标记在 `tool_exec` 的派发路径上。
4. 实现 `flush` 的后端语义：SQLite 走事务提交，内存实现为 no-op 但满足契约；trait 文档写清"append 只保证顺序接受"。
5. 改造 `resume` / `resume_with_input`：读日志 → 投影 → 重建；未闭合 step 记为中断；恢复后首次写入前先 flush。
6. 明确配置归属：恢复时模型、工具注册、hooks、retry、handoffs、store 全部由宿主提供；运行时不从日志恢复这些。
7. 写测试：step 中途崩溃、工具派发前崩溃、正常结束后 resume 三种场景的投影一致性。
8. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`。
