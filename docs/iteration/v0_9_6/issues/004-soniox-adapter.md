# 004 · Soniox adapter

## Background

Soniox provides useful pressure for multilingual and code-switching behavior.

## Goal

Add a Soniox provider adapter behind a feature flag.

## Acceptance Criteria

- [ ] Capability metadata represents multilingual and code-switching support.
- [ ] `Language` and routing behavior handle mixed-language requests without hard-coded closed sets.
- [ ] Streaming partial/final events normalize to the common ASR stream model.
- [ ] Fake and env-gated live tests cover code-switching options.
