# 005 实现路线

## 步骤

1. **理解现有实现**
   - 读 `crates/agent-runtime-core/src/tool/mcp.rs` 的 `McpStdioClient` 和 `McpStdioInner`
   - 重点看 `connect()`、`list_tools()`、工具 `execute()` 的调用路径
   - 找到所有调用 `self.inner.lock().await` 的地方

2. **替换 `McpStdioClient` 结构体**

   用新结构替换旧结构（旧的 `McpStdioInner` 一并删除）：

   ```rust
   pub struct McpStdioClient {
       stdin: Arc<Mutex<ChildStdin>>,
       pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
       next_id: Arc<std::sync::atomic::AtomicU64>,
       child: Arc<Mutex<Child>>,
       reader_abort: tokio::task::AbortHandle,
   }
   ```

3. **重写 `connect()` 方法**
   - spawn 子进程，分离 stdin/stdout
   - spawn reader task：用 `BufReader::new(stdout).lines()` 循环读，按 `id` 分发到 `pending` map
   - 执行 MCP initialize 握手（现有代码可以复用握手逻辑，只是改成通过新的 `send_request` 发送）
   - 返回新结构

4. **实现 `send_request()` 私有方法**
   - 先把 sender 注册到 `pending`，再写 stdin——避免响应比写入更快的竞争
   - stdin 写操作持锁仅在写期间（不 hold 锁等响应）
   - `rx.await` 等待 reader task 分发响应

5. **用 `send_request()` 重写 `list_tools()` 和工具调用**
   - 把所有 `self.inner.lock().await` 调用替换为 `self.send_request(method, params).await`
   - MCP 工具执行（`McpTool::execute`）底层的 transport 调用同步更新

6. **更新 `Drop` 实现**
   - `self.reader_abort.abort()`
   - `if let Ok(mut child) = self.child.try_lock() { let _ = child.start_kill(); }`

7. **添加测试**
   - 在文件底部 `#[cfg(test)]` 中添加 spec 中的 `pending_map_routes_responses_by_id` 测试
   - 这个测试不需要真实进程，只测试 pending map 的路由逻辑

8. **验收**
   - `grep -n "McpStdioInner" crates/agent-runtime-core/src/tool/mcp.rs` — 无输出
   - `cargo test --workspace` 全绿
   - `cargo clippy --workspace -- -D warnings` 全绿

## 要读的现有代码

- `crates/agent-runtime-core/src/tool/mcp.rs` — 完整文件，重点 `McpStdioClient::connect` 和 `McpStdioInner` 结构
- `crates/agent-runtime-core/src/tool/mcp.rs` 中的 `McpTool::execute` — 了解工具执行如何调用 transport

## 关键决策

- **reader task 崩溃处理**：如果子进程意外退出，reader task 的 `lines()` 会返回 `Ok(None)` 退出循环。此时 pending map 中所有 sender 会被 drop，对应的 receiver 会收到 `Err(RecvError)`。在 `send_request` 中，`rx.await.map_err(|_| McpError { message: "MCP server disconnected", ... })` 可以正确捕获这种情况
- **stdin flush**：写完 JSON-RPC 请求后必须 flush，否则数据可能在 OS buffer 里而子进程收不到。`AsyncWriteExt::flush()` 要在 unlock 前调用（flush 也在 `stdin.lock().await` 持锁期间）
- **AtomicU64 vs 锁保护的 u64**：用 `AtomicU64` 做 ID 生成器，避免为了取 next_id 而加锁。`Relaxed` ordering 足够（ID 只需要唯一，不需要 happens-before 保证）
- **`McpHttpClient` 不需要改**：HTTP 请求天然并发，每个 `reqwest` 请求独立，不存在串行锁问题
