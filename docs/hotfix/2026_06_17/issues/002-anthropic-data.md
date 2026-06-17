# Issue 002:Anthropic 模型条目数据填充

## 背景

issue 001 落地后,7 条 anthropic 模型字段全为占位值。本 issue 把它们填实。

**源**: `docs/external/anthropic/models.md`(锚点见各表)

## 范围

`anthropic_models()` 内 7 条 entry 全部填新字段。所有 claude 模型按 `models.md:11` 的口径**输入支持 text + image,输出 text**。

### 目标数据

| model_id | display_name | description | extended thinking | adaptive thinking | scenes |
|---|---|---|---|---|---|
| `anthropic/claude-fable-5` | Claude Fable 5 | "Anthropic 实验旗舰,1M 上下文,综合推理与代码 SOTA" | ?(暂沿用 sonnet 4.6 的 supports 标记) | ? | General + Coding + Reasoning |
| `anthropic/claude-opus-4-8` | Claude Opus 4.8 | "Anthropic 旗舰,综合能力最强,1M 上下文" | No(`models.md:43`) | Yes(`models.md:44`) | General + Coding + Reasoning + Agent |
| `anthropic/claude-sonnet-4-6` | Claude Sonnet 4.6 | "Anthropic 主力,平衡性能与成本" | Yes(`models.md:43`) | Yes(`models.md:44`) | General + Coding + Agent |
| `anthropic/claude-haiku-4-5` | Claude Haiku 4.5 | "Anthropic 轻量快速,200K 上下文" | Yes(`models.md:43`) | No(`models.md:44`) | General |
| `anthropic/claude-opus-4-7` | Claude Opus 4.7 | "Anthropic 上代旗舰" | No(`models.md:93`) | Yes(`models.md:94`) | General + Coding + Reasoning |
| `anthropic/claude-opus-4-6` | Claude Opus 4.6 | "Anthropic 上代旗舰(legacy)" | Yes(`models.md:93`) | Yes(`models.md:94`) | General + Coding |
| `anthropic/claude-sonnet-4-5` | Claude Sonnet 4.5 | "Anthropic 上代主力(legacy)" | Yes(`models.md:93`) | No(`models.md:94`) | General + Coding |

### Thinking 字段映射

- "Adaptive thinking: Yes" → `thinking: Some(ThinkingSpec { max_thinking_tokens: <max_output> })`(adaptive 模式 thinking budget = max_output)
- "Adaptive thinking: No" + "Extended thinking: Yes" → `thinking: Some(ThinkingSpec { max_thinking_tokens: <max_output> })`(extended 模式可手动设置 budget,上限同 max_output)
- 两者都 No → `thinking: None`(不存在这种情况;7 条里都至少其一)

claude-fable-5 在 `models.md` 里没列,沿用 opus-4-8 的口径(adaptive yes,extended no),这在源信息缺失时是保守选择。**注释里需要标 `// FIXME(catalog): claude-fable-5 not yet in docs/external/anthropic/models.md`**,等官方文档更新后再修正。

### Modalities

7 条全部:
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

## 验收标准

- [ ] 7 条 anthropic entry 的 `description` 非空,内容在 6-30 中文字符之间
- [ ] 7 条 `input_modalities` 含 `Text` 和 `Image`
- [ ] 7 条 `output_modalities` 仅 `Text`
- [ ] 7 条 `thinking` 与 `models.md` 的 extended/adaptive 矩阵一致
- [ ] 7 条 `scenes` 至少含 `General`
- [ ] 旗舰模型(opus 系列、fable-5)`scenes` 含 `Reasoning`
- [ ] sonnet 系列与 opus 系列 `scenes` 含 `Coding`
- [ ] `max_input_tokens` = `context_window - max_output_tokens`
- [ ] claude-fable-5 entry 含 `FIXME(catalog)` 注释
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 注意事项

- claude-fable-5 在源文档中缺位,本次按 opus-4-8 同口径填,标注 FIXME
- `description` 用中文,与 `display_name` 互补:display_name 是 marketing,description 是技术特点
- `scenes` 不要每条都填全 5 项;每条选 1-4 项,**最能代表它**的
