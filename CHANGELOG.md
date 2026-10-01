# Changelog

All notable changes to the published Orchest crates are documented in this
file. The crates are released in lockstep under one version (see
[ADR-0003](docs/adr/0003-release-policy.md)).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The SemVer promise covers the Supported crates (`orchest`,
`orchest-protocol`, `orchest-provider`, `orchest-storage`). The Internal
crates (`orchest-provider-core`, `orchest-provider-http`,
`orchest-provider-stream`, `orchest-provider-visual`) are not covered,
except for the items `orchest-provider` re-exports.

## [Unreleased]

### Added

- DeepSeek: `deepseek/deepseek-flash` accepts image input. `ContentBlock::Image`
  in user messages is sent as OpenAI Chat `image_url` content parts (`Url`
  as-is, `Base64` as a `data:` URL, `detail` forwarded), and the catalog row
  declares `Modality::Image`, so `Registry::chat().accepts([Modality::Image])`
  can select it. The legacy `deepseek-v4-flash*` names behave the same. For a
  DeepSeek model without image input (`deepseek-v4-pro`), a
  `CompatibilityPolicy::Strict` request carrying an image fails before it is
  sent with the `ModelError` code `unsupported_image_input`; under `Coerce`
  the image is dropped and recorded as a `content_block` `OptionAdjustment`.
  Other Chat providers keep dropping images as before
  ([#319](https://github.com/jianantian/orchest/issues/319)).

### Changed

- DeepSeek: the static catalog now lists the two online models,
  `deepseek/deepseek-flash` (DeepSeek-V4.1-Flash) and
  `deepseek/deepseek-v4-pro`, with the current 1M context / 384K output
  limits and peak-hour CNY pricing. Thinking efforts now lower to the official
  `low` / `high` / `max` (`Minimal`/`Low` → `low`, `Medium`/`High` → `high`,
  `XHigh`/`Max` → `max`). The legacy `deepseek-v4-flash*` names, which
  DeepSeek still routes to V4.1 Flash, keep Flash capabilities by name but are
  no longer catalog rows ([#318](https://github.com/jianantian/orchest/issues/318)).

### Removed

- DeepSeek: **Breaking:** `deepseek/deepseek-v4-flash` is no longer a catalog
  row, and the discontinued `deepseek-chat` / `deepseek-reasoner` names are no
  longer recognized as thinking-capable 1M-context models; they get the
  unknown-model defaults ([#318](https://github.com/jianantian/orchest/issues/318)).

## [1.0.0]

First public release on crates.io. The pre-1.0 development history (v0.1
through v0.17 plus hotfixes) is recorded in the
[iteration roadmap](docs/iteration/roadmap.md) and in `docs/archive/`, not
in this file.

### Added

#### `orchest`: agent runtime

- Agent loop with a streaming `RuntimeEvent` stream covering model output,
  tool calls, and run completion or failure. Failures carry a structured
  `RunFailureKind`, and completion carries a `stop_reason`.
- `Tool` trait and `ToolRegistry`, with optional parallel tool execution,
  draft/commit approval for risky side effects, and deferred tool
  discovery.
- MCP client over stdio and HTTP. MCP tools go through the same `Tool` trait
  as in-process tools.
- Skills: `SKILL.md` packages compatible with the Agent Skills standard,
  with zero-configuration progressive disclosure through a built-in
  `load_skill` tool.
- Composition: Agent-as-Tool sub-agents with an output-format contract, and
  handoffs with input filters. `ContextMode::Fresh` / `Fork` controls how a
  child starts.
- Supervised delegation: multi-subscriber event streams, attached watchers
  (including `LlmWatcher`) with deterministic multi-watcher arbitration,
  steering and message injection, delegated child run control, and restart
  after run-level failure.
- Safety: `ApprovalMode`, a four-layer guardrail framework, and hooks that
  can log, modify or abort.
- Resilience: LLM retry with `RetryPolicy::recommended()`, loop detection,
  a repeated-failure threshold, budget guard, context compaction and a
  structured `ToolError`.
- Session persistence and resume through `SessionStore`. The
  `sqlite-session` feature adds a SQLite store.
- Code execution through an injectable `ScriptExecutor`.
- Atomic `complete()` for single-turn calls without starting an agent run.
- Observability through `tracing` spans and `metrics`.

#### `orchest-protocol`: shared protocol

- Content model (`ContentBlock`) with multimodal input.
- Capability traits: `ChatModel`, `Asr`, `Tts`, `VoiceManager`,
  `RealtimeSession`, `GenTask`, `Decision`.
- Unified `StreamEvent`, `CapabilityDescriptor` and `ProtocolError`.

#### `orchest-provider`: provider registry

- `Registry` with selection by capability query or identity pick, catalog
  discovery (`list_models`, `find_model`) and custom provider registration
  through `Registry::register_*`.
- Features select the compiled weight tier: `http`, `stream`, `visual`, and
  the capability aliases `llm`, `decision`, `asr`, `tts`, `realtime`, `gen`.
  `llm` pulls in no WebSocket or signing dependencies.
- LLM chat: Anthropic, OpenAI, DeepSeek, OpenRouter, Volcengine Ark and
  Minimax, addressed with the `provider/[protocol/]model` syntax.
- Speech: one-shot and streaming ASR (Volcengine, Aliyun, Deepgram,
  ElevenLabs Scribe, Soniox, AssemblyAI, Speechmatics), TTS (Volcengine,
  Aliyun, Minimax) and Volcengine realtime omni sessions.
- Image, video and music generation through `GenTask` providers.
- Typed `Decision` requests, with OpenRouter as the first adapter.
- `testing` feature with deterministic `FakeAsr` / `FakeTts`.

#### `orchest-storage`: object storage

- `ObjectStore` trait with put, get, idempotent delete, presigned GET URLs
  and public URLs. It is backed by Aliyun OSS (V1 signature) and Tencent
  COS (V5 signature), and `create_object_store` picks the dialect.

## [1.0.0-rc.1]

Release candidate for 1.0.0, with the same feature set as the 1.0.0 entry
in [CHANGELOG.md](CHANGELOG.md#100). Published so crates.io users can try
the lockstep crates before the final release.

[Unreleased]: https://github.com/jianantian/orchest/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/jianantian/orchest/releases/tag/v1.0.0
[1.0.0-rc.1]: https://github.com/jianantian/orchest/releases/tag/v1.0.0-rc.1
