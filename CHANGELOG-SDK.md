# SDK changelog

Changes to the Python (`orchest-py`) and Node (`@orchest/sdk`) bindings. The
Rust crate's changes are in [CHANGELOG.md](CHANGELOG.md).

## [Unreleased]

Follows the v2.0 runtime contract
([#349](https://github.com/jianantian/orchest/issues/349)).

### Changed

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

### Added

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
