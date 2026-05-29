# 007 实现路线

## 步骤

1. **添加依赖，重写 model/mod.rs**
   - `crates/agent-runtime-core/Cargo.toml` 添加 `agent-runtime-providers = { path = "../agent-runtime-providers" }`
   - 把 `src/model/mod.rs` 的 105 行类型定义全部替换为 re-export 块（见 spec）
   - 添加 `pub type ModelStreamChunk = StreamEvent;` backward compat alias
   - 删除 `pub mod anthropic;` 和 `pub mod openai;`
   - `cargo check` — 这一步会产生大量编译错误，逐个修

2. **删除旧 adapter 文件**
   - 删除 `src/model/anthropic.rs` 和 `src/model/openai.rs`
   - 这些代码已经迁移到 providers crate

3. **修复 run.rs**
   - 这是最关键的改动——`run.rs` 引用了 `model.stream()` 和 `model.call()`
   - `model.stream(messages, tools, tx)` → `model.complete(messages, tools, &options, Some(tx))`
   - `model.call(messages, tools)` → `model.complete(messages, tools, &options, None)` 或 `chat(model, messages, tools, &options).await`
   - 需要在调用处构造 `RequestOptions::default()`（或从 `AgentConfig` 派生——但 issue 007 不引入新 feature，先用 default）
   - `StopReason` 的 match：现有代码只 match `EndTurn / ToolUse / MaxTokens`，新增变体需要 `_ => { ... }` 默认分支（现有代码可能已有）

4. **修复其他文件的 import**
   - `src/events.rs`：`ModelStreamChunk` → 通过 alias 使用或改为 `StreamEvent`
   - `src/tool/mod.rs`：如果有自己的 `ToolDef` 定义，删除并改为 `use crate::model::ToolDef;`
   - `src/budget.rs`：`TokenUsage` 新增字段有 default，向后兼容
   - 全局搜 `use crate::model::anthropic` 和 `use crate::model::openai` 确保无遗漏

5. **适配 MockModelProvider**
   - 搜索 `MockModel` 或 `mock`——如果 core 有 mock 实现，需要适配新的 `ModelAdapter` trait
   - 新增 `provider_name()` → `"mock"`、`model_name()` → `"mock-model"`、`capabilities()` → `ModelCapabilities::default()`
   - `stream()` → `complete()`，参数加 `options: &RequestOptions`（忽略即可）

6. **验证**
   - `cargo test --workspace` — 重点关注 `e2e_validation.rs` 和 `v03_runtime.rs`
   - `cargo clippy --workspace -- -D warnings`
   - 确认 SDK crate（`agent-runtime-py` / `agent-runtime-node`）编译通过——可能需要更新 import path

## 要读的现有代码

- `crates/agent-runtime-core/src/run.rs` — 找所有 `model.stream()` / `model.call()` 调用点
- `crates/agent-runtime-core/src/events.rs` — `RuntimeEvent::ModelStreamChunk` 变体
- `crates/agent-runtime-core/src/tool/mod.rs` — `ToolDef` 的定义位置
- `crates/agent-runtime-core/src/budget.rs` — `TokenUsage` 引用
- `crates/agent-runtime-py/src/lib.rs` 和 `crates/agent-runtime-node/src/lib.rs` — import path

## 关键决策

- `RequestOptions` 构造：issue 007 用 `RequestOptions::default()`。从 `AgentConfig` 派生 `RequestOptions`（例如 thinking level 配置）留到后续 issue
- `RuntimeEvent::ModelStreamChunk` 变体名是否改为 `ModelStreamEvent`：建议先保留现有名字避免 wire format 变化，后续 issue 单独处理
