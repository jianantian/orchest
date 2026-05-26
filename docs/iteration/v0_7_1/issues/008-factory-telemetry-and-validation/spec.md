# 008 · Factory, Telemetry, and End-to-End Validation

## Background

After the storage foundation, provider adapters, and gateway orchestration exist, the crate needs a stable construction path, observability helpers, examples, and full workspace validation. This issue makes the feature usable as an independent SDK crate.

## Goal

Add provider factory/config APIs, shared HTTP plumbing, telemetry helpers, examples, and final validation across all implemented image providers and gateway paths.

## Acceptance Criteria

### Factory and config

- [ ] `AigcProviderRuntimeConfig` from issue 001 is the single provider construction input for application code.
- [ ] `create_image_provider_from_config()` routes to Crazyrouter, Aliyun, OpenRouter, and Renderful adapters.
- [ ] API key resolution priority is explicit `api_key` > explicit local `api_key_env` > provider default environment variable.
- [ ] If `api_key_env` is configured and missing/empty, construction fails and does not fall back to provider defaults.
- [ ] Unknown provider returns stable `AigcError { code: "unknown_provider" }`.
- [ ] Invalid or empty model returns stable `AigcError { code: "invalid_model" }`.
- [ ] Provider-specific configs are constructed only inside the factory.

### Shared HTTP and telemetry

- [ ] All provider adapters use the shared client.
- [ ] Telemetry helpers emit spans for image create, provider request, provider poll, asset persist, and signed URL generation.
- [ ] Metrics cover provider duration, asset persistence duration, generated image count, persisted bytes, and error counts.
- [ ] Base64 payloads, signed URLs, API keys, and storage credentials are not logged by default.

### Examples and docs

- [ ] A standalone Rust example shows text-to-image with URL delivery using local storage.
- [ ] A standalone Rust example shows Base64 delivery still returning `asset_id`.
- [ ] A standalone Rust example shows resolving an old `asset_id` to a fresh URL.
- [ ] Examples compile without real provider credentials by using mock/local components or documented environment guards.

### End-to-end validation

- [ ] Provider adapter tests for Crazyrouter, Aliyun, OpenRouter, and Renderful all pass.
- [ ] Gateway end-to-end tests with mock providers and local storage pass.
- [ ] Public serialization contract tests pass.
- [ ] `cargo test -p agent-runtime-aigc-providers` passes.
- [ ] `cargo clippy -p agent-runtime-aigc-providers -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `cargo fmt --check` passes.

## Blocked By

- Issue 001
- Issue 002
- Issue 003
- Issue 004
- Issue 005
- Issue 006
- Issue 007
