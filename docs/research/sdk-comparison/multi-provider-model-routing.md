# Multi-Provider Model Routing Research Note

## Context

Current provider runtime behavior must stay conservative: a user-provided
`provider/model` resolves only against that provider's explicit `api_key`,
configured `api_key_env`, or matching default environment variable. If no API key
is available for that provider, runtime returns `missing_api_key`. It must not
guess another provider or model.

This note records a future design direction where a logical model can be backed
by multiple providers, driven by user configuration instead of runtime guessing.

## Future Direction

A future config layer may allow users to define logical model aliases with one or
more provider-backed targets:

```toml
[models.sonnet]
default_provider = "anthropic"

[[models.sonnet.providers]]
provider = "anthropic"
model = "claude-sonnet-4"
api_key_env = "ANTHROPIC_API_KEY"

[[models.sonnet.providers]]
provider = "openrouter"
model = "anthropic/claude-sonnet-4"
api_key_env = "OPENROUTER_API_KEY"
```

The key property is that routing is explicit and config-driven. Runtime should
not infer that two provider/model pairs are interchangeable just because their
names look similar.

## Layering Boundary

Provider adapters should stay as simple as possible. Their job is to provide a
uniform API calling interface and manage provider-specific protocol details:
request shape, streaming parse, error preservation, usage mapping, prompt cache
metadata, and provider-native replay requirements.

Retry, cross-provider route selection, failover, health policy, and cost/latency
tradeoffs should live in agent core or a higher-level policy layer. Provider
code should expose enough structured errors and telemetry for those layers to
make decisions, but it should not make routing decisions itself.

## Design Principles

- Availability and prompt cache behavior are the first-order goals.
- Provider adapters stay single-purpose: API call normalization and
  provider-specific management, not routing policy.
- The selected provider must be stable for the duration of a run/session unless
  the caller explicitly starts a new routing boundary.
- Do not switch providers mid-run based on retry, latency, rate limit, or cost
  alone. Mid-run switching risks losing prompt cache continuity and can change
  provider-specific reasoning, tool-call, cache, and message replay semantics.
- Retry within the same provider is acceptable when the provider contract permits
  it and the request can be safely retried.
- Cross-provider failover should be a deliberate policy at a clear boundary,
  such as before the first model call of a run, after a terminal provider
  failure before any stateful continuation, or after explicit caller approval.
- Any cross-provider policy must expose cache implications in telemetry so users
  can see when a route forfeits prompt cache continuity.
- Provider-specific prompt cache semantics must be part of route selection.
  Compatibility of model names is not enough.

## Non-Goals For Now

- No provider/model guessing in the current provider runtime.
- No automatic fallback from one provider's API key to another provider's API
  key.
- No implicit failover from `anthropic/...` to `openrouter/anthropic/...` or the
  reverse.
- No mid-run provider switching for rate, latency, or availability until prompt
  cache and replay semantics are modeled explicitly.

## Open Questions

- What is the stable identity of a prompt cache scope: provider, provider account,
  model, endpoint, cache namespace, or a combination?
- Should route selection happen once per agent run, once per conversation, or be
  caller-controlled?
- How should telemetry report "same logical model, different provider" without
  hiding provider-specific behavior?
- What config shape best represents provider priority, health, and cache
  stickiness without making the runtime policy opaque?
