# Issue 005:Volcengine 模型条目数据填充

## 背景

issue 001 落地后,6 条 volcengine 模型字段全为占位值。本 issue 把它们填实。

**源**:
- `docs/external/volceengine/llm/api.md`
- `docs/external/volceengine/llm/think.md`
- `docs/external/volceengine/llm/multimodal.md`
- `docs/external/volceengine/llm/{image,video,audio,doc}_understand.md`

## 范围

`volcengine_models()` 内 6 条 entry。

### 目标数据

| model_id | display_name | description | scenes |
|---|---|---|---|
| `volcengine/doubao-seed-2-0-pro-260215` | Doubao Seed 2.0 Pro | "豆包旗舰,默认开思考,多模态全支持(图片/视频/音频/文档)" | General + Reasoning + Coding + Agent |
| `volcengine/doubao-seed-2-0-lite-260215` | Doubao Seed 2.0 Lite | "豆包均衡款,128K 上下文,默认开思考" | General + Coding |
| `volcengine/doubao-seed-2-0-mini-260215` | Doubao Seed 2.0 Mini | "豆包经济款,响应快" | General |
| `volcengine/doubao-seed-2-0-lite-260428` | Doubao Seed 2.0 Lite (260428) | "豆包均衡款 428 版,带 thinking summary" | General + Coding |
| `volcengine/doubao-seed-2-0-mini-260428` | Doubao Seed 2.0 Mini (260428) | "豆包经济款 428 版,带 thinking summary" | General |
| `volcengine/doubao-seed-1-6-flash-250615` | Doubao Seed 1.6 Flash | "豆包 1.6 极速款,128K 上下文(legacy)" | General |

### Thinking

doubao-seed 系列**默认开思考**(`docs/external/volceengine/llm/think.md:1, 12`)。`crates/agent-runtime-providers/src/providers/volcengine/mod.rs:91-93` 的 `supports_thinking()` 实现是判断依据(`doubao-seed-*` 前缀,排除 `character` 角色扮演模型)。

**注意**: `think.md:12` 明确"250615 及之后版本的大语言模型,如无特殊说明,默认支持 Responses API",示例中用 `doubao-seed-1-6-251015` 演示 thinking 能力。**doubao-seed-1.6-flash-250615 也支持 thinking**,与 doubao-seed-2.0 系列同口径。

| model | thinking |
|---|---|
| doubao-seed-2-0-pro-260215 | `Some(ThinkingSpec { max_thinking_tokens: None })` |
| doubao-seed-2-0-lite-260215 | `Some(ThinkingSpec { max_thinking_tokens: None })` |
| doubao-seed-2-0-mini-260215 | `Some(ThinkingSpec { max_thinking_tokens: None })` |
| doubao-seed-2-0-lite-260428 | `Some(ThinkingSpec { max_thinking_tokens: None })` |
| doubao-seed-2-0-mini-260428 | `Some(ThinkingSpec { max_thinking_tokens: None })` |
| doubao-seed-1-6-flash-250615 | `Some(ThinkingSpec { max_thinking_tokens: None })` |

### Modalities

doubao-seed-2.0 系列支持多模态输入(`multimodal.md` 内文档)。1.6-flash 是否支持需要验证。

| model | input | output |
|---|---|---|
| doubao-seed-2-0-pro-260215 | Text + Image + Video + Audio | Text |
| doubao-seed-2-0-lite-260215 | Text + Image | Text |
| doubao-seed-2-0-mini-260215 | Text + Image | Text |
| doubao-seed-2-0-lite-260428 | Text + Image | Text |
| doubao-seed-2-0-mini-260428 | Text + Image | Text |
| doubao-seed-1-6-flash-250615 | Text | Text |

**FIXME**: pro 是否真支持 audio output 待源验证。lite/mini 是否支持 video 待源验证。不能确认的填保守值并加 `// FIXME(catalog)` 注释指出待补的源。

### max_input_tokens

`Some(context_window - max_output)` = `Some(128_000 - 16_384)` = `Some(111_616)`。

### 现有字段不动

`context_window: 128_000` / `max_output_tokens: Some(16_384)` / `pricing` 全保持。

## 验收标准

- [ ] 6 条 volcengine entry `description` 含"豆包"
- [ ] 6 条 doubao-seed 系列 `thinking` 非 `None`(含 1.6-flash)
- [ ] doubao-seed-2-0-pro `input_modalities` 含 `Image`(必)、`Video` / `Audio`(尽量)
- [ ] doubao-seed-1-6-flash `input_modalities` 仅 `Text`
- [ ] pro `scenes` 含 `Reasoning` + `Coding` + `Agent`
- [ ] lite 系列 `scenes` 含 `Coding`
- [ ] mini / 1.6-flash `scenes` 仅 `General`
- [ ] 不确定的字段标 `FIXME(catalog)` 并指明待补的具体源(例如 `// FIXME(catalog): pro audio output unconfirmed in volceengine/llm/audio_understand.md`)
- [ ] `max_input_tokens: Some(111_616)`
- [ ] `cargo test -p agent-runtime-providers --lib catalog` 通过

## 依赖

- 依赖 issue 001
