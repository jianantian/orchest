# 005 implementation plan

## Files to read

- `crates/orchest-provider-http/src/providers/openrouter/{mod.rs,profile.rs,request.rs,tests.rs}`
- `crates/orchest-provider-http/src/{defaults.rs,registry.rs,lib.rs}`（`normalize_provider_model`、provider 注册）
- `crates/orchest-provider-http/src/decision/{mod.rs,openrouter.rs}`
- Vercel AI Gateway 官方文档（OpenAI 兼容接口、鉴权、模型 id）

## Files to change

- `crates/orchest-provider-http/src/defaults.rs`：`pub mod vercel`
- `crates/orchest-provider-http/src/providers/vercel/{mod.rs,profile.rs,tests.rs}`
- `crates/orchest-provider-http/src/providers/mod.rs`、`registry.rs`、`lib.rs`：注册与 id 规范化
- `crates/orchest-provider-http/src/decision/`：Vercel 接入
- `CONVENTIONS.md`、`README.md`

## Steps

1. 照 OpenRouter 建 `providers/vercel`，填默认值与 profile。
2. 注册 provider，扩展 `normalize_provider_model`。
3. decision 接入。
4. 单元测试与可选冒烟测试；更新文档；跑 workspace 检查。
