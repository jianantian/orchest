# DeepSeek 模型列表更新：deepseek-flash / deepseek-v4-pro

## Background

DeepSeek 官方在线模型已收敛为 `deepseek-flash`（DeepSeek-V4.1-Flash）与 `deepseek-v4-pro`
（DeepSeek-V4-Pro-0813），见 [模型 & 价格](https://api-docs.deepseek.com/zh-cn/quick_start/pricing)
与 [更新日志 2026-09-10](https://api-docs.deepseek.com/updates)。Orchest 当前：

- catalog（`crates/orchest-provider-http/src/catalog/mod.rs`）列出 `deepseek/deepseek-v4-flash` 与
  `deepseek/deepseek-v4-pro`，价格停留在旧版本（Flash ¥1/¥2、Pro ¥3/¥6）；
- `DeepSeekProfile::supports_thinking` 与上下文窗口按 `deepseek-v4*` / `deepseek-reasoner` 前缀判断，
  `deepseek-flash` 会被当作非思考、64K 上下文的未知模型；
- `pricing::deepseek_pricing` 仍为 `deepseek-chat` / `deepseek-reasoner` 定价；
- 思考强度只映射 `high` / `max`，而官方现支持 `low` / `high` / `max`
  （[思考模式](https://api-docs.deepseek.com/guides/thinking_mode)）；
- 测试、示例、SDK 指南与 Python/Node 绑定测试仍使用 `deepseek-chat` / `deepseek-v4-flash`。

官方对旧名的说明：`deepseek-v4-flash`、`deepseek-v4-flash-vision-exp` 仍可调用，由 V4.1 Flash 服务、
按 Flash 计费；`deepseek-chat`、`deepseek-reasoner` 已于 2026-07-24 停用。

## Goal / Scope

- catalog DeepSeek 段的在线模型为 `deepseek/deepseek-flash` 与 `deepseek/deepseek-v4-pro`，能力与价格对齐官方。
  1.0.0 已发布的 `deepseek/deepseek-v4-flash` 行保留为 `ModelStatus::Deprecated`（ADR-0003 D4：
  1.x 内删除 catalog 行属破坏性变更），保持 1.0.0 的纯文本能力，仅价格随上游更新。
- profile 名称兜底识别 `deepseek-flash` 与 `deepseek-v4-pro`；无 catalog 行的官方路由别名
  `deepseek-v4-flash-vision-exp` 按 Flash 家族识别（thinking / 1M / Flash 价格 / 图像）；
  `deepseek-chat` / `deepseek-reasoner` 不再被识别（落入未知模型的保守默认）。
- 思考强度映射 `Minimal`/`Low` → `low`，`Medium`/`High` → `high`，`XHigh`/`Max` → `max`；
  capabilities `reasoning.efforts` 为 `[Low, High, Max]`。
- 同步测试、示例、SDK 指南、demo `.env.example`、绑定测试与 CHANGELOG。

## Acceptance Criteria

- [x] `list_models()` 的 DeepSeek 行为 `deepseek/deepseek-flash`、`deepseek/deepseek-v4-pro`（Stable）与
      `deepseek/deepseek-v4-flash`（Deprecated，`input_modalities = [Text]`）；默认 `ModelFilter` 的
      发现结果不含 Deprecated 行，`Registry::chat().id("deepseek/deepseek-v4-flash")` 仍可选中；
      `find_model("deepseek-v4-flash-vision-exp")`、`find_model("deepseek-chat")` 返回 `None`。
- [x] 两行均为 context 1M、max output 384K、max input 616K、`thinking: Some(_)`；
      `deepseek-flash` 价格 CNY 输入 2 / 输出 8 / 缓存命中 0.04，`deepseek-v4-pro` 输入 9 / 输出 27 /
      缓存命中 0.30（高峰价，注释说明空闲时段半价）。
- [x] `ChatAdapter` 对 `deepseek-flash`、`deepseek-v4-pro`、`deepseek-v4-flash` 报告
      `reasoning.supported = true`、`efforts = [Low, High, Max]`、`context_window_size = 1_000_000`；
      对 `deepseek-chat` / `deepseek-reasoner` 报告 `reasoning.supported = false`、64K。
- [x] `deepseek_pricing` 对 `deepseek-flash` 与 `deepseek-v4-flash*` 返回 Flash 价、对 `deepseek-v4-pro`
      返回 Pro 价；不再存在 `deepseek-chat` / `deepseek-reasoner` 分支。
- [x] 请求体：`ThinkingLevel::Minimal`/`Low` → `reasoning_effort: "low"`，`Medium`/`High` → `"high"`，
      `XHigh`/`Max` → `"max"`；`Off` 仍为 `thinking: {type: disabled}` 且无 `reasoning_effort`。
- [x] 仓库内非归档代码、测试、示例、`docs/guide`、`docs/polaris` 不再引用 `deepseek-chat` /
      `deepseek-reasoner`；`deepseek-v4-flash` 仅作为 Deprecated 行与路由别名测试出现。
- [x] CHANGELOG `[Unreleased]` 记录 catalog 变更（Changed/Deprecated），不含 **Breaking:** 条目。
- [x] `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、
      `bash scripts/lint-check.sh` 通过。

## Notes

- 不把旧名静默映射到 catalog 新行：只有官方明确声明路由的 `deepseek-v4-flash*` 在 profile 名称兜底里
  共享 Flash 能力，`model` 字段原样发送。
- `deepseek/deepseek-v4-flash` 保留为 Deprecated 行（owner 决定，2026-10-01）：1.0.0 用户的身份选择与
  `find_model` 在 1.x 内不失效，本 hotfix 可作为 minor 发布。该行保持纯文本，使
  `accepts([Image])` 只命中 `deepseek-flash`，不产生歧义选择；需要图像请用 `deepseek-flash`。
- `docs/archive`、`docs/review/evidence`、`docs/external` 为历史/上游快照，不改。
