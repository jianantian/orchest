# DeepSeek 模型列表更新 Implementation Plan

## Files to Read

- `crates/orchest-provider-http/src/catalog/mod.rs`（`deepseek_models`）
- `crates/orchest-provider-http/src/providers/deepseek/profile.rs`
- `crates/orchest-provider-http/src/pricing.rs`

## Files to Change

- `crates/orchest-provider-http/src/catalog/{mod.rs,tests.rs}`
- `crates/orchest-provider-http/src/providers/deepseek/{profile.rs,tests.rs}`
- `crates/orchest-provider-http/src/pricing.rs`
- `crates/orchest-provider-http/src/{protocol.rs,tests.rs,telemetry.rs}`（测试 fixture 模型名）
- `crates/orchest-provider/tests/selection.rs`（fixture 模型名）
- `examples/{rust,python,typescript}/providers/deepseek.*`、`examples/demo/briefing-desk/.env.example`
- `docs/guide/sdk-{python,typescript}.md`、`docs/polaris/observability.md`
- `python/tests/test_atomic_api.py`、`js/tests/atomic-api.test.cjs`
- `CHANGELOG.md`

## Steps

1. 改写 catalog DeepSeek 两行（id、display name、描述、价格）并更新 catalog tests。
2. profile：`is_flash_family` / `supports_thinking` / 上下文窗口按新名判断；effort 映射加入 `low`；
   capabilities efforts 改为 `[Low, High, Max]`。
3. `deepseek_pricing` 改为 Flash / Pro 新价，删除旧名分支，更新单测。
4. DeepSeek provider tests：默认 adapter 模型改为 `deepseek-flash`，新增别名与已停用名能力测试，
   effort 映射测试改为 low/high/max。
5. 其余测试 fixture、示例、指南、绑定测试中的旧名替换为新名。
6. CHANGELOG 条目；跑 fmt / clippy / test / lint-check。
