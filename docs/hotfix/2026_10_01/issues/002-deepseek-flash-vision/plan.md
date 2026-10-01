# deepseek-flash 图像输入 Implementation Plan

## Files to Read

- `crates/orchest-provider-http/src/chat.rs`（`build_request_body` User 分支、`record_dropped_block`）
- `crates/orchest-provider-http/src/protocol.rs`（`ProviderProfile`、`resolve_chat_preflight`）
- `crates/orchest-provider-http/src/providers/anthropic/profile.rs`（`encode_multimodal_block` 的 drop 约定）
- `crates/orchest-provider-http/src/providers/openai/tests.rs`（C5 丢弃测试）
- `docs/adr/0002-protocol-provider-decoupling.md`（rule 3 / rule 4）

## Files to Change

- `crates/orchest-provider-http/src/protocol.rs`：`ImageInputSupport`、`chat_image_input` hook、
  `resolve_chat_content_preflight`
- `crates/orchest-provider-http/src/chat.rs`：canonical `image_url` 编码 + 预检接线
- `crates/orchest-provider-http/src/providers/deepseek/{profile.rs,tests.rs}`
- `crates/orchest-provider-http/src/catalog/{mod.rs,tests.rs}`
- `crates/orchest-provider/tests/selection.rs` 或 `catalog_discovery.rs`（registry 模态查询）
- `CHANGELOG.md`

## Steps

1. 先写失败测试：DeepSeek 请求体编码、Strict 预检、Coerce 丢弃、别名、mock HTTP 往返、catalog 模态。
2. `protocol.rs` 增加 `ImageInputSupport` 与默认 hook；`resolve_chat_content_preflight(profile, cx, messages, policy)`。
3. `chat.rs` User 分支：hook `Supported` 时把 `Image` 编码为 `image_url` part，按序组装 part 数组；
   无图像保留字符串；不支持的 `MediaSource` 变体记录丢弃。`complete()` 与 `request_body_for_test` 调用预检。
4. `DeepSeekProfile::chat_image_input`：catalog `input_modalities` 优先，名称兜底仅限 `deepseek-v4-flash*`。
5. catalog `deepseek-flash` 加 `Modality::Image`。
6. CHANGELOG；fmt / clippy / test / lint-check / doc。
