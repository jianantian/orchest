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

### Changed

- **Breaking:** session persistence is an append-only event log instead of a
  snapshot ([#331](https://github.com/jianantian/orchest/issues/331)). A run
  records every change to its state as a `SessionEvent` (consecutive `seq`,
  timestamp, tagged `SessionEventKind`: `RunStarted`, `StepStarted`,
  `StepEnded`, `SystemMessage`, `UserMessage`, `AssistantMessage`,
  `ToolDispatched`, `ToolResult`, `Compacted`, `Handoff`, `RunEnded`, `RunFailed`,
  `RunAborted`); the conversation, step and budget are a `SessionProjection`
  of that log, and the projected history now ends with the final answer.
  `SessionStore` is now `put_meta` / `get_meta` / `append` / `read(from_seq)`
  / `flush` / `delete` / `list`, with session metadata (`SessionMeta`,
  schema `"2"`) stored apart from the events. `InMemorySessionStore` and
  `SqliteSessionStore` implement it; the SQLite tables are `session_meta` and
  `session_events` keyed by `(session_id, seq)`. A fresh run on a session
  that already holds events fails with `SessionError::AlreadyExists`.
- **Breaking:** `AgentRun::resume(config, model, registry)` and
  `AgentRun::resume_with_input(config, input, model, registry)` are `async`,
  read the log named by `config.session_store` / `config.session_id`, and
  return `SessionError` (`NotConfigured`, `NotFound`, store errors). The host
  supplies the config again; nothing but the log is restored.
- **Breaking:** approvals go through a blocking request channel
  ([#334](https://github.com/jianantian/orchest/issues/334)). Every request
  has a `RequestId` (`"{run_id}/{n}"`), a host-defined `RequestKind`
  (Orchest raises `tool_approval` and `retry_approval`), a JSON payload and a
  `RequestTarget`. `RunHandle::respond_approval(run_id, bool)` is replaced by
  `RunHandle::respond(&RequestId, RequestResponse)` (approve, deny, or answer
  with a payload, plus optional session rules), `RunHandle::withdraw`,
  `RunHandle::pending_requests` and `RunHandle::request`; one `RequestBus`
  (formerly `ApprovalBus`, also `ToolContext::request_bus`) serves the whole
  run tree, so child-run requests are answered by id from the root handle.
  `ApprovalRequested` carries `request_id`, `kind` and `payload`;
  `ApprovalGranted` / `ApprovalDenied` carry `request_id`. The Python and
  Node `respond_approval` / `respondApproval` take the request id instead of
  the run id.
- **Breaking:** a request that is not answered within
  `RuntimeConfig::request_timeout` (default one hour) fails closed: the tool
  call gets an error result with code `APPROVAL_TIMEOUT` and the run goes on,
  instead of the run failing. A withdrawn request gives `APPROVAL_WITHDRAWN`,
  and requests still pending when the run stops resolve as cancelled.
- **Breaking:** `ToolMetadata::side_effect: bool` is replaced by
  `ToolMetadata::effects: ToolEffects`, a set of `ToolEffect`s (`Read`,
  `WriteWorkspace`, `WriteOutside`, `Network`, `Credential`, `Destructive`,
  `Process`) or unknown ([#335](https://github.com/jianantian/orchest/issues/335)).
  Unknown is the default and counts as having side effects, so an undeclared
  tool asks under `Approval::WhenRisky`. Serialized as a list, or `null` when
  unknown. Built-in tools declare their effects (skill script tools
  `Process`, `write_file` `WriteWorkspace` + `WriteOutside`, code execution
  unknown, `read_file` and `load_skill` `Read`).
- **Breaking:** `RuntimeConfig::should_approve` is replaced by the single
  permission decision point `RuntimeConfig::decide(call, metadata, grants)`,
  which returns `PermissionDecision::{Allow, Ask, Deny}` with its
  `DecisionBasis`; every tool call goes through it.
- **Breaking:** MCP tools use `Approval::WhenRisky` with effects from the
  server's `annotations` (`readOnlyHint` → `Read`, plus `Network` with
  `openWorldHint`; otherwise unknown) instead of bypassing approval with
  `Approval::Never`. `McpToolDef` carries `annotations`.
- **Breaking:** cancellation takes effect inside a step
  ([#336](https://github.com/jianantian/orchest/issues/336)). A run shares a
  `CancelToken` with its handle and its delegated children (as child tokens);
  `RunHandle::cancel(Cancellation)` / `ChildRunHandle::cancel` set it at once
  and are idempotent (the first reason wins, `RunHandle::cancellation()`
  reports it). The in-flight model call, compaction call, retry waits and
  async-job polling are dropped, a pending approval resolves as `Cancelled`
  immediately, interruptible tool calls are dropped, and no tool call is
  dispatched afterwards; a non-interruptible call finishes and its result is
  recorded. Model calls are interrupted by dropping the `ChatModel::complete`
  future, so the provider trait is unchanged. `RuntimeEvent::RunAborted` and
  `SessionEventKind::RunAborted` carry `cancel_reason: Option<CancelReason>`
  (`User`, `Timeout`, `Upstream`, `HostDisconnected`, `Watcher`; unset when
  the supervisor gives up on a crashed worker). `ToolContext` carries
  `cancel`. A cancelled run is never restarted by the supervisor.

- **Breaking:** the primary event receiver is lossless
  ([#338](https://github.com/jianantian/orchest/issues/338)). Delivery waits
  for channel capacity instead of dropping an event after
  `EVENT_DELIVERY_TIMEOUT` (removed), so a receiver that is not drained holds
  the run back. The primary no longer gets `EventsDropped { subscriber_id: 0 }`;
  secondary subscribers keep their lossy `try_send` semantics and
  `EventsDropped` signals.

### Added

- Session checkpoints and a resume boundary
  ([#332](https://github.com/jianantian/orchest/issues/332)). A run appends
  and `flush`es its session log at the start of each step (before the model
  request), at the end of each step, at run termination (completed, failed or
  aborted), and before dispatching a tool with side effects, after recording a
  `ToolDispatched` marker. `CheckpointPolicy::EveryTool`
  (`AgentConfigBuilder::checkpoint_policy`, `AgentConfig::checkpoint_policy`)
  also flushes before every other dispatch and after every tool result; the
  policy is stored in `SessionMeta::checkpoint_policy`. On resume the store is
  flushed before the first write, and a step left open is closed: each tool
  call it requested without a result gets an error result
  (`TOOL_OUTCOME_UNKNOWN` if dispatched, `TOOL_NOT_STARTED` if not), the step
  is recorded as `SessionEventKind::StepInterrupted`, and it runs again under
  the same step number with the budget of the last completed step. No tool
  call is run again.
- In-flight tool ledger
  ([#333](https://github.com/jianantian/orchest/issues/333)). Before a tool
  runs, the log records `SessionEventKind::ToolDispatched { tool_use_id,
  tool_name, args_summary }`; `args_summary` (`session::summarize_args`)
  keeps the input's keys and value types but no values, so credentials stay
  out of the log. A `ToolResult` settles the call.
  `SessionLog::dispatched_but_unsettled()` / `SessionProjection::unsettled`
  list calls dispatched without a result. Calls a crash or cancellation left
  without a result are closed with an error tool result carrying one of the
  stable codes `tool::TOOL_OUTCOME_UNKNOWN` (dispatched, effect unknown),
  `tool::TOOL_NOT_STARTED` (crash before dispatch) or
  `tool::TOOL_ABORTED_BEFORE_DISPATCH` (cancelled before dispatch), and a
  `next_step` telling the model to check external state before repeating a
  call that may have taken effect. Settled calls produce no closure.
- `RuntimeEvent::RequestResolved { request_id, kind, resolution }` reports how
  each request ended (`Responded`, `Withdrawn`, `TimedOut`, `Cancelled`), and
  the session log records `RequestOpened` / `RequestResolved`, so
  `SessionProjection::pending` lists the requests open at a crash
  ([#334](https://github.com/jianantian/orchest/issues/334)).
- Host permission rules ([#335](https://github.com/jianantian/orchest/issues/335)):
  `PermissionPolicy` (`AgentConfigBuilder::permissions`,
  `RuntimeConfig::permissions`) holds `PermissionRule`s that allow, ask or deny
  by tool-name, command and path globs and by effects; the last matching rule
  wins, `PermissionPolicy::unmatched` sets the action for unmatched calls, and
  relative paths resolve against `PermissionPolicy::workspace`. Allow rules
  require every command of a chain and every path to match. Denied calls get a
  `PERMISSION_DENIED` tool result. `Tool::permission_subject` lets a tool name
  its commands and paths. `RuntimeEvent::PermissionDecided` reports each
  decision.
- Approval scope: `RequestResponse::for_session()` (`ApprovalScope::Session`)
  remembers an approval for the same tool and input (`tool::grant_key`) for
  the rest of the session, recorded as `SessionEventKind::PermissionGranted`
  and projected into `SessionProjection::session_grants`. One-time approvals
  (the default) are not remembered; a session grant never overrides a deny.
- Enforcement facts: `ExecutionBackend` (`AgentConfigBuilder::execution_backend`)
  lets a host declare the isolation it runs tools in, `ScriptExecutor::enforcement`
  reports code-execution isolation, and `RuntimeConfig::enforcement()` returns
  `EnforcementFacts` (`Hard` / `Soft` / `None` for isolation, the permission
  gate, code execution and read-only). Read-only is `Hard` only with a hard
  backend.
- Tool interruptibility ([#336](https://github.com/jianantian/orchest/issues/336)):
  `Tool::interruptible()` (default: true for tools without side effects),
  overridable per tool with `AgentConfigBuilder::interruptible` /
  `RuntimeConfig::interruptible`.
- Process termination backend: `ProcessTerminator`
  (`AgentConfigBuilder::process_terminator`) is called once on cancellation,
  best effort with a 5 s limit, and its outcome is reported as
  `RuntimeEvent::ProcessesTerminated { terminated, error }`. An async job
  abandoned by a cancellation gets the result code `ASYNC_JOB_ABANDONED`.
- Message lifecycle events ([#337](https://github.com/jianantian/orchest/issues/337)):
  each model call is an assistant message with an id, reported as
  `RuntimeEvent::MessageStarted { message_id, step }`,
  `MessageUpdated { message_id, delta }` and
  `MessageEnded { message_id, content, stop_reason }`. `MessageDelta` keeps
  `Text`, `Thinking` and `ToolUseArgs` apart. `ModelStreamChunk` stays.
- Request and ledger events: `RuntimeEvent::RequestOpened { request }` for
  every blocking request (approvals included), and `ToolDispatched` /
  `ToolSettled { is_error, error_code }` mirroring the session log's tool
  ledger.
- Host-defined requests: `ToolContext::request(tool_name, kind, payload,
  timeout)` lets a tool block on a question of its own kind, answered with the
  same `RunHandle::respond` (`RequestResponse::answer(payload)`).
- Capability probe: `RunHandle::capabilities()` / `AgentConfig::capabilities()`
  return `Capabilities` derived from the config (resume, preemptive cancel,
  approval scopes, request kinds, parallel tools, delegation, effects,
  enforcement, lossless delivery); `Capabilities::require` returns a
  `NotSupported` error for a missing `Capability`.
- Event delivery: `RuntimeConfig::event_channel_capacity` /
  `AgentConfigBuilder::event_channel_capacity` (default
  `DEFAULT_EVENT_CHANNEL_CAPACITY`, 1024), `RunHandle::event_delivery_stats()`
  (`EventDeliveryStats`: waits, longest and total wait, high watermark, sink
  errors) and the metrics `orchest_event_backpressure_waits_total`,
  `orchest_event_backpressure_wait_seconds` and
  `orchest_event_channel_occupancy`. `DurableEventSink`
  (`AgentConfigBuilder::event_sink`) receives every primary event with its
  sequence number before the host does and is flushed after ledger and
  terminal events. `Capabilities::lossless_events` is now `true`.
- `docs/guide/events.md`: every event, and the split between the session log
  (execution state) and the event stream (observation).

### Removed

- **Breaking:** `SessionSnapshot`, `SessionPersistenceHook`,
  `Hook::persistence_session_id` and `ConfigError::SessionStoreMissing`
  ([#331](https://github.com/jianantian/orchest/issues/331)).

## [1.1.0]

### Added

- DeepSeek: `deepseek/deepseek-flash` accepts image input. `ContentBlock::Image`
  in user messages is sent as OpenAI Chat `image_url` content parts (`Url`
  as-is, `Base64` as a `data:` URL, `detail` forwarded), and the catalog row
  declares `Modality::Image`, so `Registry::chat().accepts([Modality::Image])`
  can select it. The unlisted legacy name `deepseek-v4-flash-vision-exp`
  behaves the same. For a DeepSeek model without image input
  (`deepseek-v4-pro`, and the deprecated `deepseek-v4-flash` row), a
  `CompatibilityPolicy::Strict` request carrying an image fails before it is
  sent with the `ModelError` code `unsupported_image_input`; under `Coerce`
  the image is dropped and recorded as a `content_block` `OptionAdjustment`.
  Other Chat providers keep dropping images as before
  ([#319](https://github.com/jianantian/orchest/issues/319)).

### Changed

- DeepSeek: the static catalog adds `deepseek/deepseek-flash`
  (DeepSeek-V4.1-Flash) and updates `deepseek/deepseek-v4-pro` to the current
  1M context / 384K output limits and peak-hour CNY pricing. Thinking efforts
  now lower to the official `low` / `high` / `max` (`Minimal`/`Low` → `low`,
  `Medium`/`High` → `high`, `XHigh`/`Max` → `max`)
  ([#318](https://github.com/jianantian/orchest/issues/318)).
- DeepSeek: `deepseek-chat` / `deepseek-reasoner`, which DeepSeek discontinued
  on 2026-07-24, are no longer recognized by name as thinking-capable
  1M-context models and get the unknown-model defaults. They were never
  catalog rows ([#318](https://github.com/jianantian/orchest/issues/318)).

### Deprecated

- DeepSeek: the `deepseek/deepseek-v4-flash` catalog row. DeepSeek routes the
  name to V4.1 Flash, so it stays resolvable for 1.x (identity picks,
  `find_model`) with its 1.0.0 capabilities (text input only) and current
  pricing, but it is now `ModelStatus::Deprecated` and hidden from default
  `list_models` discovery. Use `deepseek/deepseek-flash`, which also accepts
  images ([#318](https://github.com/jianantian/orchest/issues/318)).

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

[Unreleased]: https://github.com/jianantian/orchest/compare/v1.1.0...HEAD
[1.1.0]: https://github.com/jianantian/orchest/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/jianantian/orchest/releases/tag/v1.0.0
[1.0.0-rc.1]: https://github.com/jianantian/orchest/releases/tag/v1.0.0-rc.1
