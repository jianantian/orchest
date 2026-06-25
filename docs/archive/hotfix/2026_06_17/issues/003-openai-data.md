# Issue 003:OpenAI 模型条目数据填充

## 背景

issue 001 落地后,4 条 openai 模型字段全为占位值。本 issue 把它们填实。

**源**: https://developers.openai.com/api/docs/models/all (catalog 现有注释指向)

## 范围

`openai_models()` 内 4 条 entry。

### 目标数据

| model_id | description | input modalities | output modalities | thinking | scenes |
|---|---|---|---|---|---|
| `openai/gpt-5.5` | "OpenAI 旗舰,综合能力 SOTA" | Text + Image + Audio | Text + Audio | `Some(?)` | General + Coding + Reasoning |
| `openai/gpt-5.4` | "OpenAI 主力,推理 + 多模态平衡" | Text + Image | Text | `Some(?)` | General + Coding + Reasoning |
| `openai/gpt-5.4-mini` | "OpenAI 经济款,响应快" | Text + Image | Text | `Some(?)` | General + Coding |
| `openai/gpt-5.4-nano` | "OpenAI 极致经济款" | Text | Text | `Some(?)` | General |

### Thinking

OpenAI 通过 `reasoning_effort` 控制 thinking,见 `crates/agent-runtime-providers/src/providers/openai/request.rs:182-193`。所有 GPT-5.x 模型支持 reasoning,thinking budget 上限 OpenAI 未公开具体数字。

**填法**: `thinking: Some(ThinkingSpec { max_thinking_tokens: <保守值> })`,保守值取 `max_output_tokens` 的 50%(经验,无明确源)。

**或者**:在 OpenAI 未公开的情况下,`max_thinking_tokens` 字段填 `0` 表示"上限未知"——但这违背字段语义("0 = 不支持 thinking" vs "Some(0) = 支持但上限未知"会混淆)。

**最终决策**: `thinking: None` 表示"不在 catalog 这一层暴露 thinking 信息(因为上限未知)",**在 description 里说明**支持 reasoning。**或者** `Some(ThinkingSpec { max_thinking_tokens: u32::MAX })` 表"上限未知,实际由 provider 内部限制"。

→ **推荐 `Some(ThinkingSpec { max_thinking_tokens: u32::MAX })`**,语义是"支持,无 catalog 已知上限"。在 `ThinkingSpec` doc comment 里说明 `u32::MAX` 的特殊语义。**或者**给 `ThinkingSpec` 加一个 `max_thinking_tokens: Option<u32>` 字段(`None = 上限未公开`)——这是**修订 issue 001 的小变更**,需要回头改。

→ **建议** issue 001 把 `ThinkingSpec` 字段改为 `pub max_thinking_tokens: Option<u32>`:

```rust
pub struct ThinkingSpec {
    /// thinking 内容 token 上限。`None` = 供应商未公开
    pub max_thinking_tokens: Option<u32>,
}
```

OpenAI 4 条全填 `Some(ThinkingSpec { max_thinking_tokens: None })`。

### Modalities

| model | input | output |
|---|---|---|
| gpt-5.5 | Text + Image + Audio | Text + Audio |
| gpt-5.4 | Text + Image | Text |
| gpt-5.4-mini | Text + Image | Text |
| gpt-5.4-nano | Text | Text |

`gpt-5.5` 是否真支持 Audio output 需要源验证;若文档不可达,先填 `Text + Image` 输入、`Text` 输出,标 FIXME。

### max_input_tokens

`Some(context_window)`(OpenAI 池子型)。

## 验收标准

- [ ] 4 条 openai entry `description` 非空
- [ ] 4 条 `thinking: Some(ThinkingSpec { max_thinking_tokens: None })`
- [ ] gpt-5.4-nano `input_modalities` 仅 `Text`
- [ ] 其余 3 条 `input_modalities` 至少含 `Text` + `Image`
- [ ] 4 条 `scenes` 至少含 `General`
- [ ] gpt-5.5 / gpt-5.4 `scenes` 含 `Reasoning`
- [ ] gpt-5.4-mini / gpt-5.5 / gpt-5.4 `scenes` 含 `Coding`
- [ ] gpt-5.4-nano `scenes` 仅 `General`
- [ ] 不确定的字段标 `FIXME(catalog)` 并指明缺失什么源
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 依赖

- 依赖 issue 001 的 `ThinkingSpec` 修订(`max_thinking_tokens: Option<u32>`)
