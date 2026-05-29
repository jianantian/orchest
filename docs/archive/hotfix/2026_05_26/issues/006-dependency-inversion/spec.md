# 006 · 依赖反转

## 背景

**A1**：`agent-runtime-core` 通过 Cargo dependency 依赖 `agent-runtime-providers`，获取 `ModelAdapter` trait 和核心类型。`model/mod.rs` 做 27 符号 blanket re-export。

后果：编译 core 必须编译全部 4 个 provider adapter。新增 provider 污染 core 编译图。

当前依赖方向：

```
core ──depends-on──> providers
```

应该是：

```
core ──depends-on──> model (trait + types only)
providers ──depends-on──> model
```

## 目标

抽取 `agent-runtime-model` crate，只含 `ModelAdapter` trait 和纯数据类型。core 和 providers 各自依赖 model，互不依赖。

## 需要移动的类型

从 `agent-runtime-providers/src/types.rs` 移入 `agent-runtime-model`：

- `ModelAdapter` trait
- `Message`、`ContentBlock`、`Role`、`ToolDef`
- `ToolCall`（注意：需确认实际定义位置——可能在 `core/tool/mod.rs` 而非 providers。以实际 grep 结果为准）
- `ModelResponse`、`StopReason`、`TokenUsage`
- `StreamEvent`（`ModelStreamChunk` 的 provider 层等价物）
- `RequestOptions`、`ModelCapabilities`
- `ModelError`（使用 002 修复后的新结构：`upstream` 字段已合并为 `Option<Arc<UpstreamErrorDetail>>`）
- `ModelPricing`

**前置条件**：002（Error 类型治理）必须先完成。本 issue 移动的是 002 修复后的 `ModelError`，不是原始 7 字段版本。

保留在 `agent-runtime-providers`：

- 各 adapter 实现（`AnthropicAdapter`、`OpenAiAdapter` 等）
- `create_adapter_from_config` 工厂函数
- Provider 专用配置类型（`AnthropicConfig` 等）
- SSE 解析、HTTP client、telemetry

## 新 crate 结构

```
crates/agent-runtime-model/
├── Cargo.toml
└── src/
    ├── lib.rs          # pub re-exports
    ├── adapter.rs      # ModelAdapter trait
    ├── types.rs        # Message, ContentBlock, Role, ToolDef, etc.
    ├── response.rs     # ModelResponse, StopReason, TokenUsage
    ├── options.rs      # RequestOptions, ModelCapabilities
    └── error.rs        # ModelError
```

## Cargo.toml 变更

```toml
# agent-runtime-model/Cargo.toml
[package]
name = "agent-runtime-model"
# 零 workspace-internal 依赖，外部依赖最小化

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"          # Value 类型（UpstreamErrorDetail.body）
thiserror = "2"           # ModelError derive
async-trait = "0.1"       # ModelAdapter trait（如 trait 含 async fn 则需要）

# agent-runtime-core/Cargo.toml
[dependencies]
agent-runtime-model = { path = "../agent-runtime-model" }
# 删除 agent-runtime-providers 依赖

# agent-runtime-providers/Cargo.toml
[dependencies]
agent-runtime-model = { path = "../agent-runtime-model" }
```

## core 的 model/mod.rs

从 27 符号 blanket re-export 变为：

```rust
pub use agent_runtime_model::*;
```

或者直接删除 `model/` 目录，让 core 通过 `agent_runtime_model::` 路径引用。取决于是否要保持 `crate::model::X` 的内部使用惯例。

## 验收标准

- [ ] `agent-runtime-model` crate 存在，含 `ModelAdapter` trait 和全部纯数据类型
- [ ] `agent-runtime-model` 无 workspace-internal 依赖（是叶节点）
- [ ] `agent-runtime-core/Cargo.toml` 不依赖 `agent-runtime-providers`
- [ ] `agent-runtime-providers/Cargo.toml` 依赖 `agent-runtime-model`，不依赖 core
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --workspace -- -D warnings` 全绿
