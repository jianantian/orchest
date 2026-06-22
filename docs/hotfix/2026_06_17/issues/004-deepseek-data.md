# Issue 004:DeepSeek 模型条目数据填充

## 背景

issue 001 落地后,2 条 deepseek 模型字段全为占位值。本 issue 把它们填实。

**源**:
- https://api-docs.deepseek.com/zh-cn/quick_start/pricing (价格 + 模型规格)
- https://api-docs.deepseek.com/zh-cn/guides/thinking_mode (思考模式,**v4-flash 与 v4-pro 都支持**)
- `crates/agent-runtime-providers/src/providers/deepseek/mod.rs:76-79` (`supports_thinking()` 实现,**代码注释错误,见 issue 007**)

## 范围

`deepseek_models()` 内 2 条 entry。

### 目标数据

| model_id | description | thinking | scenes |
|---|---|---|---|
| `deepseek/deepseek-v4-flash` | "DeepSeek 默认模型,支持思考与非思考双模式,1M 上下文" | `Some(ThinkingSpec { max_thinking_tokens: None })` | General + Coding + Agent |
| `deepseek/deepseek-v4-pro` | "DeepSeek 推理增强,默认开思考模式,数学/逻辑 SOTA" | `Some(ThinkingSpec { max_thinking_tokens: None })` | General + Coding + Reasoning + Agent |

### 关于 thinking 的源说明

DeepSeek 官方思考模式指南的示例**全程使用 `model="deepseek-v4-pro"`** + `reasoning_effort="high"` + `extra_body={"thinking": {"type": "enabled"}}`,响应中含 `reasoning_content` 字段(`thinking_mode` 文档"多轮对话拼接"与"工具调用"两节均如此)。

pricing 页"思考模式"行同时对 v4-flash 和 v4-pro 标了"支持",但脚注 (1) 只点名 v4-flash 对应旧的 deepseek-chat/deepseek-reasoner——这**不**意味着 v4-pro 不支持,只是说 v4-flash 兼容了旧模型名。

`crates/agent-runtime-providers/src/providers/deepseek/mod.rs:76-79` 的 `supports_thinking()` 实现错误:

```rust
// v4-flash and legacy deepseek-reasoner support thinking; v4-pro is non-thinking only  ← 错
m.starts_with("deepseek-v4-flash") || m.starts_with("deepseek-reasoner")
```

代码 bug 修复见 issue 007。本 issue 只填 catalog 数据,**catalog 与官方文档一致(v4-pro 支持 thinking)**,不等代码 bug 修复。

### Modalities

DeepSeek V4 系列**不支持多模态**(只有 text in/out)。

- `input_modalities: &[Modality::Text]`
- `output_modalities: &[Modality::Text]`

### max_input_tokens

`Some(context_window - max_output_tokens.unwrap_or(0))` = `Some(1_000_000 - 384_000)` = `Some(616_000)`。

### 现有字段不动

- `context_window: 1_000_000` 保持
- `max_output_tokens: Some(384_000)` 保持
- `pricing` 已在前次修复加了 `cache_read = Some(0.02 / 0.025)`,保持

## 验收标准

- [ ] 2 条 deepseek entry `description` 含"思考""1M 上下文"
- [ ] 2 条 `input_modalities` 与 `output_modalities` 仅 `Text`
- [ ] 2 条 `thinking: Some(ThinkingSpec { max_thinking_tokens: None })`
- [ ] v4-flash `scenes` 含 `General` + `Coding` + `Agent`
- [ ] v4-pro `scenes` 含 `Reasoning`(数学 SOTA)
- [ ] `max_input_tokens` = 616_000
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 依赖

- 依赖 issue 001 的 `ThinkingSpec` 修订(`max_thinking_tokens: Option<u32>`)
- 与 issue 007(deepseek 代码 bug 修复)并行,catalog 数据先行

## 注意事项

- v4-pro 支持 thinking 的依据是 DeepSeek 官方思考模式指南的示例代码,不是 pricing 页脚注
- `supports_thinking()` 代码 bug 不在本 issue 修复范围,见 issue 007
