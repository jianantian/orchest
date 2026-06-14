# 004 · Soniox adapter

## Background

Soniox provides useful pressure for multilingual and code-switching behavior.

## Goal

Add a Soniox provider adapter behind a feature flag.

## Acceptance Criteria

- [ ] Provider is gated by cargo feature `soniox`.
- [ ] Capability metadata represents multilingual and code-switching support.
- [ ] `Language` and routing behavior handle mixed-language requests without hard-coded closed sets.
- [ ] Streaming partial/final events normalize to the common ASR stream model.
- [ ] `transcribe()` behavior is explicitly implemented or returns `UnsupportedOperation` with tests.
- [ ] Fake and env-gated live tests cover code-switching options and document required variable names.
