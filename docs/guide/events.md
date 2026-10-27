# Events reference

A run exposes two different records. Keep them apart:

| | Session log (`SessionEvent`) | Event stream (`RuntimeEvent`) |
|---|---|---|
| Purpose | Execution state: what the run did, enough to resume it | Observation: what a host shows, logs or reacts to |
| Written by | The run, at checkpoints, into the `SessionStore` | The run, as things happen, to `EventReceiver` and subscribers |
| Complete | Yes; the run cannot continue without it | Primary receiver: yes. Secondary subscribers may drop (`events_dropped`) |
| Contains deltas | No | Yes (`message_updated`, `model_stream_chunk`) |
| Replayable | `AgentRun::resume` rebuilds the run from it | No; missed events are not replayed |

A host that needs exact state reads the session log (or asks the handle:
`RunHandle::pending_requests`, `RunHandle::cancellation`). A host that renders
progress reads the stream. Everything below is the stream.

Every event in the binding wire shape also carries `type` (the snake_case name
below), `run_depth` and `child_run_id`. `js/types.d.ts` and
`python/orchest/__init__.pyi` list the fields of each.

## Run lifecycle

| Event | Meaning |
|---|---|
| `run_started` | The run began. |
| `run_restarted` | The supervisor restarted a crashed worker (`attempt`). |
| `run_completed` | Finished normally. Check `stop_reason`: `max_tokens` means `output` is truncated. |
| `run_failed` | Finished with an error (`kind` classifies it). |
| `run_aborted` | Stopped early. `cancel_reason` is set for a cancellation (`user`, `timeout`, `upstream`, `host_disconnected`, `watcher`), unset when the supervisor gave up. |
| `processes_terminated` | The injected `ProcessTerminator` ran on cancellation: `terminated` count or `error`. |
| `agent_updated` | A handoff switched the active agent. |

## Messages

Each model call produces one assistant message with a fresh `message_id`.

| Event | Meaning |
|---|---|
| `model_call_started` | A step's model call began (`step`). |
| `message_started` | Message `message_id` began (`step`). |
| `message_updated` | An increment: `delta` is `text`, `thinking` (reasoning, kept apart from visible text) or `tool_use_args` (JSON argument chunk for `tool_use_id`). |
| `message_ended` | Message complete: final `content` blocks and model `stop_reason`. On a failed or cancelled call `stop_reason` is unset and `content` is empty. |
| `model_stream_chunk` | The raw provider chunk the deltas come from. Prefer the message events. |
| `model_call_completed` | Token usage and any option adjustments for the call. |
| `model_retry` | The call failed retryably and restarts; drop the deltas buffered for the current message: the retry restarts it under the same `message_id`. |
| `context_compacted` | Older messages were summarised. |

## Requests

Everything the run waits on the host for is a request on one channel:
tool approvals (`tool_approval`), retry approvals (`retry_approval`), and
host-defined kinds raised by tools with `ToolContext::request`. Answer any of
them with `RunHandle::respond(request_id, response)`.

| Event | Meaning |
|---|---|
| `request_opened` | A request is waiting: `request` has `id`, `run_id`, `kind`, `payload`, `target`. |
| `request_resolved` | It ended: responded, withdrawn, timed out or cancelled (`resolution`). |
| `permission_decided` | The permission policy ruled on a tool call: allow, ask or deny, with its basis. |
| `approval_requested` | Approval-specific view of a `tool_approval` request (same `request_id`), emitted after `request_opened`. |
| `approval_granted` / `approval_denied` | Approval-specific view of the resolution. |

Approval and retry requests are also recorded in the session log, so a resumed
run still sees them. Requests raised by tools are only on the stream.

## Tool ledger

The ledger events mirror the session log's dispatch and result records, so a
host can tell which calls may have had effects:

| Event | Meaning |
|---|---|
| `tool_dispatched` | The call was handed to its tool (`tool_use_id`, `tool_name`). |
| `tool_settled` | The call has a recorded result: `is_error` and `error_code` (including `TOOL_OUTCOME_UNKNOWN`, `TOOL_NOT_STARTED`, `TOOL_ABORTED_BEFORE_DISPATCH`). |

They are reported when the run checkpoints, so they trail the matching
`tool_call_*` events slightly. A call that is rejected before dispatch (unknown
tool, permission denied) settles without a `tool_dispatched`. A dispatched call
with no `tool_settled` at the end of the stream had an unknown outcome.

## Tool execution detail

| Event | Meaning |
|---|---|
| `tool_call_started` / `tool_call_update` / `tool_call_completed` / `tool_call_failed` / `tool_call_retry` | Per-call progress, partial output, result, failure and retry. |
| `tool_call_batch_started` / `tool_call_batch_item_started` / `tool_call_batch_item_completed` | A parallel batch and the order its items started and finished in. |
| `async_tool_started` / `async_tool_progress` / `async_tool_completed` | A background job of an async tool. |

## Delegation

| Event | Meaning |
|---|---|
| `sub_agent_started` / `sub_agent_completed` / `sub_agent_failed` | A child run's lifecycle. |
| `child_run_event` / `sub_agent_event` | A child's event, wrapped with its depth and ids. |

## Warnings and diagnostics

| Event | Meaning |
|---|---|
| `budget_warning` | Usage crossed the warning threshold. |
| `runtime_warning` | A non-fatal runtime problem. |
| `skill_content_read` / `skill_missing_capabilities` / `skill_load_warning` | Skill loading. |
| `hook_panicked` | A hook panicked; the run continued. |
| `events_dropped` | A subscriber missed `count` events (`from_seq`..`to_seq`). |

## Capabilities

`RunHandle::capabilities()` (and `AgentConfig::capabilities()` before a run)
says what the run supports, derived from its configuration: resume, preemptive
cancel, approval scopes, request kinds, parallel tools, delegation, the effect
classification, enforcement levels and lossless delivery.
`Capabilities::require(Capability::…)` returns a `NotSupported` error instead
of letting the host find out by failing.
