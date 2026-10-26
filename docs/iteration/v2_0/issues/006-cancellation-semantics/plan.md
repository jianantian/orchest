# 006 implementation plan

## Files to read

- `crates/orchest/src/run/{actor.rs,handle.rs,tool_exec.rs,config.rs}`（取消消息、审批等待、工具派发）
- `crates/orchest/src/model/mod.rs` 与 `crates/orchest-protocol/src/{adapter.rs,stream.rs}`（`ChatModel::complete` 的流式接口）
- `crates/orchest/src/tool/mod.rs`（`ToolContext`、`ToolOutput`）
- `crates/orchest/src/tool/async_job.rs`（长任务轮询与 webhook）
- `crates/orchest/src/session/log.rs`（取消的记账与终止事件）
- `crates/orchest/src/run/supervisor.rs`（重启与取消的交互）

## Files to change

- 新增 `crates/orchest/src/run/cancel.rs`：`CancellationToken`、取消原因枚举
- `crates/orchest-protocol/src/adapter.rs`：`ChatModel::complete` 接受取消令牌（破坏性变更）
- `crates/orchest/src/run/actor.rs`：取消贯穿 step、审批等待、工具派发门
- `crates/orchest/src/run/handle.rs`：`abort` 幂等化，暴露取消原因
- `crates/orchest/src/tool/mod.rs`：`ToolContext` 增加取消令牌；`ToolMetadata` 的可中断性标记
- `crates/orchest/src/tool/async_job.rs`：长任务在取消时停止轮询并如实上报
- 新增 `crates/orchest/src/run/process.rs`：进程组终止后端注入点
- `crates/orchest/src/events.rs`：终止事件带原因
- bindings 与示例：同步公开面

## Steps

1. 实现 `CancellationToken`（可克隆、幂等、带原因），定义取消原因枚举。
2. `ChatModel::complete` 增加取消令牌参数（协议 trait 破坏性变更），provider 实现在流读取处检查并提前返回。
3. actor：step 开始前检查取消；审批等待改为"取消或回复"二选一，取消立即按未批准处理；工具派发前检查取消，未派发记 aborted。
4. `ToolMetadata` 增加可中断性标记（只读默认 true、有副作用默认 false）；`ToolContext` 携带取消令牌，工具自行决定是否响应。
5. 不可中断工具在取消后跑完并正常落账；取消后不再发起新的工具调用。
6. 终止事件带原因；`abort` 幂等；取消与正常结束的竞争写确定性测试。
7. 进程组终止后端注入点：`ProcessTerminator` trait + 默认空实现；取消时尽力终止并把失败如实上报。
8. 同步 bindings 与示例；跑测试、clippy、fmt；在 `CHANGELOG.md` 记破坏性变更。
