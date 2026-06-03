# 007 · LlmWatcher — 实现计划

## 步骤

### 1. 定义 LlmWatcher 和 builder
文件：新建 `crates/agent-runtime-core/src/run/llm_watcher.rs`

```rust
pub struct LlmWatcher {
    model: Arc<dyn ModelAdapter>,
    system_prompt: String,
    eval_interval: usize,
    event_buffer: Mutex<Vec<String>>,  // 格式化后的事件摘要
    event_count: AtomicUsize,
}

pub struct LlmWatcherBuilder { .. }

impl LlmWatcherBuilder {
    pub fn model(self, m: Arc<dyn ModelAdapter>) -> Self;
    pub fn system_prompt(self, s: impl Into<String>) -> Self;
    pub fn eval_interval(self, n: usize) -> Self;
    pub fn build(self) -> LlmWatcher;
}
```

### 2. 事件格式化
文件：同上
- `fn format_event(event: &RuntimeEvent) -> String` — 将事件转为 LLM 可读摘要
- `ToolCallStarted` → `"Tool called: {tool} (approval: {metadata.approval}, side_effect: {metadata.side_effect}), input: {input_summary}"`
- `ToolCallCompleted` → `"Tool completed: {tool}, duration: {duration}ms"`
- `ToolCallFailed` → `"Tool failed: {tool}, kind: {error.kind}, message: {error.message}"`
- 其他事件 → 简短的 Debug 格式

### 3. 实现 Watcher trait
文件：同上
- `on_event`：
  1. 格式化事件 → push 到 buffer
  2. 递增 `event_count`
  3. 如果 `event_count % eval_interval == 0` → 调用 `evaluate()`
  4. 否则返回 `Continue`
- `evaluate()`：
  1. drain buffer 到 context
  2. 构造 messages：system prompt + 事件摘要
  3. 定义 tool：`decide_action(action: "continue"|"inject"|"steer"|"abort", message: String, reason: String)`
  4. 调用 `model.chat(messages, tools)` → 解析 tool_use response
  5. 映射到 `WatcherAction`
  6. 如果 model 调用失败 → log warning → 返回 `Continue`

### 4. 导出
文件：`crates/agent-runtime-core/src/run/mod.rs` + `src/lib.rs`
- `pub mod llm_watcher;`
- re-export `LlmWatcher` 和 `LlmWatcherBuilder`

### 5. 测试
- mock ModelAdapter：返回预设的 tool_use response → 验证映射
- 测试事件累积：发送 N-1 个事件 → 确认没有 LLM 调用；发送第 N 个 → 确认调用
- 测试 LLM 失败降级：mock 返回错误 → 确认返回 Continue
- 集成测试：用真实（或 mock）model adapter，worker + LlmWatcher 端到端

### 6. 验证
```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```
