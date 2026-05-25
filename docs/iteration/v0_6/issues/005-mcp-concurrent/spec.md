# 005 · McpStdioClient 并发化

## 背景

当前 `McpStdioClient` 持有一个全局 `Mutex<McpStdioInner>`：

```rust
pub struct McpStdioClient {
    inner: Mutex<McpStdioInner>,
}

struct McpStdioInner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}
```

每次 `tool.execute()` 调用都要持锁完成整个"写请求 → 读响应"序列。这意味着同一个 MCP server 上的工具调用是串行的。

v0.2 已启用并行 tool call，但 MCP 工具受此锁约束，并不能真正并发执行。

## 新设计

拆分读写责任：

- **写**：`Arc<Mutex<ChildStdin>>`，只锁住 stdin 写操作（微秒级），多个请求可快速依次写入
- **读**：独立的 reader task，持续读 stdout，按 JSON-RPC `id` 将响应分发到对应的 `oneshot::Sender`
- **请求追踪**：`Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>` 存储 pending 请求

```rust
pub struct McpStdioClient {
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    child: Arc<Mutex<Child>>,           // 仅 Drop 时 kill
    reader_abort: tokio::task::AbortHandle,
}
```

## 实现

文件：`crates/agent-runtime-core/src/tool/mcp.rs`

### 连接建立

```rust
impl McpStdioClient {
    pub async fn connect(command: &str, args: &[&str]) -> Result<Self, McpError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| McpError { message: format!("failed to spawn {command}: {e}"), code: None })?;

        let stdin = Arc::new(Mutex::new(child.stdin.take().unwrap()));
        let stdout = child.stdout.take().unwrap();
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
        let pending_for_reader = Arc::clone(&pending);

        let reader_handle = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<Value>(&line) {
                    if let Some(id) = resp.get("id").and_then(Value::as_u64) {
                        if let Some(tx) = pending_for_reader.lock().await.remove(&id) {
                            let _ = tx.send(resp);
                        }
                    }
                }
            }
            // Reader exiting means process died; drop remaining senders so waiters get Err
        });

        let client = Self {
            stdin,
            pending,
            next_id: Default::default(),
            child: Arc::new(Mutex::new(child)),
            reader_abort: reader_handle.abort_handle(),
        };

        // MCP initialize handshake
        client.initialize().await?;
        Ok(client)
    }
}
```

### 并发请求发送

```rust
impl McpStdioClient {
    async fn send_request(&self, method: &str, params: Value) -> Result<Value, McpError> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();

        // Register before sending to avoid race with a very fast response
        self.pending.lock().await.insert(id, tx);

        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let mut line = serde_json::to_string(&msg).unwrap();
        line.push('\n');

        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await.map_err(|e| McpError {
                message: format!("failed to write to MCP server stdin: {e}"),
                code: None,
            })?;
            stdin.flush().await.map_err(|e| McpError {
                message: format!("failed to flush MCP server stdin: {e}"),
                code: None,
            })?;
        }

        rx.await.map_err(|_| McpError {
            message: "MCP server disconnected before responding".into(),
            code: None,
        })
    }
}
```

### Drop 实现

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

## McpHttpClient

`McpHttpClient` 使用 `reqwest` HTTP 请求，天然无串行问题，不需要修改。

## 验收标准

### 结构

- [ ] `McpStdioClient` 不再包含 `Mutex<McpStdioInner>` 字段（grep 验证 `McpStdioInner` 不存在）
- [ ] `McpStdioClient` 包含 `stdin: Arc<Mutex<ChildStdin>>`、`pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>`、`reader_abort: tokio::task::AbortHandle` 三个核心字段
- [ ] reader task 在 `connect()` 时启动，在 `Drop` 时 abort

### 功能

- [ ] `connect()` 完成 MCP initialize 握手
- [ ] `list_tools()` 正常工作
- [ ] 工具 `execute()` 通过 `send_request("tools/call", ...)` 发送请求并等待对应 ID 的响应

### 并发测试

- [ ] 内嵌测试验证两个并发 `send_request` 能正确各自收到响应（使用 mock stdin/stdout 或 echo server）：

  ```rust
  #[tokio::test]
  async fn pending_map_routes_responses_by_id() {
      // 直接测试 pending map 的路由逻辑，无需真实进程
      let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Default::default();
      let (tx1, rx1) = oneshot::channel::<Value>();
      let (tx2, rx2) = oneshot::channel::<Value>();
      pending.lock().await.insert(1, tx1);
      pending.lock().await.insert(2, tx2);

      // 模拟 reader 按 id 分发
      let resp1 = json!({"jsonrpc": "2.0", "id": 1, "result": "a"});
      let resp2 = json!({"jsonrpc": "2.0", "id": 2, "result": "b"});
      if let Some(tx) = pending.lock().await.remove(&resp2["id"].as_u64().unwrap()) {
          let _ = tx.send(resp2);
      }
      if let Some(tx) = pending.lock().await.remove(&resp1["id"].as_u64().unwrap()) {
          let _ = tx.send(resp1);
      }

      // 乱序响应，各自 receiver 仍能正确收到
      assert_eq!(rx1.await.unwrap()["result"], "a");
      assert_eq!(rx2.await.unwrap()["result"], "b");
  }
  ```

### 正确性

- [ ] 现有使用 `McpStdioClient` 的集成测试（如果有）继续通过
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
