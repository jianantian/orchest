# Issue 002:Anthropic 模型条目数据填充

## 背景

issue 001 落地后,anthropic 模型字段全为占位值。本 issue 把它们填实。

**源**: `docs/external/anthropic/models.md`(锚点见各表)

## 范围

`anthropic_models()` 内现有 7 条 entry。**删 1 条(claude-fable-5),填实剩余 6 条**。

### 删除 `anthropic/claude-fable-5`

claude-fable-5 与 claude-mythos-5 已暂时下架,无可用的 API。从 catalog 移除 `anthropic/claude-fable-5` entry。

**相关清理**:
- `catalog/mod.rs` 的 `anthropic_models()` 中删除该条目
- `catalog/tests.rs:15`(`assert!(ids.contains(&"anthropic/claude-fable-5"))`)同步删除该断言
- `catalog/tests.rs` 中引用 claude-fable-5 的其它断言一并清理
- `pricing.rs:31` 的 `claude-fable-5` / `claude-mythos-5` 分支保留(模型名匹配逻辑,无害,未来若重新上架可继续用)
- `providers/anthropic/mod.rs:91-97` 的 `supports_adaptive()` 中 `claude-fable-5` / `claude-mythos-5` 分支保留(同上理由)

**不在本 issue 范围**:不清理 `pricing.rs` 和 `supports_adaptive()` 里的 fable-5/mythos-5 分支——保留无害,未来重新上架可立即生效。

### 目标数据(6 条)

所有 claude 模型按 `models.md:11` 的口径**输入支持 text + image,输出 text**。

| model_id | display_name | description | extended thinking | adaptive thinking | scenes |
|---|---|---|---|---|---|
| `anthropic/claude-opus-4-8` | Claude Opus 4.8 | "Anthropic Opus 系旗舰,复杂推理与 agentic coding"(`models.md:37`) | No(`models.md:43`) | Yes(`models.md:44`) | General + Coding + Reasoning + Agent |
| `anthropic/claude-sonnet-4-6` | Claude Sonnet 4.6 | "Anthropic 主力,速度与智能平衡"(`models.md:37`) | Yes(`models.md:43`) | Yes(`models.md:44`) | General + Coding + Agent |
| `anthropic/claude-haiku-4-5` | Claude Haiku 4.5 | "Anthropic 最快模型,接近前沿智能"(`models.md:37`) | Yes(`models.md:43`) | No(`models.md:44`) | General |
| `anthropic/claude-opus-4-7` | Claude Opus 4.7 | "Anthropic 上代旗舰(legacy)" | No(`models.md:93`) | Yes(`models.md:94`) | General + Coding + Reasoning |
| `anthropic/claude-opus-4-6` | Claude Opus 4.6 | "Anthropic 上代旗舰(legacy)" | Yes(`models.md:93`) | Yes(`models.md:94`) | General + Coding |
| `anthropic/claude-sonnet-4-5` | Claude Sonnet 4.5 | "Anthropic 上代主力(legacy)" | Yes(`models.md:93`) | No(`models.md:94`) | General + Coding |

### Thinking 字段映射

- "Adaptive thinking: Yes" → `thinking: Some(ThinkingSpec { max_thinking_tokens: Some(<max_output>) })`(adaptive 模式 thinking budget = max_output)
- "Adaptive thinking: No" + "Extended thinking: Yes" → `thinking: Some(ThinkingSpec { max_thinking_tokens: Some(<max_output>) })`(extended 模式可手动设置 budget,上限同 max_output)
- 两者都 No → `thinking: None`(6 条中不存在)

### Modalities

6 条全部:
- `input_modalities: &[Modality::Text, Modality::Image]`
- `output_modalities: &[Modality::Text]`

### max_input_tokens

按"`max_input = context_window - max_output`"计算填入。例如 opus-4-8: `1_000_000 - 128_000 = 872_000`。

**实际上 Anthropic 是池子型(input + output 共享 context window),`max_input_tokens` 等于 `context_window` 才更准确**。但当 output 占满 max_output 时 input 不能超过差值,所以填差值是保守安全的。

**约束**:`max_input_tokens` 字段语义需要在 `LlmModelEntry` 的字段 doc comment 里明确:
> "在 max_output_tokens 用满前提下,允许的最大输入 tokens。Anthropic / OpenAI 等池子型模型,实际上 input + output ≤ context_window;此字段取保守下限。"

### Pricing

不动(已是真实数据)。

## 不在本 issue 范围

- 改 ModelPricing 字段
- 加测试(006 处理)
- 清理 `pricing.rs` / `supports_adaptive()` 里的 fable-5/mythos-5 分支(保留无害)

## 验收标准

- [ ] `anthropic_models()` 中 `anthropic/claude-fable-5` 条目已删除
- [ ] `catalog/tests.rs` 中所有 claude-fable-5 相关断言已删除
- [ ] `list_models().filter(|m| m.provider == "anthropic").count()` == 6
- [ ] 6 条 anthropic entry 的 `description` 非空,内容在 6-30 中文字符之间
- [ ] 6 条 `input_modalities` 含 `Text` 和 `Image`
- [ ] 6 条 `output_modalities` 仅 `Text`
- [ ] 6 条 `thinking` 与 `models.md` 的 extended/adaptive 矩阵一致
- [ ] 6 条 `scenes` 至少含 `General`
- [ ] opus 系列 `scenes` 含 `Reasoning`
- [ ] sonnet 系列与 opus 系列 `scenes` 含 `Coding`
- [ ] `max_input_tokens` = `context_window - max_output_tokens`
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 注意事项

- claude-fable-5 / claude-mythos-5 已下架,**从 catalog 删除**。`pricing.rs` 与 `supports_adaptive()` 里的模型名分支保留(未来重新上架可立即生效)
- `description` 用中文,与 `display_name` 互补:display_name 是 marketing,description 是技术特点
- `scenes` 不要每条都填全 5 项;每条选 1-4 项,**最能代表它**的
