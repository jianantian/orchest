# 004 implementation plan

## Files to read

- `crates/orchest/src/run/handle.rs`（`ApprovalBus`、`RunHandle::respond_approval`）
- `crates/orchest/src/run/actor.rs`（审批等待点、`APPROVAL_TIMEOUT`、拒绝如何变成工具结果）
- `crates/orchest/src/tool/mod.rs`（`ToolContext.approval_bus`、子 run 转发）
- `crates/orchest/src/tool/agent_as_tool.rs`（子 run 的审批转发路径）
- `crates/orchest/src/events.rs`（`ApprovalRequested` / `ApprovalGranted` / `ApprovalDenied`）
- `crates/orchest/src/session/log.rs`（请求生命周期要落日志）

## Files to change

- 新增 `crates/orchest/src/run/request.rs`：`RequestId`、`RequestKind`、`PendingRequest`、`RequestBus`、`RequestDecision`
- `crates/orchest/src/run/handle.rs`：`RunHandle` 的请求相关方法（列出、按 id 回复、撤回）
- `crates/orchest/src/run/actor.rs`：审批等待改为走新通道；超时可配且 fail-closed
- `crates/orchest/src/tool/mod.rs`：`ToolContext` 的 bus 字段换成新通道
- `crates/orchest/src/tool/agent_as_tool.rs`：子 run 请求转发
- `crates/orchest/src/events.rs`：事件变体补 request id / kind / payload，新增"请求已解决"
- `crates/orchest/src/bindings.rs`、`orchest-py`、`orchest-node`：公开面同步

## Steps

1. 定义 `RequestId`（单调、run 内唯一、可跨重连定位）、`RequestKind`（宿主可扩展的标签）、`PendingRequest`（id、kind、payload、阻塞对象）、`RequestDecision`（同意/拒绝/带内容回答 + 可选会话级规则载荷）。
2. 实现 `RequestBus`：按 request id 存 responder；支持列出、按 id 回复、撤回；回复后清理。
3. 把 actor 的审批等待改为走新通道：请求发出 → 挂起 → 收到决定 → 生成工具结果（拒绝 → `APPROVAL_DENIED`）；超时可配（run 级），超时按未批准处理并报稳定错误码。
4. 子 run 的请求经 `ChildRunRegistry` 路由到父，父按 id 回复。
5. 事件面同步：请求发出/解决事件带 id、kind、payload；保持 `#[non_exhaustive]`。
6. 公开面硬切换：删除 `respond_approval(run_id, approved)`，改为按 request id 的接口；同步 bindings 与示例。
7. 请求生命周期落日志（进入/解决/撤回/超时）。
8. 测试：并发多请求、按 id 回复、撤回、超时 fail-closed、子 run 路由。
9. 跑 `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --check`、`scripts/check-ts-event-variants.sh`。
