# 006 · 依赖反转 — 实施计划

## 依赖

**必须在 002 之后**。本 issue 移动的是 002 修复后的 `ModelError`（`upstream` 字段已合并为 `Option<Arc<UpstreamErrorDetail>>`），不是原始 7 字段版本。

## 步骤

### Step 1: 创建 crate 骨架

```bash
mkdir -p crates/agent-runtime-model/src
```

**文件**：`crates/agent-runtime-model/Cargo.toml`

```toml
[package]
name = "agent-runtime-model"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
async-trait = "0.1"
```

**文件**：根 `Cargo.toml` workspace members 中添加 `"crates/agent-runtime-model"`

### Step 2: 确定需要移动的类型

从 `crates/agent-runtime-providers/src/types.rs`（586 行）中识别：

```bash
grep 'pub \(struct\|enum\|trait\)' crates/agent-runtime-providers/src/types.rs
```

需要移动的（trait + 纯数据类型）：
- `ModelAdapter` trait
- `Message`, `ContentBlock`, `Role`, `ToolDef`, `JsonSchema`
- `ModelResponse`, `StopReason`, `TokenUsage`
- `StreamEvent` / `ModelStreamChunk`
- `RequestOptions`, `ModelCapabilities`
- `ModelError`, `UpstreamErrorDetail`（002 后的版本）
- `ModelPricing`
- `OptionAdjustment`
- `ProviderRuntimeConfig`

保留在 providers：
- 各 adapter 实现
- provider 专用配置（`AnthropicConfig` 等）
- SSE 解析、HTTP client、telemetry

### Step 3: 移动类型到新 crate

**文件结构**：
```
crates/agent-runtime-model/src/
├── lib.rs          — pub mod + pub use re-exports
├── adapter.rs      — ModelAdapter trait
├── types.rs        — Message, ContentBlock, Role, ToolDef, JsonSchema
├── response.rs     — ModelResponse, StopReason, TokenUsage, OptionAdjustment
├── options.rs      — RequestOptions, ModelCapabilities, ModelPricing
├── stream.rs       — StreamEvent, ModelStreamChunk
└── error.rs        — ModelError, UpstreamErrorDetail
```

1. 逐文件从 `providers/src/types.rs` 剪切粘贴
2. 修复 import 路径
3. `lib.rs` 统一 re-export

### Step 4: 更新 providers Cargo.toml

**文件**：`crates/agent-runtime-providers/Cargo.toml`

1. 添加 `agent-runtime-model = { path = "../agent-runtime-model" }`
2. `providers/src/types.rs` 改为 `pub use agent_runtime_model::*;`（或删除文件，各处直接用 model crate 路径）
3. 保持 `providers/src/lib.rs` 的 `pub use types::*` 不变（对外 API 不变）

### Step 5: 更新 core Cargo.toml

**文件**：`crates/agent-runtime-core/Cargo.toml`

1. 将 `agent-runtime-providers` 依赖替换为 `agent-runtime-model = { path = "../agent-runtime-model" }`
2. 删除 `agent-runtime-providers` 依赖

### Step 6: 更新 core 的 model/mod.rs

**文件**：`crates/agent-runtime-core/src/model/mod.rs`（当前 12 行，27 符号 re-export）

改为：
```rust
pub use agent_runtime_model::*;
```

### Step 7: 修复编译错误

这一步是工作量主体。预期错误来源：

1. core 中通过 `crate::model::X` 引用的类型——路径不变（re-export 兜底）
2. core 中直接 `use agent_runtime_providers::X` 的位置——改为 `use agent_runtime_model::X`
3. providers 中 `use crate::types::X` 的位置——改为 `use agent_runtime_model::X`（或保持 re-export）

```bash
cargo check -p agent-runtime-model
cargo check -p agent-runtime-providers
cargo check -p agent-runtime-core
cargo check --workspace
```

逐轮修复直到全 workspace 通过。

### Step 8: ToolCall 位置（已确认）

`ToolCall` 定义在 `core/tool/mod.rs:107`，不在 providers。**保留在 core，不移动。** 但 `ToolDef`（tool 的 schema 定义）在 providers `types.rs` 中，作为 `ModelAdapter` 接口的一部分，应移入 model crate。

## 依赖图验证

完成后的 crate 依赖应为：
```
agent-runtime-model       — 叶节点，零 workspace 依赖
agent-runtime-core        — 依赖 model，不依赖 providers
agent-runtime-providers   — 依赖 model，不依赖 core
agent-runtime-py          — 依赖 core + providers
agent-runtime-node        — 依赖 core + providers
```

```bash
cargo tree -p agent-runtime-core | grep agent-runtime   # 应只有 model
cargo tree -p agent-runtime-providers | grep agent-runtime  # 应只有 model
```

## 验证

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo tree -p agent-runtime-core | grep -v model | grep agent-runtime  # 应为空
```
