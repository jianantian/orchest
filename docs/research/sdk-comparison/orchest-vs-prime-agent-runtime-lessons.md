# Orchest vs Prime Agent: Runtime Lessons

> **Date:** 2026-08-11
>
> **Orchest baseline:** `834b697`
>
> **Prime Agent baseline:** `14d6e7491` (`v0.7.1` plus unreleased changes)
>
> **Scope:** Prime Agent as a reference for an Orchest-based Code Agent, not as
> a Multivac Runtime provider.

## Executive Summary

Orchest and Prime Agent operate at different layers. Orchest is a low-level,
provider-neutral Rust runtime SDK. Prime Agent is a complete local coding and
research agent whose runtime, daemon, TUI, persistent Python kernel, schedules,
and session storage ship together.

The useful comparison is therefore not "which product has more features." It
is "which Prime Agent mechanisms reveal a missing reusable contract in Orchest,
and which belong only in an Orchest Code Agent product layer."

The central findings are:

1. Orchest already has the stronger reusable core for approvals, budgets,
   guardrails, Tool/MCP/Skill boundaries, provider neutrality, structured
   events, and synchronous Agent-as-Tool delegation.
2. Prime Agent is ahead in retained asynchronous agents, persistent
   model-facing execution state, long-running input admission, process
   continuity, and append-only operational recovery.
3. Orchest should absorb Prime Agent's separation of **child admission from
   child completion**, but should not absorb its daemon, scheduler, goals, or
   continual harness into the core SDK.
4. The correct target has three layers: Orchest core, Orchest Code Agent
   runtime, and Code Agent service. Copying Prime Agent into one runtime object
   would erase Orchest's strongest architectural advantage.

## Comparison Boundary

```text
Orchest core
  loop · state · events · Tool/MCP/Skill · approval · budget · child control

Orchest Code Agent runtime
  persistent execution · workspace tools · context · artifacts · harness state

Code Agent service
  worker residency · schedules · reconnect · recovery · command durability
```

Features should move downward only when they are product-neutral and useful to
multiple classes of agent application. A heartbeat scheduler is useful to a
coding agent, but it is not required to run an agent loop. A public retained
child handle is different: it is a general runtime primitive and belongs in
the core.

## Architecture Comparison

| Concern | Orchest | Prime Agent | Lesson |
| --- | --- | --- | --- |
| Runtime boundary | Minimal SDK core with Rust/Python/Node bindings | Product runtime plus local daemon and TUI | Preserve Orchest's smaller public surface |
| Model abstraction | Provider registry and protocol-oriented provider crates | Unified TypeScript provider package | Both validate provider separation; Orchest should retain its protocol wall |
| Tool model | Typed `Tool`, registry, MCP normalization, approval metadata | Generic loop plus product tools; built-in RLM path emphasizes IPython | Persistent execution should remain a Tool/capability, not replace Tool semantics |
| Skill model | Standard procedural-knowledge package with progressive disclosure | Standard skill metadata plus executable Python packages | Keep Orchest's stricter Skill/Tool distinction |
| Approval and safety | Run policy, per-tool approval, guardrails, injectable executor | Extension gates; default model-generated Python has user permissions | Do not copy Prime's trust model |
| Budget and failure | Token/tool/time/cost budgets, structured failure and retry safety | Goal/autonomous budgets and usage attribution | Orchest core is stronger; Code Agent may add higher-level goal budgets |
| Session persistence | Versioned point-in-time snapshots through `SessionStore` | Append-only JSONL transcript plus artifact and operation state | Code Agent needs a journal in addition to core snapshots |
| Context compaction | Provider-neutral compaction with tool boundary protection | Compaction integrated with transcript, kernel, child registry, and harness | Add extension state anchors; do not make compaction own product state |
| Child agents | `AgentAsTool` waits for completion and returns typed output | `rlm()` returns after admission; retained child continues independently | Add retained delegation without changing Agent-as-Tool semantics |
| Mid-run control | Abort, inject, steer, watchers | Steering/follow-up queues and agent messages | Generalize input admission and ordering |
| Long-running execution | In-process actor supervision and snapshots | Detached workers, schedules, reconnect, replay, recovery journals | Core needs reliable event/control seams; service owns processes |
| Programmable execution | Explicit one-shot `ScriptExecutor` | Persistent IPython namespace and typed host bridge | Add a Code Agent `ExecutionSession` abstraction |
| Continual improvement | Skills, hooks, application-local eval harnesses | Session-local/global harness ledger with refinement and rollback | Keep refinement outside core and gate it with evidence |

## Stable Lessons to Absorb

### 1. Admission is not completion

Orchest's current `AgentAsTool` deliberately waits for the child run and
returns its output as a tool result. That is the right semantic for bounded
delegation with an immediate consumer.

Prime Agent demonstrates a second semantic: admit a child, return a stable
handle immediately, let the parent continue, and deliver results later through
messages, files, or explicit observation. This is not an option flag on
Agent-as-Tool. It is a different lifecycle contract.

Orchest should keep both explicit:

- `AgentAsTool`: synchronous, output-producing delegation.
- retained delegation: asynchronous admission returning a public child handle.

See [RLM and retained agents](./prime-agent-rlm-and-retained-agents.md).

### 2. Every input needs an admission contract

Prime Agent routes user prompts, steering, follow-ups, heartbeats, schedules,
goal continuations, autonomous continuations, and agent messages through the
same session execution path. The source changes; ordering and execution
ownership do not.

Orchest already exposes `inject_message` and `steer`, but the public API does
not yet describe durable admission, source identity, queue position,
coalescing, cancellation before execution, or accepted/running/completed
states. A generic input-admission contract belongs in core. Heartbeat and cron
producers do not.

### 3. Snapshot, transcript, artifact, and operation journal are different

A snapshot reconstructs current loop state. A transcript explains what the
model and tools observed. Artifacts hold large or external outputs. An
operation journal resolves whether a side-effecting command was received,
completed, or left uncertain after a crash.

Prime Agent's recovery work is strongest when it treats these separately.
Orchest's `SessionSnapshot` should remain focused; an Orchest Code Agent should
add append-only trajectory and operation stores instead of stretching
`SessionStore` into every durability concern.

### 4. Recovery uses a state baseline, not infinite event retention

Prime Agent's local client reconnect model uses generation-aware cursors for
incremental replay and a coherent snapshot when replay is unavailable. This is
a sound general principle: replay is an optimization, while a durable state
baseline is the recovery contract.

Orchest's v1.0 watcher-loss and pre-run-attachment issues are the immediate
place to make event loss explicit. A daemon protocol is not required to define
sequence, loss, and resynchronization semantics.

See [long-running runtime](./prime-agent-long-running-runtime.md).

### 5. Model-facing programmability needs an authoritative host

Prime Agent's Python kernel is useful because it can retain variables and
compose work programmatically. Its stronger architectural decision is that the
kernel does not own provider calls, credentials, transcripts, child policy, or
schedules. Typed host requests cross back to the authoritative runtime.

An Orchest Code Agent can adopt persistent execution without putting business
truth in Python. See
[persistent execution and harness](./prime-agent-persistent-execution-and-harness.md).

## Where Orchest Is Already Stronger

- Provider-independent typed runtime contracts and multi-language bindings.
- Explicit Tool/MCP/Skill separation.
- Approval modes, guardrails, and executor injection.
- Four-dimensional budget enforcement and child budget propagation.
- Structured Tool failure, retry safety, and run failure taxonomy.
- Synchronous Agent-as-Tool with Fresh/Fork context and output contracts.
- Hooks and watchers as application-owned intervention seams.
- Provider-independent eval evidence rather than self-modifying runtime policy.

These should not be redesigned merely to resemble Prime Agent.

## Structural Debt Not to Copy

Prime Agent's product features accumulate in very large session, interactive,
and daemon objects. The result works, but it couples queues, compaction, goals,
refinement, child lifecycle, scheduling, persistence, provider behavior, and UI
concerns inside a small number of files.

Orchest should reject the following transfers:

- daemon commands or scheduling APIs in the core loop;
- a persistent Python kernel as the only Tool surface;
- Python-backed Skill semantics that blur knowledge and capability;
- implicit mutation of reusable global harness state;
- treating process isolation as a security sandbox;
- exposing an internal local daemon protocol as the public hosted protocol.

## Research Set

- [RLM and retained agents](./prime-agent-rlm-and-retained-agents.md)
- [Long-running runtime](./prime-agent-long-running-runtime.md)
- [Persistent execution and continual harness](./prime-agent-persistent-execution-and-harness.md)
- [Adoption map](./prime-agent-lessons-adoption-map.md)

The older [orchest vs pi-agent-core gap analysis](./orchest-vs-pi-agent-gap-analysis.md)
remains useful as a May 2026 historical baseline, but its conclusions do not
describe current Prime Agent.

## Evidence Base and Drift

Prime Agent evidence was inspected at revision `14d6e7491`, primarily from:

- `README.md`;
- `packages/agent/src/agent-loop.ts` and `agent.ts`;
- `packages/coding-agent/docs/{architecture,agent-connection,daemon,rlm,rlm-runtime,acp}.md`;
- `packages/coding-agent/src/core/{agent-session,agent-session-runtime,rlm-runtime}.ts`;
- `packages/coding-agent/src/modes/daemon/daemon-protocol.ts`;
- `packages/coding-agent/src/modes/acp/`; and
- `prime-agent-runtime/src/rlm/`.

Orchest evidence was inspected at revision `834b697`, primarily from:

- `docs/polaris/{overview,concept-boundaries,non-goals}.md`;
- `docs/archive/iteration/v1_0/`;
- `crates/orchest/src/run/`;
- `crates/orchest/src/tool/agent_as_tool.rs` and `code_exec.rs`;
- `crates/orchest/src/session/`; and
- `crates/orchest/src/events.rs`.

Prime Agent is evolving quickly. One concrete drift signal is that its daemon
architecture document still describes protocol v4 while the inspected source
declares protocol v7 and schema revision 16. For newer capabilities, source and
focused tests take precedence over summary documentation. The capability
studies should be re-baselined before they become implementation contracts.
