# Issue 004:`SubAgentBuilder::build()` 返回 `Result`

GitHub: [#199](https://github.com/jianantian/orchest/issues/199) · release-blocker · 无依赖

## 现状

`SubAgentBuilder::build()`(`crates/orchest/src/tool/agent_as_tool.rs`)在 `.model()` 或 `.registry()` 未调用时内部 `.expect(...)` panic,与 runtime 其余可失败构造路径不一致:`AgentConfigBuilder::build() -> Result<_, ConfigError>`、`ToolRegistry::register() -> Result<_, RegistryError>`。

## 方向(提案)

```rust
pub fn build(self) -> Result<SubAgentTool, ConfigError>
```

复用 `ConfigError`(builder 缺字段正是配置错误),新增变体如 `ConfigError::MissingField { builder: &'static str, field: &'static str }`(或对齐 `ConfigError` 现有变体风格,实施时以现有命名惯例为准),不为两个缺字段场景引入新错误类型。

## 落地与测试

- 更新全部调用方:`examples/rust/agents/agent_as_tool.rs`、`examples/demo/briefing-desk/src/app.rs` 的 `reviewer_tool()`、crate 内测试
- 测试:缺 `model`、缺 `registry` 各返回对应错误;齐全时 `Ok`
- 重跑 `cargo test -p briefing-desk-demo`(特别是 `review_report` 相关),输出贴回 #199 或关闭它的 PR

## 验收标准(对齐 GitHub #199)

- [ ] `build()` 签名改为 `Result`,不再有内部 `expect`
- [ ] 所有调用方更新,`cargo test --workspace` 过
- [ ] demo 测试重跑,输出贴回 issue/PR
- [ ] `docs/review/v0_10_demo_validation.md` 更新(Triage #5 行)
