# 006 implementation plan

## Files to read

- `crates/orchest-provider-http/src/providers/openrouter/`（模板）
- `crates/orchest-provider-http/src/{defaults.rs,registry.rs,lib.rs}`
- `crates/orchest-provider-http/src/decision/`
- Fireworks 官方文档（OpenAI 兼容 chat、鉴权、模型 id）

## Files to change

- `crates/orchest-provider-http/src/defaults.rs`：`pub mod fireworks`
- `crates/orchest-provider-http/src/providers/fireworks/{mod.rs,profile.rs,tests.rs}`
- `crates/orchest-provider-http/src/providers/mod.rs`、`registry.rs`、`lib.rs`
- `crates/orchest-provider-http/src/decision/`
- `CONVENTIONS.md`、`README.md`

## Steps

1. 照 OpenRouter 建 `providers/fireworks`，填默认值与 profile。
2. 注册 provider，实现 id 映射。
3. decision 接入。
4. 单元测试与可选冒烟测试；更新文档；跑 workspace 检查。
