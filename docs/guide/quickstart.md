# Quickstart：从零到一个可运行的 agent

本教程带你用 Rust 从零跑通一个最小 Orchest agent：配置 provider、注册一个 tool、启动 run、监听事件流。全部代码与 [`examples/rust/basic_agent_run.rs`](../../examples/rust/basic_agent_run.rs) 一致，可直接 `cargo run --example basic_agent_run` 运行。

> Python / TypeScript 用户请看 [SDK · Python](./sdk-python.md) 和 [SDK · TypeScript](./sdk-typescript.md)。

## 1. 前置条件

- Rust toolchain（stable）
- 一个 provider 的 API key。本教程以 Anthropic 为例，读取环境变量 `ANTHROPIC_API_KEY`：

  ```bash
  export ANTHROPIC_API_KEY=sk-...
  ```

## 2. 添加依赖

在你的 `Cargo.toml`：

```toml
[dependencies]
agent-runtime-core = { git = "https://github.com/jianantian/orchest" }
agent-runtime-providers = { git = "https://github.com/jianantian/orchest" }
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
serde_json = "1"
```

> Orchest 尚未发布到 crates.io（计划于 v1.0）。在那之前用 git 依赖；发布后改为 `agent-runtime-core = "x.y"`。

## 3. 配置 provider

`create_adapter_from_config` 接收一个 `ProviderRuntimeConfig`，返回一个 `Box<dyn ModelAdapter>`，用 `Arc::from` 转成 run loop 需要的 `Arc<dyn ModelAdapter>`：

```rust
use std::sync::Arc;
use agent_runtime_core::model::ModelAdapter;

let model: Arc<dyn ModelAdapter> =
    Arc::from(agent_runtime_providers::create_adapter_from_config(
        agent_runtime_providers::ProviderRuntimeConfig {
            model: "anthropic/claude-sonnet-4-6".into(),
            api_key: None,
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            api_url: None,
            max_tokens: Some(1024),
        },
    )?);
```

- **model 字符串是 `provider/model`**：前缀选 provider（`anthropic` / `openai` / `deepseek` / `openrouter`），后缀是该 provider 的原生模型名。
- **API key 解析顺序**：显式 `api_key` → `api_key_env` 指定的环境变量 → provider 默认环境变量。本例用 `api_key_env`。

## 4. 注册一个 tool

tool 是模型可以调用的能力。实现 `Tool` trait 的 6 个方法，包装成 `Arc<dyn Tool>` 注册进 `ToolRegistry`：

```rust
use std::sync::OnceLock;
use agent_runtime_core::tool::{
    Approval, JsonSchema, Tool, ToolContext, ToolError, ToolMetadata, ToolOutput, ToolSource,
};
use async_trait::async_trait;
use serde_json::json;

struct CurrentTimeTool;

fn empty_object_schema() -> &'static JsonSchema {
    static SCHEMA: OnceLock<JsonSchema> = OnceLock::new();
    SCHEMA.get_or_init(|| json!({ "type": "object", "properties": {} }))
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
        Ok(ToolOutput::Immediate(json!({ "utc": "2026-06-06T00:00:00Z" })))
    }
}
```

要点：

- `input_schema()` / `metadata()` 返回引用，因此值必须 `'static`——用 `OnceLock` 持有（无 `unsafe`）。
- **`Approval` 三态**：`Never`（只读/计算，从不审批）、`WhenRisky`（有副作用时按 run 级策略）、`Always`（外部通信/破坏性操作，总是审批）。无副作用的 tool 用 `Never`。
- `execute` 返回 `ToolOutput::Immediate(value)` 是最常见的同步结果；另有 `Structured` / `AsyncJob` / `Handoff` 等高级形态。

### ToolError 的失败分类

`ToolError::invalid_input(...)` 用于输入已经明确但不符合 schema 或业务约束的情况，例如缺少字段、字段类型错误或值越界。`ToolError::ambiguity(...)` 用于请求本身存在多个合理解释的情况，此时默认 `next_step = "clarify"`，引导模型向用户澄清而不是猜测或重试。

`ToolError::spec_gap(...)` 用于 SDK 或应用契约缺少必要行为的情况，此时默认 `next_step = "escalate"`，引导模型升级给调用方或 supervising agent。

### 重复失败 Hook

`RuntimeConfig.repeated_failure.threshold` 默认是 `3`。同一个 run 内，当同一个 tool 以同一个 `ErrorKind` 连续累计到阈值时，runtime 会调用 `Hook::on_repeated_failure`，传入 `run_id`、`tool_name`、`error_kind`、`error_history` 和 `count`。Hook 可以返回 `Continue` 继续运行，或返回 `Abort(reason)` 让 run 失败退出。

这个 hook 是应用层介入点，不是内置策略引擎。需要跨子 agent、Supervised Delegation 或多订阅者场景做观察、注入 steering、或统一 abort 时，优先配合 watcher 示例使用。

## 5. 启动 run + 监听事件

```rust
use agent_runtime_core::events::RuntimeEvent;
use agent_runtime_core::model::StreamEvent;
use agent_runtime_core::run::{AgentConfig, AgentRun};
use agent_runtime_core::tool::registry::ToolRegistry;

let mut registry = ToolRegistry::new();
registry.register(Arc::new(CurrentTimeTool))?;

let config = AgentConfig::builder("anthropic/claude-sonnet-4-6")
    .system_prompt("You are a helpful assistant. Use tools when useful.")
    .max_steps(5)
    .build()?;

let (handle, mut rx) = AgentRun::start(config, "What time is it?".into(), model, registry);

while let Some(event) = rx.recv().await {
    match event {
        RuntimeEvent::ModelStreamChunk { delta: StreamEvent::Text { delta } } => print!("{delta}"),
        RuntimeEvent::ToolCallStarted { tool, .. } => println!("\n[tool] calling {tool}"),
        RuntimeEvent::ToolCallCompleted { tool, output, .. } => println!("[tool] {tool} -> {output}"),
        RuntimeEvent::RunCompleted { output } => println!("\n[done] {output}"),
        RuntimeEvent::RunFailed { error } => eprintln!("\n[failed] {error}"),
        _ => {}
    }
}
```

`AgentRun::start` 返回 `(RunHandle, EventReceiver)`。事件流是 Orchest 的核心模型——模型输出、tool 调用、审批、预算、子 agent 等都以 `RuntimeEvent` 流出。起步阶段你会关心：

| 事件 | 含义 |
|------|------|
| `RunStarted` | run 开始 |
| `ModelStreamChunk` | 模型流式输出的一段增量（`StreamEvent::Text { delta }` 是文本） |
| `ToolCallStarted` | 模型发起一次 tool 调用 |
| `ToolCallCompleted` | tool 返回结果 |
| `ToolCallFailed` | tool 执行失败（携带结构化 `ToolError`） |
| `ToolCallRetry` | runtime 准备重试 tool 调用（携带 attempt 和上一条错误） |
| `RunCompleted` | run 正常结束，带最终 `output` |
| `RunFailed` | run 失败 |

完整变体见 [`RuntimeEvent` 的 rustdoc](../../crates/agent-runtime-core/src/events.rs)。

> 进阶：`RunHandle` 还有 `subscribe_events(capacity)` 可获得额外的事件订阅者（多方观察场景），以及 `inject_message` / `steer` / `abort` 用于运行中干预。起步阶段用 `start` 直接返回的 `rx` 即可。

## 6. 等待完成

事件流结束后，等 run loop 的后台任务收尾：

```rust
handle.wait().await;
```

## 7. 完整代码

以上片段合起来就是 [`examples/rust/basic_agent_run.rs`](../../examples/rust/basic_agent_run.rs)。直接运行：

```bash
ANTHROPIC_API_KEY=sk-... cargo run --example basic_agent_run
```

## 8. 下一步

- **进阶示例**（[`examples/rust/`](../../examples/rust/)）：
  - 自定义 Hook：`hook_logging.rs` / `hook_abort.rs` / `hook_modifier.rs`
  - Guardrail：`guardrail_keyword_filter.rs` / `guardrail_output_sanitize.rs`
  - Session 持久化 + 恢复：`session_persist_resume.rs`
  - Handoff：`handoff_routing.rs` / `handoff_input_filter.rs`
  - Mid-run steering / watcher：`watcher_inject_message.rs` / `watcher_abort_on_pattern.rs`
  - Supervised Delegation：`supervised_delegation.rs`
  - Agent-as-Tool：`agent_as_tool.rs`
- **其他语言**：[SDK · Python](./sdk-python.md)、[SDK · TypeScript](./sdk-typescript.md)
