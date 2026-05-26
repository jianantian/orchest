# 005 · OpenRouter Image Adapter

## Background

OpenRouter exposes image generation through chat completions/responses rather than image-specific endpoints. Its model metadata and `image_config` provide useful capability information, while image input / image-to-image behavior is model-specific and should not be guessed.

## Goal

Implement an OpenRouter `ImageProvider` adapter for documented image-output generation, model capability discovery, streaming image deltas, and `image_config` mapping.

## Acceptance Criteria

### Adapter construction

- [ ] `OpenRouterImageAdapter` exists and implements `ImageProvider`.
- [ ] Config supports model, API key, API URL override, timeout, app title, and site URL-style headers where applicable.
- [ ] The adapter uses the crate shared HTTP client.

### Capability discovery

- [ ] The adapter can parse OpenRouter model metadata for image output models.
- [ ] Capability metadata records whether a model is image-only or text+image when available.
- [ ] Aspect ratio, image size, and known model-specific config support are reflected in per-operation capabilities.
- [ ] Strict mode does not rely on assumed image input capabilities.

### Request mapping

- [ ] Text-to-image requests use `/api/v1/chat/completions` for the first implementation.
- [ ] Prompt maps to a user message.
- [ ] `modalities` includes `image` and includes `text` only when the selected model supports/needs text output.
- [ ] `AspectRatio` maps to `image_config.aspect_ratio`.
- [ ] `ResolutionTier` maps to `image_config.image_size`.
- [ ] Recraft/Sourceful documented style fields map into `image_config`.
- [ ] Image input / image-to-image requests are rejected unless a selected model has verified official input mapping or the caller uses explicit provider options.

### Response and streaming

- [ ] Non-streaming `message.images[]` parses into provider assets.
- [ ] Streaming `delta.images[]` parses into `ProviderImageEvent::PartialAsset`.
- [ ] Base64 data URLs become `AssetIngestSource::DataUrl`.
- [ ] Assistant text content is preserved in provider metadata when returned.

### Tests

- [ ] Request mapping tests cover image-only and text+image modality choices.
- [ ] `image_config` mapping tests cover aspect ratio, image size, strength, text layout, style, colors, font inputs, and super-resolution references.
- [ ] Response parsing tests cover non-streaming images.
- [ ] Streaming parsing tests cover `delta.images`.
- [ ] Strict compatibility tests reject unverified image-to-image mapping.
- [ ] `cargo test -p agent-runtime-aigc-providers openrouter` passes.

## Blocked By

- Issue 001
- Issue 002

