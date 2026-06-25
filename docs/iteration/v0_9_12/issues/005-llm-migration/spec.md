# Issue 005: LLM migration + dependency inversion

## Background

`agent-runtime-providers` (LLM) is the **only** provider crate with internal consumers (`core/node/py`).
Move its dialect impls behind the wall (into `orchest-provider-http`), keep `agent-runtime-providers` as a
deprecated re-export, and invert `core → providers` to `core → orchest-protocol`. `node/py` are insulated
at **two** points: the `core::model::ModelAdapter` trait alias **and** the
`agent_runtime_providers::create_adapter_from_config`/`normalize_provider_model` functions.

## Goal / Scope

In scope:

- Move anthropic / openai / deepseek / openrouter / volcengine-ark / minimax-llm into
  `orchest-provider-http` dialect modules implementing `ChatModel`.
- Register them through the wall (Issue 004).
- Keep `agent-runtime-providers` as a deprecated re-export preserving the `ModelAdapter` path,
  `create_adapter_from_config`, and `normalize_provider_model` (same signatures).
- Flip `agent-runtime-core` to depend on `orchest-protocol`; keep `core::model::ModelAdapter` stable.

Out of scope: asr/tts/realtime (006), aigc (007), removing the shim (008).

## Acceptance Criteria

- [ ] LLM dialects live in `orchest-provider-http` as `ChatModel` impls, registered via the wall.
- [ ] `agent-runtime-providers` is a thin deprecated re-export; `create_adapter_from_config` /
      `normalize_provider_model` still resolve with identical signatures.
- [ ] `agent-runtime-core` depends on `orchest-protocol` (not concrete providers); `core::model::ModelAdapter`
      alias unchanged.
- [ ] `node/py` compile and their tests pass **unchanged**.
- [ ] Existing LLM provider tests pass (characterization).

## Notes

The riskiest binding-facing phase; the two insulation points are the safety contract. Depends on 002/003/004.
