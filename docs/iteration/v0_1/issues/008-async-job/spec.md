# 008 · 异步 Job 轮询

## 背景

当 tool 返回 `ToolOutput::AsyncJob` 时，runtime 需要在 background task 里按 `poll_interval` 轮询，定期发出进度事件，完成后把结果注入消息流并恢复 loop。

## 目标

实现 `poll_until_done()`：接管 `JobHandle`，轮询至完成或超时，全程发出事件。

## 验收标准

- [ ] `poll_until_done(handle: JobHandle, event_tx: ...) -> Result<Value, ToolError>` 实现
- [ ] 调用后立即发出 `AsyncToolStarted { tool, job_id }`
- [ ] 每次 `poll()` 后：
  - `Pending` → 发出 `AsyncToolProgress { tool, job_id, status }`，等待 `poll_interval` 后再次轮询
  - `Completed(value)` → 发出 `AsyncToolCompleted { tool, job_id, output, elapsed }`，返回 `Ok(value)`
  - `Failed(msg)` → 发出 `ToolCallFailed { tool, error: msg }`，返回 `Err`
- [ ] `JobHandle.timeout` 超时时，停止轮询，返回 `Err("async_job_timeout")`
- [ ] `BudgetConfig.max_duration` 超时时，同样停止轮询（由 budget guard 外部触发取消）
- [ ] 轮询在 background tokio task 中运行，不阻塞 loop task（通过 channel 返回结果）
- [ ] `RunStatus` 在轮询期间为 `WaitingForAsyncTool { tool_call, job_handle, since }`

## 说明

`JobHandle.poll` 是 `Arc<dyn Fn() -> BoxFuture<...>>`，每次轮询就调用一次。Poll 之间的等待用 `tokio::time::sleep(handle.poll_interval)` 实现。
