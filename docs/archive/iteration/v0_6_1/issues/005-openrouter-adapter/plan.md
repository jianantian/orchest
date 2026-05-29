# 005 实现路线

## v0.7 依赖判断

不依赖 v0.7。OpenRouter adapter 只实现 provider HTTP 映射和 provider-layer asset/event 输出，不需要 runtime Hook 或主线扩展框架。

## 步骤

1. **建立 OpenRouter provider 模块**
   - 新建 `crates/agent-runtime-aigc-providers/src/providers/openrouter.rs`
   - 在 `providers/mod.rs` 显式导出 `OpenRouterImageAdapter` 和 config
   - config 字段包括 model、api_key、api_url override、timeout、app title、site URL-style headers
   - HTTP 请求统一使用 crate shared client

2. **实现 capability discovery**
   - 解析 OpenRouter model metadata 中的 image output 信息
   - 记录模型是 image-only 还是 text+image（如果 metadata 提供）
   - aspect ratio、image size、model-specific config 写入 per-operation capabilities
   - Strict mode 下不假设 image input / image-to-image 能力

3. **实现 text-to-image request mapping**
   - 第一版使用 `/api/v1/chat/completions`
   - prompt 映射 user message
   - `modalities` 包含 `image`
   - 只有模型支持或需要 text output 时才包含 `text`
   - `AspectRatio` 映射 `image_config.aspect_ratio`
   - `ResolutionTier` 映射 `image_config.image_size`
   - Recraft/Sourceful 文档化 style fields 映射进 `image_config`

4. **处理 image input / image-to-image**
   - 如果所选模型没有官方确认的 input mapping，Strict 直接拒绝
   - caller 明确使用 `provider_options` 时允许透传文档化字段
   - Coerce 只做安全调整，不能静默伪造未支持能力

5. **实现响应和 streaming 解析**
   - non-streaming `message.images[]` 转为 provider assets
   - streaming `delta.images[]` 转为 `ProviderImageEvent::PartialAsset`
   - base64 data URLs 转为 `AssetIngestSource::DataUrl`
   - assistant text content 保存在 provider metadata

6. **写测试**
   - request mapping：image-only 和 text+image modality choices
   - `image_config`：aspect ratio、image size、strength、text layout、style、colors、font inputs、super-resolution references
   - non-streaming response images
   - streaming `delta.images`
   - Strict reject unverified image-to-image mapping

7. **验收**
   - `cargo test -p agent-runtime-aigc-providers openrouter`
   - 确认没有把 chat completion text response 误当成 public gateway response

## 关键决策

- OpenRouter 的图像能力是模型级差异，不做统一假设。
- adapter 只输出 provider assets 和 metadata；是否持久化、转存和生成公共 URL 由 gateway 决定。
