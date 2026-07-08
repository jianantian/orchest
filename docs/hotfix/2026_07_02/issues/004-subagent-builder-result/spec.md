# Issue 004:`SubAgentBuilder::build()` 返回 `Result`

GitHub: [#199](https://github.com/jianantian/orchest/issues/199) · release-blocker · 无依赖

## 现状

`SubAgentBuilder::build()`(`crates/orchest/src/tool/agent_as_tool.rs`)在 `.model()` 或 `.registry()` 未调用时内部 `.expect(...)` panic,与 runtime 其余可失败构造路径不一致:`AgentConfigBuilder::build() -> Result<_, ConfigError>`、`ToolRegistry::register() -> Result<_, RegistryError>`。

## 方向(提案)

```rust
pub fn build(self) -> Result<Arc<dyn Tool>, ConfigError>
```

`build()` 现有返回类型是 `Arc<dyn Tool>`(`agent_as_tool.rs:313`),只加 `Result` 外壳,不改动内层类型。

复用 `ConfigError`(builder 缺字段正是配置错误),新增两个专属变体
`SubAgentMissingModel` / `SubAgentMissingRegistry`,对齐 `ConfigError` 现有
"一个变体对应一种具体错误"的风格(`MissingModel`、`InvalidMaxSteps(u32)` 等
都是如此)。不引入 `MissingField { builder, field }` 这类泛化 struct 变体——
那会让调用方 `match` 时分不清具体是哪个 builder 抛的。也不直接复用已有的
`ConfigError::MissingModel`:那个变体是 `AgentConfigBuilder::build()` 校验
`model.spec.model` 空字符串时抛的,与 `SubAgentBuilder` 缺 `.model()`
(`Arc<dyn ModelAdapter>`)语义不同、字段类型也不同,复用会把两个 builder
的错误混进同一变体。

## 落地与测试

- 更新全部调用方:`examples/rust/agents/agent_as_tool.rs`、`examples/demo/briefing-desk/src/app.rs` 的 `reviewer_tool()`、crate 内测试
- 测试:缺 `model`、缺 `registry` 各返回对应错误;齐全时 `Ok`
- 重跑 `cargo test -p briefing-desk-demo`(特别是 `review_report` 相关),输出贴回 #199 或关闭它的 PR

## 验收标准(对齐 GitHub #199)

- [x] `build()` 签名改为 `Result`,不再有内部 `expect`
- [x] 所有调用方更新,`cargo test --workspace` 过
- [x] demo 测试重跑,输出贴回 issue/PR
- [x] `docs/review/v0_10_demo_validation.md` 更新(Triage #5 行)

## 实现记录

- `SubAgentBuilder::build()` 签名改为 `Result<Arc<dyn Tool>, ConfigError>`(`crates/orchest/src/tool/agent_as_tool.rs`),内部 `.expect(...)` 换成 `.ok_or(ConfigError::SubAgentMissingModel)?` / `.ok_or(ConfigError::SubAgentMissingRegistry)?`
- `ConfigError` 新增 `SubAgentMissingModel`、`SubAgentMissingRegistry` 两个专属变体(`crates/orchest/src/run/config.rs`),不复用已有的 `MissingModel`(语义与字段类型都不同,见 spec"方向"一节)
- 调用方全部更新:`examples/rust/agents/agent_as_tool.rs`(`.unwrap()`)、`examples/rust/agents/deep_research_agent.rs`(`?`,函数已返回 `Box<dyn Error>`)、`crates/orchest-py/src/lib.rs`(`.map_err(PyRuntimeError::new_err)?`)、`crates/orchest/tests/{v03_runtime,v07_integration}.rs`、`crates/orchest/src/run/tests.rs`(均 `.unwrap()`)、`examples/demo/briefing-desk/src/app.rs` 的 `reviewer_tool()`(`.expect(..)`,该函数的两个调用点确实总是设置 `.model()`/`.registry()`)
- 新测试(`crates/orchest/src/tool/agent_as_tool.rs` 新增 `mod tests`):缺 `model`→`SubAgentMissingModel`,缺 `registry`→`SubAgentMissingRegistry`,两者都设置→`Ok`
- `cargo test --workspace --features orchest/sqlite-session`:全部通过(orchest lib 254 个测试,较 003 前新增 3 个)
- `cargo clippy --workspace --all-targets -- -D warnings`:无新增 finding(与 001-003 记录的两处既有基线 finding 一致)
- `cargo fmt --check`:通过
- `bash scripts/lint-check.sh`:通过(exit 0)
- `cargo test -p briefing-desk-demo`:20 个测试全绿
