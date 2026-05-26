# 003 · 一行级结构修复

## 背景

Review 发现多个一行或几行即可修复的结构问题。集中在一个 issue 里批量处理。

## A8. `record_model_success()` 参数爆炸

**文件**：`crates/agent-runtime-providers/src/telemetry.rs:28`

7 个位置参数，4 个调用点（`anthropic.rs:700`、`openai.rs:456`、`deepseek.rs:462`、`openrouter.rs:486`）。

**修复**：提取参数结构体：

```rust
pub struct ModelSuccessRecord<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub duration: Duration,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub first_token_latency: Option<Duration>,
    pub stream_duration: Option<Duration>,
}

pub fn record_model_success(record: &ModelSuccessRecord) { ... }
```

修复后删除 `crates/agent-runtime-core/src/run/loop_.rs:51` 和 `tool/agent.rs:26` 的 `#[allow(clippy::too_many_arguments)]`。

## A10. Event Channel 容量硬编码 256

**文件**：`crates/agent-runtime-core/src/run/mod.rs:44`

```rust
let (event_tx, event_rx) = mpsc::channel(256);
```

**修复**：提取为常量并提高到合理值：

```rust
const EVENT_CHANNEL_CAPACITY: usize = 256;
let (event_tx, event_rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
```

本轮仅将硬编码提取为常量，值保持 256 不变。调整值需要 profiling 依据（如一次 streaming response 产生的事件峰值数量）。提取为常量后未来改值只需一处。

## A12. `ToolContext` 手动逐字段复制

**文件**：`tool/in_process.rs`（`InProcessTool::execute()`）

逐字段 clone `ToolContext`，新增字段容易遗漏。

**修复**：为 `ToolContext` 添加 `#[derive(Clone)]`，调用点改为 `.clone()`。

注意：`ToolContext` 含 `Option<mpsc::Sender<RuntimeEvent>>`，`mpsc::Sender` 是 `Clone` 的，所以 derive 可行。确认所有字段都实现 `Clone`。

## A14. `SkillEnvManager` 缺少显式 `Clone` derive

**文件**：`skill/mod.rs:129`

`SkillBundledTool::clone_for_poll()` 调用 `self.env_manager.clone()`，但 `SkillEnvManager` 不 derive `Clone`。当前因唯一字段 `PathBuf` 是 `Clone` 的而编译通过。加任何非 `Clone` 字段会直接 break。

**修复**：`#[derive(Clone)]` 加到 `SkillEnvManager`。

## A15. MCP HTTP Client JSON-RPC ID 硬编码为 `1`

**文件**：`tool/mcp.rs:350`

```rust
let body = json!({"jsonrpc": "2.0", "id": 1, ...});
```

Stdio client 用 `AtomicU64` 递增。HTTP client 硬编码 `1`，并发化后响应路由错乱。

**修复**：HTTP client 也用 `AtomicU64` 递增 ID，与 Stdio client 对齐。

```rust
static HTTP_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
// ...
let id = HTTP_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
let body = json!({"jsonrpc": "2.0", "id": id, ...});
```

## R2. Node SDK 每次调用创建新 Tokio Runtime

**文件**：`crates/agent-runtime-node/src/lib.rs:325, 387, 422`

`run_sync`、`run_stream`、`respond_approval` 各自 `Runtime::new()`。

**修复**：

```rust
use std::sync::OnceLock;
use tokio::runtime::Runtime;

fn shared_runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Runtime::new().expect("failed to create tokio runtime")
    })
}
```

三个函数改用 `shared_runtime().block_on(...)` 或 `shared_runtime().spawn(...)`。

## 验收标准

- [ ] A8：`record_model_success` 接受结构体参数，4 个调用点同步更新
- [ ] A8：`crates/` 中无 `#[allow(clippy::too_many_arguments)]` 注解
- [ ] A10：event channel 容量提取为 `const EVENT_CHANNEL_CAPACITY`
- [ ] A12：`ToolContext` 派生 `Clone`，无逐字段手动复制
- [ ] A14：`SkillEnvManager` 派生 `Clone`
- [ ] A15：MCP HTTP client JSON-RPC ID 使用 `AtomicU64` 递增
- [ ] R2：Node SDK 使用共享 Tokio Runtime
- [ ] `cargo test --workspace` 全绿
