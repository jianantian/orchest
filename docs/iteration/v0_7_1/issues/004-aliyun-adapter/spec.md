# 004 · Alibaba Cloud DashScope Image Adapter

## Background

Alibaba Cloud DashScope covers multiple image model families with different request shapes and capabilities: Qwen-Image, Z-Image, and Wan. The adapter must preserve those differences behind the normalized provider boundary without pretending they are one identical API.

## Goal

Implement an Aliyun DashScope `ImageProvider` adapter for Qwen-Image, Z-Image, and Wan image generation/editing, including synchronous multimodal calls and documented async task polling where supported.

## Acceptance Criteria

### Adapter construction

- [ ] `AliyunImageAdapter` exists and implements `ImageProvider`.
- [ ] Config supports model, API key, region/endpoint, API URL override, and timeout.
- [ ] Beijing, Singapore, and Virginia-style endpoint selection is represented without mixing API keys across regions.
- [ ] The adapter uses the crate shared HTTP client.

### Qwen and Z-Image mapping

- [ ] Current multimodal endpoint requests are built with `input.messages[].content[]`.
- [ ] Prompt maps to a `text` content entry.
- [ ] URL/base64 image inputs map to `image` content entries.
- [ ] `Pixels` size maps to `parameters.size` as `W*H`.
- [ ] Count, negative prompt, prompt extend, watermark, and seed map into `parameters`.
- [ ] Z-Image `prompt_extend` response metadata is preserved in provider metadata when returned.

### Wan mapping

- [ ] Wan sync requests use the documented multimodal endpoint.
- [ ] `ResolutionTier("1K" | "2K" | "4K")` and pixel sizes map to `parameters.size`.
- [ ] Region bbox edits map to `parameters.bbox_list` when pixel bboxes are supplied.
- [ ] Color palette, thinking mode, and sequential generation are supported through typed config where present or `provider_options`.
- [ ] Wan image input limits are validated per operation capability.

### Async and response handling

- [ ] Provider async task creation and polling are supported only for documented models/endpoints.
- [ ] Async task status maps to `ProviderGenerationStatus`.
- [ ] Output image URLs become `ProviderAsset` ingestion sources.
- [ ] Known 24-hour generated URL expiry is represented on provider assets.
- [ ] Provider request id, usage, actual prompt, and reasoning/prompt extension metadata are preserved where returned.

### Tests

- [ ] Request mapping tests cover Qwen text-to-image, Qwen edit/fusion, Z-Image, Wan text-to-image, Wan sequential generation, and Wan bbox edit.
- [ ] Async polling tests cover task success, failure, timeout, and unsupported async model behavior.
- [ ] Response parsing tests cover current multimodal responses and legacy async task responses.
- [ ] Compatibility tests cover unsupported format, unsupported regions, input image limits, and strict/coerce behavior.
- [ ] `cargo test -p agent-runtime-aigc-providers aliyun` passes.

## Blocked By

- Issue 001
- Issue 002

