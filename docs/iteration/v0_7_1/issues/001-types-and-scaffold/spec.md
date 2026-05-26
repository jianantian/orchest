# 001 · Crate Scaffold and Image Gateway Public Types

## Background

v0.7.1 introduces a standalone image AIGC provider crate. All later storage, provider adapter, and gateway orchestration work depends on one stable type contract. This issue creates the crate and defines the public request, response, capability, event, error, and provider-boundary types without implementing provider HTTP calls or asset storage.

The source of truth is the Image AIGC Gateway design document at `docs/superpowers/specs/2026-05-25-image-aigc-gateway-design.md`.

## Goal

Create `agent-runtime-aigc-providers` with zero workspace-internal dependencies and a compiling public type surface for image generation/editing.

## Acceptance Criteria

### Crate scaffold

- [ ] `crates/agent-runtime-aigc-providers/Cargo.toml` exists.
- [ ] The crate is added to the root workspace members.
- [ ] The crate has no workspace-internal path dependencies.
- [ ] Initial dependencies are limited to what the type surface and later lightweight HTTP adapters need: `tokio`, `serde`, `serde_json`, `async-trait`, `thiserror`, `reqwest`, `bytes`, `chrono`, `tracing`, and `metrics` unless a new dependency is explicitly justified.
- [ ] `src/lib.rs`, `src/types.rs`, `src/image.rs`, and module stubs for `providers`, `storage`, `http`, and `telemetry` exist.
- [ ] `src/http.rs` exposes a shared `reqwest::Client` helper usable by provider adapters.
- [ ] Multiple calls to the shared HTTP client helper return the same client instance.
- [ ] `cargo check -p agent-runtime-aigc-providers` passes.

### Provider boundary types

- [ ] `ImageProvider` trait exists with `provider_name()`, `model_name()`, `capabilities()`, `create_image_generation()`, and `get_image_generation()`.
- [ ] `ImageProvider` returns provider-layer types, not public output types.
- [ ] `ProviderImageJob`, `ProviderAsset`, `ProviderImageEvent`, and `ProviderGenerationStatus` exist.
- [ ] `ProviderGenerationStatus` does not include gateway-only states such as `PersistingAssets`.

### Public request types

- [ ] `ImageGenerationRequest` includes operation, prompt, negative prompt, inputs, generation config, execution config, output config, compatibility policy, and provider options.
- [ ] `ImageOperation` includes `TextToImage`, `ImageToImage`, `EditImage`, `Upscale`, and `FaceSwap`.
- [ ] `ImageInput`, `ImageInputRole`, and `AssetRef` represent source images, references, masks, fonts, super-resolution references, URLs, data URLs, base64, bytes, local paths, and stored assets.
- [ ] `ImageGenerationConfig`, `ImageSize`, `ImageQuality`, `ImageFormat`, `ImageBackground`, `SafetyConfig`, `ImageEditConfig`, `ImageRegion`, and `ImageStyleConfig` exist.
- [ ] `GenerationExecutionConfig` represents async preference, streaming, partial image count, poll interval, timeout, webhook URL, and user identifier.
- [ ] `ImageOutputConfig` and `ImageOutputDelivery` represent `Url` and `Base64` delivery.

### Public response types

- [ ] `ImageGenerationResponse` and `GeneratedImage` exist.
- [ ] `GeneratedImage.asset_id` is top-level and present for both `Url` and `Base64` delivery.
- [ ] `ImageOutput` has only `Url(ImageUrlOutput)` and `Base64(ImageBase64Output)` variants.
- [ ] `ImageUrlOutput` contains an immediately usable URL and optional expiration, but does not expose bucket, endpoint, object key, or storage credentials.
- [ ] `GenerationStatus` includes gateway states, including `PersistingAssets`.
- [ ] `ImageGenerationEvent` is clearly gateway-public and does not appear in the provider trait.

### Capabilities, errors, and usage

- [ ] `ImageModelCapabilities` contains per-operation `ImageOperationCapability` entries.
- [ ] `GenerationExecutionMode` represents sync, async, and stream modes.
- [ ] `CompatibilityPolicy` and `OptionAdjustment` exist.
- [ ] `AigcError` preserves normalized message/code/provider/status and upstream error details.
- [ ] `ImageUsage` exists with fields sufficient for image count, dimensions, provider details, and optional cost metadata.
- [ ] `AigcProviderRuntimeConfig` exists with provider, model, API key, API key env, API URL, region, timeout, and provider options fields.
- [ ] Types crossing SDK boundaries derive `Serialize` and `Deserialize` where practical.

### Tests

- [ ] Serde round-trip tests cover `ImageGenerationRequest`, `ImageGenerationResponse`, `ProviderImageJob`, `GeneratedImage`, and `ImageModelCapabilities`.
- [ ] Tests verify `GeneratedImage.asset_id` exists for both URL and Base64 outputs.
- [ ] Tests verify public URL output types do not contain bucket, object key, endpoint, access key, or provider URL fields.
- [ ] Tests verify the shared HTTP client helper is a singleton.
- [ ] `cargo test -p agent-runtime-aigc-providers` passes.
- [ ] `cargo clippy -p agent-runtime-aigc-providers -- -D warnings` passes.

## Notes

- Do not implement provider adapters, storage backends, or gateway orchestration in this issue.
- Keep module files focused; avoid putting business logic in `mod.rs`.
