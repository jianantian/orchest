# 007 · MCP 子进程泄漏与 Node Event Dropping

## 背景

两个 Important 级别的资源管理和通知问题，分别在 MCP 客户端和 Node 绑定中。

## 7a. `McpStdioClient::Drop` 子进程泄漏（Important）

**文件**：`crates/agent-runtime-core/src/tool/mcp.rs:66-75`

```rust
impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.reader_abort.abort();
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        }
    }
}
```

`try_lock()` 失败时（reader task 正在持有 Mutex），child 不会被 kill，进程泄漏为僵尸。

当前代码已经先 abort reader task（`self.reader_abort.abort()`），但 abort 是异步的——task 可能还没释放 Mutex 就执行了 `try_lock()`。

**修复**：保持当前 Drop 结构，在 `try_lock` 失败时 spawn 一个 OS 线程等待 lock 释放后 kill。`abort` 是异步的，reader task 可能尚未释放 Mutex，但 abort 传播后 task 会被 drop，Mutex 随之释放。OS 线程不依赖 tokio runtime，在 Drop 中安全使用。

```rust
impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.reader_abort.abort();
        let child = Arc::clone(&self.child);
        if let Ok(mut guard) = self.child.try_lock() {
            let _ = guard.start_kill();
        } else {
            std::thread::spawn(move || {
                if let Ok(mut guard) = child.lock() {
                    let _ = guard.start_kill();
                }
            });
        }
    }
}
```

## 7b. Node binding event dropping 无通知（Important）

**文件**：`crates/agent-runtime-node/src/lib.rs`

Node binding 使用 `ThreadsafeFunctionCallMode::NonBlocking` 转发事件到 JS callback。`NonBlocking` 模式下如果 V8 event queue 满，call 被静默跳过。消费者无感知。

**修复**：在 event forwarding 循环中，检测 `NonBlocking` 的返回值（`napi::Status`），失败时通过 Rust 侧的 event channel 发送 `EventsDropped`：

```rust
let status = tsfn.call(event.clone(), ThreadsafeFunctionCallMode::NonBlocking);
if status != napi::Status::Ok {
    // JS 侧无法消费，通过 primary event channel 通知
    if let Some(primary_tx) = &primary_event_tx {
        let _ = primary_tx.try_send(RuntimeEvent::EventsDropped {
            subscriber_id: node_subscriber_id,
            count: 1,
            run_depth: event.run_depth(),
        });
    }
}
```

需要 Node binding 持有 primary event channel 的引用。如果架构上不方便，最小方案是 log warning（`tracing::warn!`）。

## 验收标准

- [ ] `McpStdioClient::Drop` 在 `try_lock` 失败时仍能 kill 子进程（通过后台线程）
- [ ] Node binding event forwarding 失败时有通知机制（`EventsDropped` 或 log warning）
- [ ] 无新增 zombie 进程风险
- [ ] `cargo test -p agent-runtime-core` 全绿
- [ ] `cargo build -p agent-runtime-node` 成功
