# Prime Agent: Long-Running Runtime

> **Capability study:** detached execution, input admission, scheduling,
> reconnect, replay, backpressure, and crash recovery.

## Prime Agent's Runtime Shape

Prime Agent's normal interactive path is not an in-process TUI loop. A detached
supervisor owns public local connections and worker discovery. Each active root
session tree has a worker process that owns the root runtime, scheduler,
persistent kernels, and descendants. A separate catalog process scans inactive
saved sessions.

The topology provides lifecycle and failure containment, not a security
sandbox. Workers and kernels normally retain the user's operating-system
permissions.

The important lesson is not the number of processes. It is the ownership
assignment:

| Responsibility | Owner |
| --- | --- |
| Terminal rendering and local input | client |
| Attachment, routing, worker discovery | supervisor |
| Provider calls, tools, queues, schedules, kernels | session worker |
| Durable transcript and artifacts | session storage |
| Inactive-session scans | catalog process |

The supervisor does not execute model calls or workspace tools. A failed
catalog scan does not interrupt a live agent. A failed worker affects one root
tree rather than every active session.

## Unified Input Admission

After admission, Prime Agent uses the same session execution path for:

- an attached user's prompt;
- steering or follow-up input;
- a heartbeat;
- a cron schedule;
- a persistent-goal continuation;
- autonomous continuation; and
- another agent's message.

This is a stronger abstraction than adding a separate code path for every
producer. The runtime can define one set of busy, queued, cancelled, accepted,
and terminal semantics.

### What belongs in Orchest core

A general input envelope and admission result can be provider- and
product-neutral:

```text
RunInputEnvelope
  id
  source
  content
  delivery_policy
  admitted_at

AdmissionResult
  accepted | queued | rejected
  target_run_id
  queue_position?
  reason?
```

Core should define ordering, cancellation before execution, and how steering
differs from next-turn input. It should not define cron syntax, goal policy, or
heartbeat intervals.

## Scheduling Lessons

Prime Agent persists schedules per session. A due tick is claimed and advanced
before its prompt is delivered. After a crash, an uncertain prompt is not
blindly replayed. Missed ticks coalesce rather than producing an unbounded
backlog.

These are good Code Agent service rules:

- scheduling state is durable and scoped to its target;
- a tick has an identity separate from the prompt it may produce;
- delivery uncertainty is visible;
- retries depend on replay safety;
- a busy target has an explicit coalescing policy; and
- schedule recovery never claims task success.

They should consume a core input-admission API rather than enter Orchest's
runtime loop directly.

## Reconnect and Event Recovery

Prime Agent assigns each worker generation a monotonically increasing event
sequence. Clients retain a `{generation, sequence}` cursor. Reconnect attempts
incremental replay, but a coherent session snapshot remains the recovery
baseline when replay is incomplete or unavailable.

This gives four useful distinctions:

1. transport reconnect is not runtime restart;
2. a sequence number is meaningful only within its generation;
3. replay availability is bounded and observable; and
4. a replacement snapshot is a normal recovery path, not an exceptional
   corruption path.

### Immediate Orchest relevance

Orchest's current secondary subscriptions are explicitly lossy and report
`EventsDropped` to the primary subscriber. The v1.0 issues already identify the
right repairs:

- watcher loss must have an observable recovery contract;
- watchers must be attachable before the first relevant event;
- multiple watcher actions need deterministic arbitration; and
- nested child events must reach declared watchers without duplication.

Orchest does not need a daemon protocol to solve these. It needs event
identity, a bounded replay or resynchronization seam, and a documented state
baseline.

## Mutation Idempotency

Prime Agent keys mutating daemon commands by stable client and command IDs. It
journals receipt before dispatch and records completion afterward. Repeating a
known completed command returns the stored result. A received command with no
durable result is reported as uncertain instead of being replayed blindly.

This is especially relevant to:

- file mutations;
- starting or deleting retained children;
- schedule changes;
- approval responses;
- queued-message changes; and
- destructive process control.

The Code Agent service should own the command journal. Orchest core may expose
operation IDs and replay-safety metadata, but should not embed a local daemon's
journal format.

## Crash Recovery

Prime Agent separates several failure classes:

| Failure | Correct interpretation |
| --- | --- |
| Client disconnect | Agent may still be running |
| Supervisor replacement | Workers may still be running |
| Worker crash | One root tree lost its execution owner |
| Kernel crash | Model-facing execution state needs restoration |
| Provider failure | Run-level runtime failure |
| Tool subprocess uncertainty | Side effect may have occurred |

Orchest's actor supervisor currently restarts on actor failure, while v1.0
issue 003 addresses terminal `RunFailed` results that should trigger bounded
restart. That work should preserve the same distinction: an actor crash and a
declared run failure are different evidence and may have different replay
safety.

## Recommended Layering

### Orchest core

- input admission and cancellation semantics;
- public child control;
- event identity and explicit loss;
- bounded replay/resynchronization seam;
- deterministic watcher arbitration;
- structured restart eligibility and attempts;
- operation replay-safety metadata.

### Orchest Code Agent runtime

- context construction for scheduled or agent-origin input;
- persistent execution-session restoration;
- artifact and trajectory projection;
- workspace-aware safety classification.

### Code Agent service

- supervisor and worker processes;
- residency and passivation;
- cron and heartbeat scheduling;
- command journal and idempotency;
- reconnect protocol and snapshots;
- process registry and orphan cleanup;
- update coordination.

## What Not to Copy

- Do not put daemon wire types into Orchest public events.
- Do not require every Orchest embedding to run a supervisor process.
- Do not equate detached execution with sandboxing.
- Do not retain unbounded subscriber queues or infinite replay history.
- Do not automatically replay uncertain side effects after a crash.
- Do not treat a reached autonomous limit as successful completion.

## Verification Scenarios

An Orchest Code Agent long-running layer should eventually prove:

1. detach and reattach without stopping the run;
2. client restart while the worker remains alive;
3. worker restart from the latest safe state baseline;
4. event loss followed by bounded replay or snapshot resynchronization;
5. duplicate command delivery without duplicate side effects;
6. accepted-but-uncertain mutation reporting;
7. busy-target schedule coalescing;
8. retained child completion after the parent turn ends; and
9. provider failure remaining distinguishable from transport failure.
