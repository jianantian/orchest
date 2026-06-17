# Issue 004:DeepSeek 模型条目数据填充

## 背景

issue 001 落地后,2 条 deepseek 模型字段全为占位值。本 issue 把它们填实。

**源**: https://api-docs.deepseek.com/zh-cn/quick_start/pricing
DeepSeek 模型详情页:https://api-docs.deepseek.com/zh-cn/quick_start/models

## 范围

`deepseek_models()` 内 2 条 entry。

### 目标数据

| model_id | description | thinking | scenes |
|---|---|---|---|
| `deepseek/deepseek-v4-flash` | "DeepSeek 默认模型,支持思考与非思考双模式,1M 上下文" | `Some(ThinkingSpec { max_thinking_tokens: None })` | General + Coding + Agent |
| `deepseek/deepseek-v4-pro` | "DeepSeek 推理增强,默认开思考模式,SOTA on math" | `Some(ThinkingSpec { max_thinking_tokens: None })` | General + Coding + Reasoning + Agent |

DeepSeek 思考模式 token 上限源未明确公开,填 `None`。

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

- [ ] 2 条 deepseek entry `description` 含"思考""1M 上下文"等关键词
- [ ] 2 条 `input_modalities` 与 `output_modalities` 仅 `Text`
- [ ] 2 条 `thinking` = `Some(ThinkingSpec { max_thinking_tokens: None })`
- [ ] v4-flash `scenes` 含 `General` + `Coding` + `Agent`
- [ ] v4-pro `scenes` 含 `Reasoning`(数学 SOTA)
- [ ] `max_input_tokens` = 616_000
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 依赖

- 依赖 issue 001 的 `ThinkingSpec` 修订(`max_thinking_tokens: Option<u32>`)
