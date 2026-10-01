# deepseek-flash 图像输入（Chat `image_url` content part）

## Background

[DeepSeek 图像理解指南](https://api-docs.deepseek.com/guides/vision) 与
[模型 & 价格](https://api-docs.deepseek.com/zh-cn/quick_start/pricing)（「图像理解」行：
`deepseek-flash` 支持、`deepseek-v4-pro` 不支持）确认：`deepseek-flash` 在 OpenAI 兼容 Chat
Completions 上接受图像，user 消息 `content` 为数组：

```json
[
  {"type": "text", "text": "What is in this image?"},
  {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,<BASE64>", "detail": "low"}}
]
```

`url` 可为 `http(s)` 外链或 `data:` URL；`detail` 可选 `low` / `high` / `original` / `auto`。
格式 JPEG/PNG/GIF/WebP；外链 ≤ 8192 字符、请求体 ≤ 48 MiB、单图 ≤ 32 MiB、每请求 ≤ 600 张、
单边 ≤ 8192 px；图像只能出现在 `user` 消息，`system` / `assistant` 中出现返回 400。旧名
`deepseek-v4-flash-vision-exp` / `deepseek-v4-flash` 路由到 V4.1 Flash，同样支持图像。

Orchest 已有公开的图像输入通路：`orchest-protocol` 的 `ContentBlock::Image { source: MediaSource, detail }`
（`RunInput` 自 hotfix 2026-07-02 起可携带图片），catalog `input_modalities` 投影为
`CapabilityDescriptor` 的输入模态。Messages 侧（Anthropic / Minimax）经 `encode_multimodal_block`
编码图像；但共享 Chat 核（`crates/orchest-provider-http/src/chat.rs`）对所有 Chat provider 丢弃 `Image`
并记录 `chat_unsupported_content_block`，DeepSeek 无法收到图片。

`image_url` 是 OpenAI Chat 的**协议标准**编码，不是 DeepSeek 的方言分叉（ADR-0002 rule 4），因此编码放在
共享 Chat 核；是否对某模型启用由 profile 依据 catalog 能力事实声明（ADR-0002「profiles read capability
facts from the resolution」）。

## Goal / Scope

- `ProviderProfile` 新增具名 hook `chat_image_input(cx) -> ImageInputSupport`（crate 内部，非公开 API）：
  默认 `Unsupported { strict_error: None }`，即保持所有现有 Chat provider 的丢弃行为不变。
- 共享 Chat 核：当 hook 返回 `Supported` 时，把 user 消息中的 `Image` block 编码为 canonical
  `image_url` part（`MediaSource::Url` → `url`，`MediaSource::Base64` → `data:<media_type>;base64,<data>`，
  `detail` 透传），user `content` 变为按原顺序排列的 `text` / `image_url` part 数组；无图像时 `content`
  仍为原字符串（逐字节不变）。
- Chat 预检：请求含 `Image` block 而 hook 返回 `Unsupported { strict_error: Some(..) }` 且策略为
  `Strict` 时，发请求前返回 `ModelError`；`Coerce` 下逐 block 可见丢弃（既有 `OptionAdjustment`）。
- `DeepSeekProfile` 覆盖该 hook：catalog 行 `input_modalities` 含 `Image` → `Supported`；无 catalog 行时
  名称兜底仅对官方路由别名 `deepseek-v4-flash*` 返回 `Supported`；其余返回
  `Unsupported { strict_error: Some(("unsupported_image_input", ..)) }`。
- catalog：`deepseek/deepseek-flash` `input_modalities = [Text, Image]`；`deepseek/deepseek-v4-pro`
  保持 `[Text]`。

## Acceptance Criteria

- [x] `find_model("deepseek-flash")` 的 `input_modalities` 含 `Modality::Image`，`deepseek-v4-pro` 不含；
      其 `CapabilityDescriptor` 输入模态同步，`orchest-provider` registry `.chat().accepts([Image])`
      可选中 `deepseek/deepseek-flash`、不会选中 `deepseek/deepseek-v4-pro`。
- [x] `deepseek-flash` 请求体：user 消息 `[Text, Image(Url, detail=low), Image(Base64 png)]` 编码为
      `[{"type":"text",..}, {"type":"image_url","image_url":{"url":"https://..","detail":"low"}},
      {"type":"image_url","image_url":{"url":"data:image/png;base64,.."}}]`，顺序保持，无丢弃 adjustment。
- [x] 纯文本 user 消息的 `content` 仍为字符串（与本 hotfix 前逐字节一致）。
- [x] `system` / `assistant` 中的 `Image` block 仍被丢弃并记录 `chat_unsupported_content_block`；
      与 `ToolResult` 混排的 `Image` 同样可见丢弃；`Video` / `Audio` 仍丢弃。
- [x] `deepseek-v4-pro` + `Image` + `CompatibilityPolicy::Strict`：`complete()` 在任何 HTTP 请求前返回
      `code = "unsupported_image_input"` 的 `ModelError`；`Coerce`：请求成功（mock SSE），图像被丢弃并记录
      `chat_unsupported_content_block`，`content` 为字符串。
- [x] 路由别名 `deepseek-v4-flash` 同样编码 `image_url`；`deepseek-flash` 经 mock HTTP 服务端的完整
      `complete()` 往返成功，服务端收到的请求体含 `image_url` part。
- [x] OpenAI / Volcengine / OpenRouter / Elss-chat 行为不变：既有 C5 丢弃测试保持通过，
      即使 `Strict` 也不因图像报错。
- [x] 不新增 Supported crate（`orchest` / `orchest-protocol` / `orchest-provider` / `orchest-storage`）公开类型或
      函数；新增 hook 与 `ImageInputSupport` 位于 `orchest-provider-http` 的 `pub(crate)` `protocol` 模块。
- [x] CHANGELOG `[Unreleased]` 记录 DeepSeek 图像输入（Added）与新错误码 `unsupported_image_input`。
- [x] `cargo test --workspace`、`cargo clippy --workspace -- -D warnings`、`cargo fmt --check`、
      `bash scripts/lint-check.sh`、`cargo doc` 通过。

## Notes

- 不实现 Files API `file` part、Anthropic 兼容端点或 Responses API `input_image`。
- 不做本地图片大小 / 像素 / 数量 / 格式校验，交由 provider 返回错误（与 hotfix 2026-08-06 ASR 一致）。
- 其余 Chat provider 是否启用 `image_url` 由各自后续 issue 依据官方文档单独决定。
