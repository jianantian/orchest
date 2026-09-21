# Structured decisions

`Decision` is a standalone capability for bounded judgments. Give it shared
JSON state and named, independent questions; receive typed answers under the
same names. Your code combines the answers, selects thresholds, and performs
actions. No agent loop is started.

| Question | Result | Typical use |
|----------|--------|-------------|
| `boolean` | `probability`: P(true), from 0 to 1 | Relevance, urgency, policy match |
| `choice` | Named `choice`, optional distribution and confidence | Classification and routing |
| `score` | Fractional `score` over zero-based ordered levels | Ranking and rubric evaluation |

Instructions and descriptions can be strings, objects, or arrays. Choice
descriptions may also be null. The shared protocol accepts any JSON state;
individual implementations can reject unsupported input forms. Questions in
one batch are independent: a question cannot implicitly consume another answer.

Confidence is distinct from the probability of a particular outcome. Missing
confidence or probabilities means unavailable; decide explicitly whether your
application should request review in that case. A three-level score ranges
from 0 to 2 and can be 1.05; it is not a percentage or an integer class label.

## Rust

Use `orchest-protocol` for the contract and `orchest-provider` with the
`decision` feature for the built-in HTTP provider:

```rust,no_run
use orchest_protocol::DecisionRequest;
use orchest_provider::{create_decision, DecisionConfig};
use serde_json::json;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let engine = create_decision(&DecisionConfig::new("openrouter/~typesafe/jev-latest"))?;
let request: DecisionRequest = serde_json::from_value(json!({
    "state": {"message": "My payouts have failed for three days."},
    "questions": {
        "urgent": {"type": "boolean", "instructions": "Is this time-sensitive?"}
    }
}))?;
let response = engine.decide(request).await?;
// Retain engine across calls to avoid rebuilding provider configuration.
# Ok(())
# }
```

For occasional calls, `orchest_provider::decide(&config, request).await` combines
construction and evaluation. Both paths preserve metadata and usage instead of
returning only the chosen label. There is no implicit model selection in this
convenience API.

## Custom implementations

Implement `orchest_protocol::Decision` for a local model, deterministic engine,
or another remote service, and register an
`Entry<Box<dyn Decision>>` with `Registry::register_decision`. A registry with
no features still supports this mechanism; HTTP and credentials are optional.
Call `request.validate()` and `response.validate_for(&request)` at your boundary.
Usage can be absent for engines that do not account in tokens.

`registry.decision().provider("your-provider").select()` supports capability
discovery and provider defaults. `registry.create_decision(&DecisionConfig::new(
"your-provider/your-model"))` makes an explicit identity selection. Applications
can inject a `Box<dyn Decision>` without importing its concrete implementation.
See the [custom implementation test](../../crates/orchest-provider/tests/decision.rs).

## First adapter: OpenRouter

The initial built-in deployment uses
`POST https://openrouter.ai/api/alpha/decisions` and registers
`openrouter/~typesafe/jev-latest` (provider default) and
`openrouter/typesafe/jev-1.13`. It accepts string/object/array state. The adapter
maps portable `boolean` / `probability` to OpenRouter's `noul` wire format, and
maps the optional USD `usage.cost` to `usage.cost_usd`.

Explicit `api_key` wins; otherwise the adapter reads `OPENROUTER_API_KEY`.
An explicitly configured `api_key_env` must exist and does not fall back to
another key. `api_url` overrides the complete endpoint, including its path.
`timeout_ms` sets a positive per-request timeout. No automatic retries occur.
Unsupported model IDs return `NoMatchingProvider`; register another model or
provider through the registry when needed.

Input validation errors use `InvalidRequest`; malformed or inconsistent answers
use `InvalidResponse`. HTTP errors retain status, numeric Retry-After seconds,
and upstream details. Python exposes `ModelError`; JavaScript exposes
`ProviderError`. Both retain the structured error classification.

OpenRouter marks this endpoint alpha. Offline HTTP contract tests verify the
adapter; they do not establish current service availability or model accuracy.

## Examples and language guides

- [Rust ticket triage](../../examples/rust/providers/decisions.rs)
- [Python ticket triage](../../examples/python/providers/decisions.py) and [SDK guide](./sdk-python.md)
- [TypeScript ticket triage](../../examples/typescript/providers/decisions.ts) and [SDK guide](./sdk-typescript.md)

Protocol references: [OpenRouter OpenAPI](https://openrouter.ai/openapi.json),
[TypeSafe primitives](https://docs.typesafe.ai/primitives).
