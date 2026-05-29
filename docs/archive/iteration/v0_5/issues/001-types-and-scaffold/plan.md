# 001 实现路线

## 步骤

1. **创建 crate 骨架**
   - `crates/agent-runtime-providers/Cargo.toml` — 参照 `crates/agent-runtime-core/Cargo.toml` 的格式，依赖列表见 spec
   - `crates/agent-runtime-providers/src/lib.rs` — 只写 `pub mod types; pub use types::*;`
   - 在根 `Cargo.toml` 的 `[workspace] members` 加入 `"crates/agent-runtime-providers"`
   - 运行 `cargo check -p agent-runtime-providers` 确认空 crate 编译通过

2. **从 core 类型扩展，定义 `types.rs`**
   - 以 `crates/agent-runtime-core/src/model/mod.rs`（105 行）为起点——现有 `Message`、`Role`、`ContentBlock`、`ModelResponse`、`TokenUsage`、`StopReason`、`ModelError`、`ModelSpec` 都在这里
   - **不是复制粘贴**——几乎每个类型都需要扩展。按 spec Public types 章节逐个定义：
     - `ContentBlock` 新增 `Thinking` 变体（3 个 Option 字段）
     - `StopReason` 从 3 个变体扩展到 10 个
     - `ModelError` 从 2 字段扩展到 7 字段 + `internal()` 构造器
     - `TokenUsage` 从 2 字段扩展到 6 字段 + details HashMap
     - `ModelResponse` 新增 `option_adjustments`（含 serde 属性）
   - 新增类型（core 中不存在）：`ThinkingLevel`、`CachePolicy`、`CompatibilityPolicy`、`CapabilitySource`、`RequestOptions`、`ModelCapabilities`、`ReasoningCapability`、`CacheCapability`、`OptionAdjustment`、`StreamEvent`
   - `ToolDef` + `JsonSchema` 从 core 的 `crate::tool::ToolDef` 独立定义（不引用 core）
   - `ModelAdapter` trait：4 个方法，`complete()` 替代 core 的 `stream()` + `call()`

3. **处理 derive 兼容性**
   - 优先尝试 spec 中标注的 derive 组合（`Debug, Clone, Serialize, Deserialize` + 枚举额外 `Default, PartialEq, Eq`）
   - `OptionAdjustment` 含 `serde_json::Value` 字段——`Value` 不实现 `Eq`，所以 `OptionAdjustment` 只能 derive `PartialEq` 不能 `Eq`
   - `ModelCapabilities` 含 `ReasoningCapability { efforts: Vec<ThinkingLevel> }`——derive `Default` 需要 `Vec` 的 default（空 vec），OK
   - 如有 derive 冲突，修复后在 `types.rs` 顶部注释记录差异

4. **写测试**
   - spec 列了 17 个测试，全部在 `types.rs` 底部的 `#[cfg(test)] mod tests` 中
   - serde 往返测试模式：`let x = ...; let json = serde_json::to_string(&x).unwrap(); let y: T = serde_json::from_str(&json).unwrap(); assert_eq!(x, y);`
   - `ModelResponse` skip_serializing_if 测试：序列化后用 `serde_json::from_str::<Value>` 检查 JSON key 存在/不存在
   - 运行 `cargo test -p agent-runtime-providers` 和 `cargo clippy -p agent-runtime-providers -- -D warnings`

## 关键决策

- `ModelAdapter` 用 `#[async_trait]` 还是原生 async trait（Rust 1.75+）？查项目 MSRV。core 用了 `async-trait = "0.1"`，保持一致
- `StreamEvent` vs core 的 `ModelStreamChunk`：在 providers 中只定义 `StreamEvent`，`ModelStreamChunk` alias 留给 issue 007 在 core 中定义
