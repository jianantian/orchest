# 003 · Crazyrouter GPT Image Adapter

## Background

Crazyrouter exposes GPT Image through OpenAI-style image generation and edit endpoints. It is the narrowest provider surface and is a good first provider slice after the public types and storage foundations are in place.

## Goal

Implement a Crazyrouter `ImageProvider` adapter that maps normalized generation/edit requests into Crazyrouter GPT Image API calls and returns provider-layer assets without persisting or exposing public output URLs.

## Acceptance Criteria

### Adapter construction

- [ ] `CrazyrouterImageAdapter` exists under the AIGC providers crate.
- [ ] The adapter implements `ImageProvider`.
- [ ] The adapter accepts config for model, API key, API URL override, and timeout.
- [ ] Default image base URL uses the documented Crazyrouter image route host.
- [ ] The adapter uses the crate shared HTTP client.

### Generation mapping

- [ ] `ImageOperation::TextToImage` maps to `/v1/images/generations`.
- [ ] `prompt` maps to Crazyrouter `prompt`.
- [ ] `count` maps to `n` and validates the documented range.
- [ ] `ImageSize::Auto` and `ImageSize::Pixels` map to Crazyrouter `size`.
- [ ] Quality, background, output format, output compression, moderation, stream, partial images, and user map where documented.
- [ ] Unsupported or rejected fields produce `OptionAdjustment` in coerce mode or stable errors in strict mode.

### Edit mapping

- [ ] `ImageOperation::ImageToImage` and `EditImage` map to `/v1/images/edits`.
- [ ] Source/reference images map to multipart `image[]`.
- [ ] Mask input maps to multipart `mask`.
- [ ] Region edits are rejected until mask synthesis exists.
- [ ] The adapter enforces documented max reference image count.

### Response and events

- [ ] Response `data[].url` becomes `ProviderAsset { source: AssetIngestSource::Url, ... }`.
- [ ] The adapter does not call `AssetStore`.
- [ ] The adapter never returns public `GeneratedImage` values.
- [ ] Streaming partial images are represented as `ProviderImageEvent::PartialAsset` when available.

### Tests

- [ ] Request construction tests cover generation.
- [ ] Multipart request construction tests cover single-image edit, mask edit, and multi-reference edit.
- [ ] Compatibility tests cover `quality=hd`, `quality=standard`, transparent background, png compression, and unsupported style/input fidelity.
- [ ] Response parsing tests cover one and multiple generated URLs.
- [ ] Streaming parser tests cover partial image events if documented response fixtures are available.
- [ ] `cargo test -p agent-runtime-aigc-providers crazyrouter` passes.

## Blocked By

- Issue 001
