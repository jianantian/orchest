# Prime Agent: RLM and Retained Agents

> **Capability study:** asynchronous child admission, retained child state,
> run trees, communication, and usage attribution.

## Why This Is New

The May 2026 pi-agent comparison described a single-agent loop. Current Prime
Agent has a materially different runtime model:

- the model calls `rlm(prompt, name=..., model=...)` from a persistent Python
  environment;
- the call returns after task admission with a child handle;
- the child runs as an independent session with its own context and artifacts;
- results arrive later through explicit agent messages or shared files;
- completed children may remain addressable;
- a parent-scoped registry survives compaction, kernel restart, and session
  restoration; and
- child usage is attributed into the parent's aggregate while remaining
  distinguishable in the context tree.

This is not merely "sub-agents added to pi." It separates delegation admission,
execution, result delivery, observation, and retention.

## Prime Agent's Contract

### Admission handle

The return value confirms admission, not success. Its meaningful fields are a
stable child identifier, a readable name, a session directory, and the chosen
model. It intentionally contains no answer.

This lets the parent admit several children independently and end its turn
without blocking on them.

### Result delivery

Children reply through explicit agent-to-agent messages or files. The parent
does not poll an `rlm()` future for the final answer. That prevents a long child
from holding the parent's current tool call open and makes background work a
normal lifecycle.

### Registry and retention

The authoritative registry lives in the host runtime, not in the Python
namespace. Kernel loss therefore does not erase child identity. The registry
tracks running, completed, and failed states and can rehydrate retained daemon
children.

### Policy inheritance

Children inherit provider hooks, resources, configured tools, retry policy,
thinking configuration, and usually the parent model. Depth and requested
model selection are validated by the host. The Python shim cannot silently
bypass these controls.

### Accounting

Child usage contributes to the parent session's billable aggregate, but tree
reporting distinguishes the child's own usage from the parent's model context.
This avoids both missing cost and double-counting context.

## Orchest's Current Model

Orchest's `AgentAsTool` provides strong synchronous delegation:

- `Fresh` or bounded `Fork` context;
- child budget capped by the parent's remaining budget;
- shared approval routing;
- recursive child events;
- depth limit;
- structured failure classification; and
- an optional output contract with one correction attempt.

The missing property is lifecycle ownership. `AgentAsTool::execute` starts the
child, consumes the child's event stream, waits for a terminal event, and then
returns a Tool result. The Tool internally owns the handle.

The v1.0 delegated-child-control issue correctly identifies the immediate
public seam gap: callers cannot independently control or await the child.

## Recommended Orchest Semantics

### Keep synchronous Agent-as-Tool

Do not add `background: bool` to `AgentAsTool`. Synchronous delegation has a
clear contract: the parent requested a value and cannot continue that tool call
without it.

### Add retained delegation

A separate core primitive should return after admission:

```rust
pub struct DelegatedRunHandle {
    pub child_run_id: RunId,
    pub parent_run_id: RunId,
    // private routing and lifecycle state
}

impl DelegatedRunHandle {
    pub fn inject_message(&self, message: &str) -> Result<(), ControlError>;
    pub fn steer(&self, instruction: &str) -> Result<(), ControlError>;
    pub fn abort(&self, reason: Option<&str>) -> Result<(), ControlError>;
    pub async fn subscribe_events(&self, cursor: Option<EventCursor>)
        -> Result<EventSubscription, ControlError>;
    pub async fn wait(&self) -> Result<RunOutcome, RunError>;
    pub async fn status(&self) -> Result<DelegatedRunStatus, ControlError>;
}
```

The exact Rust shape remains an implementation decision. The semantic
requirements are stable:

- admission failure is returned synchronously;
- completion is independently awaitable;
- control targets the child, never the parent by accident;
- the child has a stable public identity;
- terminal status remains inspectable after the initial caller stops waiting;
- loss of a local actor reference does not redefine child identity; and
- no private actor or channel types leak into the public API.

### Make the run tree explicit

The runtime should be able to describe direct parent-child edges without
requiring a product database:

```text
RunTree
  root RunId
  parent_of(child)
  children_of(parent)
  status(run)
  depth(run)
```

Persistence of that tree should be an optional store seam. Orchest core should
not decide how long a completed child remains resident or how a UI lists it.

### Add typed agent messages only after child control

Agent-to-agent messages need stable endpoints first. A minimal message should
carry:

- message ID;
- sender and receiver Run IDs;
- delivery mode (`inject_next_turn`, `steer_current`, or application-defined);
- content blocks;
- accepted/delivered/rejected state; and
- an attributable failure reason.

Role names such as `parent` and `child` are convenient selectors, not durable
identity. Durable routing should resolve them to Run IDs before admission.

### Preserve budget and approval invariants

Detached children must not escape the current synchronous guarantees:

- child budget is capped at admission;
- child consumption remains attributable after the parent turn ends;
- approvals route by child Run ID;
- cancelling a parent follows an explicit descendant policy; and
- retention never implies unlimited execution.

## Event Contract

Retained delegation needs lifecycle events distinct from nested runtime events:

```text
ChildAdmissionRequested
ChildAdmitted
ChildAdmissionRejected
ChildStatusChanged
ChildMessageAccepted
ChildMessageDelivered
ChildMessageRejected
ChildReleased
```

`ChildRunEvent` can continue to wrap the child's ordinary `RuntimeEvent`s. A
consumer must be able to distinguish "the child was admitted" from "the child
started a model call" and "the child completed."

## Failure and Recovery Questions

The contract should decide, rather than infer:

- whether parent completion cancels, detaches, or retains children;
- whether parent failure changes an already-admitted child's fate;
- whether completed children accept new messages by starting a continuation;
- how restored children prove that their persisted Run ID matches the runtime
  instance being controlled;
- what happens when a message was accepted but delivery is uncertain; and
- how usage is reconciled after recovery.

Prime Agent supplies useful answers, but Orchest should express them as small
runtime contracts instead of inheriting Prime's daemon-specific state machine.

## Priority for Orchest

1. Complete v1.0 issue `001-delegated-child-control` as a real public control
   surface.
2. Preserve synchronous `AgentAsTool` and introduce retained delegation as a
   separate semantic path.
3. Make child event delivery reliable through v1.0 issues 002, 004, and 005.
4. Add a run-tree store seam and retained-child policy after the v1.0 public
   control API is stable.
5. Add typed agent messaging only once endpoints, admission, and recovery are
   explicit.

## What Not to Copy

- Do not make a Python callable the only public delegation API.
- Do not encode the run tree only in filesystem paths.
- Do not return a fake "result" from admission.
- Do not silently fall back from requested model or Fork context.
- Do not make all descendants globally discoverable; default scope should be
  the caller's declared run tree.
- Do not conflate retained residency with durable product ownership.
