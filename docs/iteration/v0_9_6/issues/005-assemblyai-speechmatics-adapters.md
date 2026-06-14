# 005 · AssemblyAI and Speechmatics adapters

## Background

AssemblyAI pressures rich transcript metadata and batch result shape; Speechmatics pressures multilingual enterprise fallback behavior.

## Goal

Add AssemblyAI and Speechmatics adapters behind feature flags.

## Acceptance Criteria

- [ ] AssemblyAI provider is gated by cargo feature `assemblyai`.
- [ ] Speechmatics provider is gated by cargo feature `speechmatics`.
- [ ] AssemblyAI adapter maps transcript metadata into provider-neutral result fields or `provider_options`.
- [ ] Speechmatics adapter maps multilingual capabilities into provider-neutral metadata.
- [ ] Batch/result shape differences are documented.
- [ ] Each adapter's `transcribe()` and `start_stream()` support matrix is explicit and tested.
- [ ] Fake tests cover routing, unsupported options and metadata preservation.
- [ ] Env-gated live tests are documented with required variable names and ignored by default.
