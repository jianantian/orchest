# 002 · Asset Store and Registry Foundation

## Background

Generated provider URLs may expire and provider output shapes differ. The gateway public contract requires generated assets to be returned either as base64 or as an immediately usable Orchest-controlled URL. Long-lived clients store `asset_id`, not provider URLs or storage descriptors.

This issue builds the storage foundation that all provider and gateway slices depend on.

## Goal

Implement `AssetStore`, `AssetRegistry`, local/noop stores, and the OSS-backed production store contract so generated assets can be persisted, resolved by scoped `asset_id`, and exposed through directly fetchable URLs without leaking storage details.

## Acceptance Criteria

### Storage traits and types

- [ ] `AssetStore` exists with `put_stream()` and `signed_url()` methods.
- [ ] `signed_url()` accepts stored asset metadata and returns `AssetAccessUrl { url, expires_at }`, not a bare string.
- [ ] `AssetRegistry` exists with `save(scope, asset)` and `get(scope, asset_id)`.
- [ ] `AssetScope` exists and includes tenant/workspace/app/namespace-style scope fields.
- [ ] `StoredAsset`, `StorageLocation`, `OssObjectLocation`, and `LocalObjectLocation` exist.
- [ ] `AssetIngestSource` supports provider URL, data URL, base64, and bytes ingestion.
- [ ] `PutAssetOptions` carries namespace/key-prefix, content type hints, and any relevant persistence options.

### Public contract

- [ ] Normal asset use requires only `GeneratedImage.asset_id` and/or `ImageUrlOutput.url`.
- [ ] Public output types do not require bucket, endpoint, region, object key, access key, ACL, or storage-provider signing logic.
- [ ] `resolve_asset_url(scope, asset_id, ttl)` returns an immediately usable `ImageUrlOutput`.
- [ ] Resolving an asset under the wrong `AssetScope` returns a stable error and does not reveal whether the asset exists in another scope.

### Implementations

- [ ] `InMemoryAssetRegistry` exists for unit tests and mock gateway flows.
- [ ] A local/file-backed registry implementation exists for local integration tests.
- [ ] `NoopAssetStore` exists for tests that do not need persistence.
- [ ] `LocalAssetStore` persists assets under a local root and can return directly usable local/dev URLs or paths as configured.
- [ ] `OssAssetStore` exists as the first production implementation.
- [ ] `OssStorageConfig` includes endpoint, bucket, region, access key id, access key secret, optional public base URL, signed URL TTL, and key prefix.
- [ ] OSS configuration uses storage-scoped environment names and does not reuse DashScope API keys.

### Low-overhead behavior

- [ ] Provider URL ingestion streams downloads into storage uploads where possible.
- [ ] Ingestion computes SHA-256, byte count, and MIME type without image decoding.
- [ ] Base64 ingestion enforces configured max bytes.
- [ ] Tests cover that large payloads are not logged or included in tracing fields.

### Tests

- [ ] Unit tests cover local store put/resolve.
- [ ] Unit tests cover in-memory registry save/get.
- [ ] Unit tests cover local/file-backed registry persistence across registry instances.
- [ ] Unit tests cover scoped registry lookup success and wrong-scope rejection.
- [ ] Unit tests cover signed/non-expiring URL metadata behavior.
- [ ] Unit tests cover base64 payload max-size failure.
- [ ] Unit tests verify public response helpers do not expose storage descriptors.
- [ ] `cargo test -p agent-runtime-aigc-providers storage` passes.
- [ ] `cargo clippy -p agent-runtime-aigc-providers -- -D warnings` passes.

## Notes

- Real OSS network integration can be behind ignored tests or feature-gated integration tests if credentials are unavailable.
- The storage API must remain provider-neutral even though OSS is the first production implementation.
