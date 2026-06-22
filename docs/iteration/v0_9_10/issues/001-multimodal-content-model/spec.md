# 001 · 多模态 content model 地基

## 背景

Minimax LLM(Anthropic 兼容)支持 image / video content block、若干 Minimax-only role、以及
`service_tier` 选项。这些落在 **`agent-runtime-model`** 的共享类型上,是后续 `MinimaxAdapter`
(issue 002)和未来 omni(`docs/todo/provider-unification.md` Step 2)的数据地基。

当前类型(已核对):
- `ContentBlock`(`crates/agent-runtime-model/src/types.rs:25-41`)只有 4 variant:
  `Text(String)` / `Thinking` / `ToolUse` / `ToolResult` —— **无 image / video / audio**。
- `Role`(`types.rs:16-22`)只有 `System` / `User` / `Assistant` / `Tool`。
- `RequestOptions`(`src/options.rs:46-56`)无 `service_tier`。
- **无 `MediaSource` 类型**。

这是一次 **SDK 级破坏性改动**:`ContentBlock` / `Role` 加 variant 后,所有现有 LLM provider
(anthropic / openai / deepseek / openrouter / volcengine)的穷尽 `match` 必须处理新 variant
(各自 `request.rs` 内均有 `ContentBlock::` match)。决策见设计文档 §七 Q1,采用 **选项 A**。

设计来源:[`docs/research/minimax-api-analysis.md`](../../../../research/minimax-api-analysis.md) §二 2.3 / §七 Q1。

## 1a. 新增 `MediaSource` 类型

`crates/agent-runtime-model/src/types.rs`。覆盖 Minimax / Anthropic 两种 image source 形态:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MediaSource {
    /// 远程 URL(Minimax `image_url` / Anthropic `{type:url}`)
    Url(String),
    /// base64 内联(Anthropic `{type:base64, media_type, data}` / `data:...;base64,`)
    Base64 { media_type: String, data: String },
}
```

## 1b. `ContentBlock` 扩展(加 4 variant)

| variant | 字段 | 锚点 |
|---|---|---|
| `Image` | `{ source: MediaSource, detail: Option<String> }` | `llm.md:843,1188` |
| `Video` | `{ source: MediaSource, fps: Option<f32>, detail: Option<String>, max_long_side_pixel: Option<u32> }`(`fps`/`max_long_side_pixel` 为 Minimax 专属) | `llm.md:843,1188,1334-1343` |
| `Audio` | `{ source: MediaSource }` | 无 Minimax 锚点 —— **Step 2 omni 前向占位**(PRD delta 3) |
| `MidConvSystem` | `(String)` | `llm.md:1136,1202-1211` |

## 1c. `Role` 扩展(加 4 Minimax-only variant)

`UserSystem` / `Group` / `SampleMessageUser` / `SampleMessageAi`(锚点 `llm.md:1088-1091`)。

> NOTE: 这 4 个 role 是 Minimax 专属,只有 `MinimaxAdapter`(002)序列化它们。把 vendor-specific
> role 放进共享 `Role` 是设计文档 §2.3 的决策;Step 3 provider 统一时复核"vendor 角色是否该留在
> 共享 model"(`docs/todo/provider-unification.md`)。

## 1d. `RequestOptions.service_tier`

`src/options.rs`:加 `pub service_tier: Option<String>`(`standard` | `priority`,锚点
`llm.md:807,360,548`),更新手写 `Default`(`None`)。

> 命名冲突警告:`agent-runtime-aigc-providers/src/types/video.rs:88` 已有同名
> `service_tier`,语义是"图片/视频生成优先级",与此处 LLM 的 `standard|priority` **不同语义**。
> 在本字段 doc 注释里明示,二者不互借(设计文档 §2.3)。

## 1e. 现有 LLM adapter 处理新 variant(§七 Q1-A)

新 variant 加入后,5 个 provider 的穷尽 match 编译会断。处理规则:

- **content block**:不支持的 `Image`/`Video`/`Audio`/`MidConvSystem` → 丢弃该 block 并记录
  `OptionAdjustment`(对齐现有 `thinking_budget_tokens` unsupported 路径)。
- **例外 —— Anthropic 的 `Image`**:Anthropic Messages API 原生支持 image,**顺手实现真序列化**
  (`{type:image, source:{...}}`)。`Video`/`Audio`/`MidConvSystem` 在 Anthropic 仍走 OptionAdjustment。
  OpenAI vision 的 image 实现留作后续 follow-up,不在本迭代。
- **新 role**:非 Minimax adapter 把 `UserSystem`→`system` 语义、其余 3 个 → `user` 语义映射,
  并记 `OptionAdjustment`(few-shot sample 角色在非 Minimax 上无对应)。

## 验收标准

- [ ] `agent-runtime-model` 有 `MediaSource { Url, Base64 }`
- [ ] `ContentBlock` 含 `Image` / `Video` / `Audio` / `MidConvSystem` 四个新 variant,字段如 1b
- [ ] `Role` 含 `UserSystem` / `Group` / `SampleMessageUser` / `SampleMessageAi`
- [ ] `RequestOptions.service_tier: Option<String>` 存在,`Default` 为 `None`,doc 注释标注与 aigc 同名字段的语义区别
- [ ] anthropic adapter 对 `ContentBlock::Image` 输出真实 Anthropic image 序列化,有单元测试断言 JSON 形状
- [ ] openai / deepseek / openrouter / volcengine 对新 content variant 走 `OptionAdjustment` 丢弃,不 panic
- [ ] 5 个 adapter 对新 `Role` variant 编译通过(穷尽 match 不留 `todo!()`/`unimplemented!()`)
- [ ] `cargo test -p agent-runtime-model -p agent-runtime-providers` 全绿
- [ ] `cargo clippy -p agent-runtime-model -p agent-runtime-providers -- -D warnings` 无 warning
