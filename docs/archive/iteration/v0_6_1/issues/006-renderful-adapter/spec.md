# 006 · Renderful Image Adapter

## Background

Renderful is already a unified AIGC generation gateway. Its API is async task based and exposes model capability metadata. The Orchest adapter should treat Renderful like any other provider while preserving Renderful's task lifecycle and model metadata.

## Goal

Implement a Renderful `ImageProvider` adapter for text-to-image, image-to-image, and image-related task polling through the `/api/v1/generations` API.

## Acceptance Criteria

### Adapter construction

- [ ] `RenderfulImageAdapter` exists and implements `ImageProvider`.
- [ ] Config supports model, API key, API URL override, timeout, and optional webhook behavior.
- [ ] The adapter uses `/api/v1/generations` as the primary API.
- [ ] Legacy `/v1/predictions` references are not part of the public adapter contract.
- [ ] The adapter uses the crate shared HTTP client.

### Capability metadata

- [ ] The adapter can parse `GET /api/v1/models?type=text-to-image`.
- [ ] The adapter can parse `GET /api/v1/models?type=image-to-image`.
- [ ] Capabilities include aspect ratios, resolutions, max outputs, cost ranges, and webhook support when returned.
- [ ] Missing model metadata is handled as static or assumed capability according to compatibility policy.

### Request mapping

- [ ] `TextToImage` maps to `type: "text-to-image"`.
- [ ] `ImageToImage` and `EditImage` map to documented Renderful image task types when supported by the selected model.
- [ ] Prompt, model, and webhook URL map directly.
- [ ] Inputs that need URLs are accepted only after gateway/input preprocessing has converted them to provider-usable URLs or data URLs.
- [ ] Unresolved local paths or stored asset refs return a stable adapter error instead of making the adapter call `AssetStore`.
- [ ] Unsupported operations such as `Upscale` and `FaceSwap` are represented but not exposed as first milestone tools unless explicitly enabled.

### Task lifecycle

- [ ] Create returns provider job id and queued/running status.
- [ ] Poll maps `queued`, `processing`, `completed`, and `failed` to provider statuses.
- [ ] Completed `outputs[]` URLs become provider asset ingestion sources.
- [ ] Failed tasks preserve provider error details.

### Tests

- [ ] Model metadata parsing tests cover text-to-image and image-to-image fixtures.
- [ ] Create request mapping tests cover text-to-image and image-to-image.
- [ ] Polling tests cover queued, processing, completed, failed, and timeout flows.
- [ ] Input preprocessing tests cover already-resolved URL/data URL inputs and stable rejection of unresolved local or stored inputs.
- [ ] `cargo test -p agent-runtime-aigc-providers renderful` passes.

## Blocked By

- Issue 001
