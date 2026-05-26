# 007 · Image Gateway Orchestration and Public Output Contract

## Background

Provider adapters return provider assets and provider events. The gateway owns validation, provider execution, async polling, asset persistence, public event emission, and final response construction. This issue delivers the first end-to-end public image generation path using mock providers and real local storage.

## Goal

Implement `ImageGateway` orchestration so callers can submit normalized image requests and receive only public `Url` or `Base64` outputs with stable `asset_id` values.

## Acceptance Criteria

### Gateway flow

- [ ] `ImageGateway` owns an `ImageProvider`, `AssetStore`, `AssetRegistry`, and gateway config.
- [ ] Gateway validates requests against per-operation capabilities.
- [ ] Gateway calls provider adapters and drains provider events.
- [ ] Gateway polls async provider jobs until completion/failure/timeout.
- [ ] Gateway maps provider statuses into public `GenerationStatus`, including `PersistingAssets`.
- [ ] Gateway persists provider assets before returning `Completed`.

### Public output contract

- [ ] Final responses contain only `ImageOutput::Url` or `ImageOutput::Base64`.
- [ ] Provider raw URLs are never serialized in public responses by default.
- [ ] URL outputs are directly fetchable without bucket, endpoint, object key, storage credentials, ACL, or signing logic.
- [ ] Base64 delivery still persists assets by default and returns `GeneratedImage.asset_id`.
- [ ] URL delivery returns `GeneratedImage.asset_id` and a usable URL.
- [ ] `asset_id` can be used later with scoped asset resolution to refresh an expired URL.

### Compatibility and events

- [ ] Strict compatibility rejects unsupported fields for the selected operation.
- [ ] Coerce compatibility records `OptionAdjustment` for safe adjustments.
- [ ] Gateway public events are emitted separately from provider adapter events.
- [ ] Public partial image events are disabled by default unless they can satisfy the public output contract.

### Tests

- [ ] Mock provider + local asset store end-to-end text-to-image test passes.
- [ ] Mock provider async task test covers queued/running/completed/persisting/completed public flow.
- [ ] Mock provider failure test preserves provider error.
- [ ] Tests prove provider raw URL is not present in serialized public response.
- [ ] Tests prove URL output is directly usable and does not require storage metadata.
- [ ] Tests prove Base64 output still includes `asset_id`.
- [ ] Tests prove wrong-scope asset resolution is rejected.
- [ ] `cargo test -p agent-runtime-aigc-providers gateway` passes.

## Blocked By

- Issue 001
- Issue 002

