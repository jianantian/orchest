# 003 · 一行级结构修复 — 实施计划

## 依赖

无前置依赖，6 个修复彼此独立，可任意顺序。

## 步骤

### ~~Step 1: A14 — SkillEnvManager derive Clone~~

**已完成，skip。** `mod.rs:139` 已有 `#[derive(Debug, Clone)]`。

### Step 2: A12 — ToolContext derive Clone

**文件**：`crates/agent-runtime-core/src/tool/mod.rs`（`struct ToolContext`）

1. 确认 `ToolContext` 所有字段是否 impl Clone：
   - `run_id: RunId` — 需确认 RunId 是否 Clone（是 Copy）
   - `on_update: Option<mpsc::Sender<Value>>` — `mpsc::Sender` 是 Clone
   - `event_tx: Option<mpsc::Sender<RuntimeEvent>>` — 同上
   - 其他字段逐一确认
2. 添加 `#[derive(Clone)]`（如果不行就 `Debug, Clone`）
3. 找到 `tool/in_process.rs` 中逐字段复制的代码，改为 `.clone()`

### Step 3: A10 — Event channel 容量常量化

**文件**：`crates/agent-runtime-core/src/run/mod.rs:44`

1. 在 `mod.rs` 或 `config.rs` 添加：`const EVENT_CHANNEL_CAPACITY: usize = 256;`
2. `mpsc::channel(256)` → `mpsc::channel(EVENT_CHANNEL_CAPACITY)`

**1 行改动。**

### Step 4: A8 — record_model_success 参数结构体

**文件**：`crates/agent-runtime-providers/src/telemetry.rs:28`

1. 定义 `ModelSuccessRecord<'a>` 结构体
2. 将 `record_model_success` 签名改为接受 `&ModelSuccessRecord`
3. 更新 4 个调用点：
   - `anthropic.rs`（grep `record_model_success`）
   - `openai.rs`
   - `deepseek.rs`
   - `openrouter.rs`
4. 删除 `tool/agent.rs:26` 的 `#[allow(clippy::too_many_arguments)]`
5. `loop_.rs:51` 的 `#[allow(clippy::too_many_arguments)]` 需要同步处理——`run_loop_inner` 当前 7 个参数，004 加 `cancel_token` 后变成 8 个。应将参数收束为 `RunContext` 结构体：
   ```rust
   struct RunContext {
       run_id: RunId,
       config: AgentConfig,
       model: Arc<dyn ModelAdapter>,
       registry: ToolRegistry,
       tx: mpsc::Sender<RuntimeEvent>,
       approval_bus: ApprovalBus,
       cancel_token: CancellationToken,  // 004 新增
   }
   ```
   注意：此步骤与 004 有交叉，建议 004 完成后一起处理。`input: String` 不入 struct（是一次性消费的）。
6. 注意：`agent-runtime-py/src/lib.rs:526` 也有此 allow，但那是 PyO3 `Agent::new` 的 9 个参数（Python API 设计），不在 A8 范围内。需要加 `// justified: PyO3 constructor mirrors Python API`

### Step 5: A15 — MCP HTTP JSON-RPC ID 递增

**文件**：`crates/agent-runtime-core/src/tool/mcp.rs:350`（grep `"id": 1`）

1. 在文件顶部或 MCP HTTP client 区域添加：
   ```rust
   static HTTP_REQUEST_ID: AtomicU64 = AtomicU64::new(1);
   ```
2. 替换 `"id": 1` 为 `"id": HTTP_REQUEST_ID.fetch_add(1, Ordering::Relaxed)`
3. 确认 `use std::sync::atomic::{AtomicU64, Ordering}` 已导入

### Step 6: R2 — Node SDK 共享 Tokio Runtime

**文件**：`crates/agent-runtime-node/src/lib.rs:325, 387, 422`

1. 添加 `shared_runtime()` 函数：
   ```rust
   fn shared_runtime() -> &'static Runtime {
       static RT: OnceLock<Runtime> = OnceLock::new();
       RT.get_or_init(|| Runtime::new().expect("failed to create tokio runtime"))
   }
   ```
2. grep `Runtime::new()` 找到所有 3 处，改为 `shared_runtime().block_on(...)` 或 `shared_runtime().spawn(...)`
3. 确认 `run_stream` 的场景是否需要 `spawn` 而非 `block_on`

## 验证

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
grep -rn 'allow(clippy::too_many_arguments)' crates/ --include='*.rs' | grep -v target  # 应为空
```
