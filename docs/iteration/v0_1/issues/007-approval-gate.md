# 007 · Approval Gate

## 背景

某些 tool（如写文件、执行命令）在调用前需要人工确认。Run loop 需要在这些调用前暂停，等待外部响应，然后根据结果继续或中止。

## 目标

实现 approval gate：run loop 遇到 `requires_approval: true` 的 tool 时暂停，通过 `RunHandle.respond_approval()` 恢复。

## 验收标准

- [ ] 遇到 `requires_approval: true` 的 tool 时：
  - 发出 `ApprovalRequested { tool_call }` 事件
  - `RunStatus` 转为 `WaitingForApproval { tool_call }`
  - Loop 阻塞，等待 oneshot channel 信号
- [ ] `RunHandle.respond_approval(run_id: RunId, approved: bool)` 唤醒 loop
- [ ] `approved: true` → 发出 `ApprovalGranted`，继续执行该 tool
- [ ] `approved: false` → 发出 `ApprovalDenied`，tool result 为拒绝消息，loop 继续下一步
- [ ] E2E 覆盖 `approved: false` 路径：被拒绝的 tool 不发出 `ToolCallStarted` / `ToolCallCompleted`，模型收到拒绝 tool result 后继续下一步
- [ ] `approved: false` 的最终 run 语义明确断言：除非后续模型调用失败或预算耗尽，run 应以 `RunCompleted` 收尾，而不是因为拒绝本身进入 `RunFailed`
- [ ] `max_duration` 超时时，waiting for approval 状态也会被 budget guard 终止
- [ ] `RunHandle` 可以跨线程安全传递（`Send + Sync`）

## 说明

Approval gate 只针对单个 tool call，不影响 loop 的其他 tool。被拒绝的 tool 不会重试，模型会根据拒绝消息自行决定下一步。
