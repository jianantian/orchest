# 008 · Examples + Final Validation — 实施计划

## 前置条件

- 002（Hook Framework）、003（Agent-as-Tool）、004（Handoff）、006（LLM Retry）、007（Loop Detection）全部合入
- `cargo test --workspace` 全绿

---

## 步骤

### 步骤 1：确认 examples 目录结构和基础 mock 设施

现有示例位于 `examples/rust/`。新增示例也放在此目录，文件名与 spec 一致。

现有 `run/tests.rs` 中已有多种 mock model（`FakeModelAdapter`、`ToolCallModelAdapter` 等），可在示例中复用相同模式：每个示例自定义简单 mock model，不依赖真实 API key。

在 `crates/agent-runtime-core/Cargo.toml` 中为每个新示例添加 `[[example]]` 条目（紧接现有 `rust_deep_research_agent` 条目之后）：

```toml
[[example]]
name = "hook_logging"
path = "../../examples/rust/hook_logging.rs"

[[example]]
name = "hook_modifier"
path = "../../examples/rust/hook_modifier.rs"

[[example]]
name = "hook_abort"
path = "../../examples/rust/hook_abort.rs"

[[example]]
name = "agent_as_tool"
path = "../../examples/rust/agent_as_tool.rs"

[[example]]
name = "handoff_routing"
path = "../../examples/rust/handoff_routing.rs"

[[example]]
name = "handoff_input_filter"
path = "../../examples/rust/handoff_input_filter.rs"

[[example]]
name = "retry_exhausted"
path = "../../examples/rust/retry_exhausted.rs"

[[example]]
name = "loop_detection"
path = "../../examples/rust/loop_detection.rs"
```

---

### 步骤 2：编写 `examples/rust/hook_logging.rs`

演示内容：注册两个自定义 Hook，在 `on_run_start` / `before_model` / `after_tool` 打印日志。

核心结构：

```rust
struct LoggingHook { name: String }

#[async_trait]
impl Hook for LoggingHook {
    async fn on_run_start(&self, ctx: &mut RunHookContext) {
        println!("[{}] run started: {:?}", self.name, ctx.run_id);
    }
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        println!("[{}] before_model: {} messages", self.name, ctx.messages.len());
        ModelHookAction::Continue
    }
    async fn after_tool(&self, ctx: &mut ToolHookContext, _: &ToolOutput) -> HookAction {
        println!("[{}] after_tool: {}", self.name, ctx.tool_name);
        HookAction::Continue
    }
}

#[tokio::main]
async fn main() {
    let config = AgentConfig::default()
        .with_hook(Arc::new(LoggingHook { name: "hook-1".to_string() }))
        .with_hook(Arc::new(LoggingHook { name: "hook-2".to_string() }));
    // 使用 mock model 运行，收集事件
    // ...
}
```

---

### 步骤 3：编写 `examples/rust/hook_modifier.rs`

演示内容：`before_model` hook 修改 messages（注入系统 context）。

```rust
struct ContextInjectorHook { extra_context: String }

#[async_trait]
impl Hook for ContextInjectorHook {
    async fn before_model(&self, ctx: &mut ModelHookContext) -> ModelHookAction {
        ctx.messages.push(Message {
            role: Role::User,
            content: vec![ContentBlock::Text(
                format!("[injected context] {}", self.extra_context)
            )],
        });
        ModelHookAction::Continue
    }
}
```

---

### 步骤 4：编写 `examples/rust/hook_abort.rs`

演示两种 error path：

**场景 A**：hook 返回 `Abort` 终止 run

```rust
struct AbortAfterNHook { n: usize, count: Mutex<usize> }

#[async_trait]
impl Hook for AbortAfterNHook {
    async fn before_model(&self, _: &mut ModelHookContext) -> ModelHookAction {
        let mut count = self.count.lock().unwrap();
        *count += 1;
        if *count >= self.n {
            ModelHookAction::Abort("abort triggered by hook".to_string())
        } else {
            ModelHookAction::Continue
        }
    }
}
```

**场景 B**：hook panic，run 继续执行

```rust
struct PanickingHook;

#[async_trait]
impl Hook for PanickingHook {
    async fn before_model(&self, _: &mut ModelHookContext) -> ModelHookAction {
        panic!("intentional panic for demo");
    }
}
// 主函数中观察到 HookPanicked 事件但 run 完成
```

---

### 步骤 5：编写 `examples/rust/agent_as_tool.rs`

演示 `AgentConfig::as_tool()`：父 agent 调用子 agent 作为工具。

```rust
// 子 agent：接受 "input" 参数，返回处理结果
let child_config = AgentConfig::builder()
    .system_prompt("You are a calculator agent. Given a math expression, return the result.")
    .build();

// 父 agent：注册子 agent 为工具
let child_tool = child_config.as_tool("calculator", "Evaluates math expressions", mock_model.clone(), ToolRegistry::new());
let parent_registry = ToolRegistry::new();
parent_registry.register(child_tool).unwrap();

let parent_config = AgentConfig::builder()
    .system_prompt("You are an assistant. Use the calculator tool when needed.")
    .build();
// 运行父 agent，观察 SubAgentStarted/SubAgentCompleted 事件
```

---

### 步骤 6：编写 `examples/rust/handoff_routing.rs`

演示 Handoff：Triage agent 路由到 Billing / Support agent。

```rust
let billing_config = AgentConfig::builder().system_prompt("Billing specialist").build();
let support_config = AgentConfig::builder().system_prompt("Support specialist").build();

let triage_config = AgentConfig::builder()
    .system_prompt("Triage agent. Route to billing or support.")
    .with_handoff(Handoff {
        tool_name: "route_to_billing".to_string(),
        tool_description: "Route user to billing department".to_string(),
        input_schema: json!({"type":"object","properties":{"reason":{"type":"string"}}}),
        target: Arc::new(HandoffTarget::Static(billing_config)),
        input_filter: None,
        nest_history: false,
    })
    .with_handoff(Handoff {
        tool_name: "route_to_support".to_string(),
        // ...
        target: Arc::new(HandoffTarget::Static(support_config)),
        // ...
    })
    .build();
// 运行，观察 AgentUpdated 事件
```

---

### 步骤 7：编写 `examples/rust/handoff_input_filter.rs`

演示 Handoff 时用 `input_filter` 裁减上下文。

```rust
struct TruncateHistoryFilter { keep_last: usize }

#[async_trait]
impl HandoffInputFilter for TruncateHistoryFilter {
    async fn filter(&self, mut data: HandoffInputData) -> HandoffInputData {
        let n = data.input_history.len();
        if n > self.keep_last {
            data.input_history = data.input_history.split_off(n - self.keep_last);
        }
        data
    }
}

let handoff = Handoff {
    // ...
    input_filter: Some(Arc::new(TruncateHistoryFilter { keep_last: 3 })),
    nest_history: false,
};
```

---

### 步骤 8：编写 `examples/rust/retry_exhausted.rs`

演示 429 重试 + 重试耗尽后的错误处理（error path）。

```rust
// Mock model: 前 N 次返回 429，第 N+1 次成功
struct RetryableModel { fail_count: Mutex<u32>, max_fails: u32 }

#[async_trait]
impl ModelAdapter for RetryableModel {
    async fn complete(...) -> Result<ModelResponse, ModelError> {
        let mut count = self.fail_count.lock().unwrap();
        *count += 1;
        if *count <= self.max_fails {
            Err(ModelError { status: Some(429), message: "rate limited".to_string(), ... })
        } else {
            Ok(/* simple response */)
        }
    }
}

// 场景 A: max_fails=2, max_retries=3 → 成功，发出 2 次 ModelRetry 事件
// 场景 B: max_fails=10, max_retries=3 → 失败，RunFailed 事件
```

---

### 步骤 9：编写 `examples/rust/loop_detection.rs`

演示循环检测 → 警告 → 最终终止（error path）。

```rust
// Mock model: 每次返回相同工具调用（同一 tool + 同一 input）
let config = AgentConfig::builder()
    .system_prompt("test agent")
    .with_loop_detection()
    .build();

// 观察事件序列：
// - 前 warn_threshold 次：正常工具调用
// - 第 warn_threshold 次之后的 before_model：messages 末尾有警告消息
// - 第 stop_threshold 次 after_tool：RunFailed（loop detected）
```

---

### 步骤 10：集成验证测试（5 个场景）

在 `crates/agent-runtime-core/tests/e2e_validation.rs` 末尾追加：

| 场景 | 测试名 | 验证要点 |
|------|--------|---------|
| Hook + Agent-as-Tool | `hook_and_agent_as_tool` | 子 run hook 独立于父 run hook，父 run hook 不被子 run 触发 |
| Hook + Handoff | `hook_and_handoff` | `on_handoff` hook 在切换时触发，新 agent 配置自己的 hook 链 |
| Retry + Hook | `retry_and_hook` | 重试时每次都触发 before_model/after_model hook |
| Loop Detection + Handoff | `loop_detection_and_handoff` | Handoff 后新 agent 的 LoopDetectionHook 窗口为空，不继承旧 agent 的状态 |
| Agent-as-Tool + Handoff | `agent_as_tool_and_handoff` | 父 agent 注册 as_tool 子 agent 和 handoff 目标，两者共存不冲突 |

---

### 步骤 11：Lint + CI 验证

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo build --examples
```

---

## 验证

```bash
# 每个示例独立运行
cargo run --example hook_logging
cargo run --example hook_modifier
cargo run --example hook_abort
cargo run --example agent_as_tool
cargo run --example handoff_routing
cargo run --example handoff_input_filter
cargo run --example retry_exhausted
cargo run --example loop_detection

# 全量测试
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings
```

---

## 关键决策

- **所有示例使用内联 mock model**：每个示例文件自包含，复用 `tests.rs` 中的 mock 模式（直接实现 `ModelAdapter` trait），无 API key 依赖
- **error path 示例（hook_abort、retry_exhausted、loop_detection）**：通过观察 `RuntimeEvent` 序列验证错误处理路径正确
- **集成验证在 `e2e_validation.rs` 中追加**：复用已有测试基础设施，避免新建测试 crate
- **示例 `[[example]]` 注册**：每个示例必须在 `Cargo.toml` 中注册，才能 `cargo run --example` 运行
