# SDK Changelog

All notable changes to the Python package `orchest-py` and the npm package
`orchest-sdk` are documented in this file. The Rust crates have their own
[CHANGELOG.md](CHANGELOG.md).

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
The SDKs share one version, independent of the crate version
([ADR-0004](docs/adr/0004-sdk-packaging.md) D6). While the version is 0.x,
minor releases may break the Python or TypeScript API; such changes are
marked **Breaking:**. Each release names the crate version it is built
from.

## [Unreleased]

The bindings follow the v2.0 runtime contract
([#349](https://github.com/jianantian/orchest/issues/349)).

### Added

- First published release of `orchest-py` (PyPI) and `orchest-sdk` (npm),
  with prebuilt packages for Linux x86_64, Linux arm64 and macOS arm64
  ([ADR-0004](docs/adr/0004-sdk-packaging.md)).
- `Agent.abort(reason=None, message=None)` / `agent.abort(reason?, message?)`
  cancels the active run (reason `user` by default, or `timeout`, `upstream`,
  `host_disconnected`, `watcher`) and returns whether this call cancelled it.
- `Agent.capabilities()` / `agent.capabilities()` returns what a run of the
  agent supports (resume, preemptive cancel, approval scopes, request kinds,
  parallel tools, delegation, effects, enforcement, lossless events).
- `effects` on tool registration (Python `tool` / `register_tool`, Node
  `registerTool` and handler options): the declared effects (`read`,
  `write_workspace`, `write_outside`, `network`, `credential`, `destructive`,
  `process`), used by permission rules and approval. Without it `side_effect`
  keeps working (`True` means effects unknown).

### Changed

- **Breaking:** the npm package is `orchest-sdk`; it was `@orchest/sdk`
  when built from source.
- **Breaking:** `respond_approval(request_id, approved)` (Python) and
  `respondApproval(requestId, approved)` (Node) are replaced by
  `respond(request_id, payload)` / `respond(requestId, payload)`. `payload` is
  the response: `{"decision": "approve"}`, `{"decision": "deny"}` or
  `{"decision": {"type": "answer", "payload": ...}}`, optionally with
  `"scope": "session"` to remember an approval for the rest of the session.
  An invalid payload raises `ValueError` (Python) or throws (Node).
- **Breaking:** approval events carry `request_id`, `kind` and `payload`;
  answer them by `request_id`. Requests raised by tools arrive as
  `request_opened` with `request.id`.
- **Breaking:** the event stream is lossless: a host that stops reading events
  holds the run back instead of losing events.
- The event unions in `orchest/__init__.pyi` and `types.d.ts` gain
  `message_started`, `message_updated`, `message_ended`, `request_opened`,
  `request_resolved`, `tool_dispatched`, `tool_settled`,
  `permission_decided` and `processes_terminated`, and `run_aborted` carries
  `cancel_reason`.
