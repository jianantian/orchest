# Issue 002 Plan: Duplex session scaffolding

## Files to Read

- `docs/iteration/v0_9_11/prd.md`
- `docs/iteration/v0_9_11/issues/001-protocol-fixtures/spec.md` and its provider decision output
- Selected vendor documentation from `docs/external/`
- Existing ASR/TTS duplex provider modules and tests
- Relevant crate `Cargo.toml` files for feature/dependency patterns

## Files to Change

- Selected provider crate source files for experimental realtime scaffolding, as determined by Issue 001
- Selected provider crate tests or fixture modules
- Relevant `Cargo.toml` feature/dependency entries if needed
- Manual-run docs if credentials/config names are finalized here

## Steps

1. Use Issue 001 outputs to choose the implementation location based on the selected provider and dependency weight.
2. Add feature-gated config and credential loading.
3. Add a minimal session lifecycle type or module: start, send input, close.
4. Add a fake session implementation for tests.
5. Add unit tests for lifecycle success and basic failure paths.
6. Confirm live-provider paths are manual/ignored unless credentials are present.
7. Run formatting, tests for the touched crate, and workspace checks; document any credential-gated checks that are skipped.
