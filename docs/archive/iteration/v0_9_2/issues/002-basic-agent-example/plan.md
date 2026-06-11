# 002 · basic_agent_run 示例 — 实现计划

## 要读的现有代码

- `examples/rust/hook_logging.rs` — 整体结构（config → start → event loop → wait），去掉 hook
- `examples/rust/approval_mode_side_effect.rs:110-181` — 自定义 `Tool` trait impl 的最短写法
- `examples/rust/provider_runtime_deepseek.rs` — `create_adapter_from_config` + `ProviderRuntimeConfig` 用法
- `crates/agent-runtime-core/src/tool/mod.rs:28-38` — `Tool` trait 方法签名
- `crates/agent-runtime-core/src/tool/builtin.rs` — `input_schema()` 返回 `&JsonSchema` 的持有方式（静态 schema）
- `crates/agent-runtime-core/src/run/mod.rs:38-52` — `AgentRun::start` 签名与返回
- `crates/agent-runtime-core/Cargo.toml:41-104` — `[[example]]` 注册格式

## 步骤

### 1. 写示例骨架

`examples/rust/basic_agent_run.rs`：

```rust
//! Basic agent run — the smallest possible Orchest agent.
//!
//! Configures an Anthropic provider, registers one tool, starts a run,
//! and prints streamed output. Run with:
//!   ANTHROPIC_API_KEY=sk-... cargo run --example basic_agent_run

use std::sync::{Arc, OnceLock};

use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use agent_runtime_core::tool::registry::ToolRegistry;
use agent_runtime_providers::{create_adapter_from_config, ProviderRuntimeConfig};
use async_trait::async_trait;
use serde_json::json;
```

### 2. 定义最简 tool

无副作用的 `get_current_time`：

```rust
struct CurrentTimeTool;

fn empty_object_schema() -> &'static JsonSchema {
    static SCHEMA: OnceLock<JsonSchema> = OnceLock::new();
    SCHEMA.get_or_init(|| json!({"type": "object", "properties": {}}))
}

fn time_tool_metadata() -> &'static ToolMetadata {
    static META: OnceLock<ToolMetadata> = OnceLock::new();
    META.get_or_init(|| ToolMetadata {
        side_effect: false,
        approval: Approval::Never,
        cost_hint: None,
        timeout: None,
        max_output_tokens: None,
        source: ToolSource::InProcess,
    })
}

#[async_trait]
impl Tool for CurrentTimeTool {
    fn name(&self) -> &str { "get_current_time" }
    fn description(&self) -> &str { "Return the current UTC time as an ISO-8601 string." }
    fn input_schema(&self) -> &JsonSchema { empty_object_schema() }
    fn output_schema(&self) -> Option<&JsonSchema> { None }
    fn metadata(&self) -> &ToolMetadata { time_tool_metadata() }
    async fn execute(&self, _input: serde_json::Value, _ctx: &ToolContext)
        -> Result<ToolOutput, ToolError>
    {
        // 真实实现可用 chrono/time；起步示例返回固定串即可
        Ok(ToolOutput::Immediate(json!({ "utc": "2026-06-06T00:00:00Z" })))
    }
}
```

> 确认 `Approval` / `ToolMetadata` / `ToolSource` / `JsonSchema` 的 re-export 路径（可能是 `agent_runtime_core::tool::{...}`）。若 `JsonSchema` 未从 `tool` 顶层导出，改用 `serde_json::Value`。

### 3. main：5 步串起来

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. provider
    let model = create_adapter_from_config(ProviderRuntimeConfig {
        model: "anthropic/claude-sonnet-4-6".into(),
        api_key: None,
        api_key_env: Some("ANTHROPIC_API_KEY".into()),
        api_url: None,
        max_tokens: Some(1024),
    })?;
    let model: Arc<dyn _> = Arc::from(model);

    // 2. tool registry
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CurrentTimeTool))?;

    // 3. config
    let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
        .system_prompt("You are a helpful assistant. Use tools when useful.")
        .max_steps(5)
        .build()?;

    // 4. start
    let (handle, mut rx) = AgentRun::start(config, "What time is it?".into(), model, registry);

    // 5. events + wait
    while let Some(event) = rx.recv().await {
        match event {
            RuntimeEvent::ModelStreamChunk { delta } => { /* print text delta */ }
            RuntimeEvent::ToolCallStarted { tool, .. } => println!("[tool] {tool}"),
            RuntimeEvent::ToolCallCompleted { tool, output, .. } => println!("[tool done] {tool}: {output}"),
            RuntimeEvent::RunCompleted { output } => println!("[done] {output}"),
            RuntimeEvent::RunFailed { error } => eprintln!("[failed] {error}"),
            _ => {}
        }
    }
    handle.wait().await;
    Ok(())
}
```

> `ModelStreamChunk { delta }` 的 `delta` 是 `StreamEvent`；打印文本 delta 的具体匹配参考 `examples/python/basic.py` 中 `delta.Text.delta` 的结构，在 Rust 侧对应 `StreamEvent` 的 `Text` 变体。具体形状实现时对照 `model/streaming.rs`。

### 4. 注册 example

在 `crates/agent-runtime-core/Cargo.toml` 末尾 `[[example]]` 区块追加：

```toml
[[example]]
name = "basic_agent_run"
path = "../../examples/rust/basic_agent_run.rs"
```

### 5. 验证

```bash
cargo build --example basic_agent_run
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## 关键决策

- **用真实 Anthropic provider 而非 MockModel**：起步示例要让用户看到真实接法。代价是不能在 CI 真跑（无 key），但 `cargo build --example` 能保证编译链接正确，足够作为回归防线。
- **tool 用裸 `Tool` impl 而非 `InProcessTool`**：裸 impl 对初学者更直观地展示 trait 的 6 个方法；`InProcessTool` 的闭包 + `ToolCallback` 类型签名反而更绕。
- **静态 schema/metadata 用 `OnceLock`**：`input_schema()`/`metadata()` 返回引用，需要 `'static` 持有，`OnceLock` 是无 unsafe 的标准做法。
