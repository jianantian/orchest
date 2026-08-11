# Prime Agent: Persistent Execution and Continual Harness

> **Capability study:** persistent model-facing computation, typed host
> requests, session-local supplemental state, refinement, and rollback.

## Persistent Execution Is the Product Differentiator

Prime Agent exposes a persistent IPython kernel as its primary model-facing
tool. Variables, imports, parsed data, functions, and child handles survive
across tool calls and context compaction. Shell commands can be composed from
the same programming environment.

Orchest currently exposes trusted Python and JavaScript execution through an
injected `ScriptExecutor`. The built-in wrappers start from a fresh namespace
for each execution. This is the correct minimal SDK default: execution is
explicitly injected, bounded by timeout, and not assumed to be safe merely
because it is a subprocess.

An Orchest Code Agent, however, benefits from an optional persistent execution
session. It should be additive rather than a replacement for one-shot Tools.

## Proposed Code Agent Capability

The product layer needs an abstraction similar to:

```rust
#[async_trait]
pub trait ExecutionSession: Send + Sync {
    async fn execute(&self, request: ExecuteRequest)
        -> Result<ExecuteOutcome, ExecutionError>;
    async fn interrupt(&self) -> Result<(), ExecutionError>;
    async fn snapshot(&self) -> Result<ExecutionSnapshotRef, ExecutionError>;
    async fn restore(&self, snapshot: &ExecutionSnapshotRef)
        -> Result<(), ExecutionError>;
    async fn close(&self) -> Result<(), ExecutionError>;
    fn capabilities(&self) -> ExecutionCapabilities;
}
```

This belongs in an Orchest Code Agent runtime crate or application layer unless
multiple non-coding Orchest consumers prove they need the same abstraction.

Required semantics:

- one session has explicit working-directory and environment ownership;
- executions within a session are serialized unless capabilities say
  otherwise;
- stdout, stderr, structured values, images, diffs, and errors are distinct;
- interrupt is distinguishable from process death;
- snapshot support is capability-driven, not assumed;
- restoration failure is loud and does not silently start a fresh namespace;
- the executor reports its containment guarantee; and
- credentials do not enter the model-facing namespace by default.

## Typed Host Bridge

Prime Agent's Python shim sends typed host requests back to the TypeScript
runtime for operations such as child admission, goal state, compaction, agent
messages, and scheduling. This preserves a critical authority boundary:

```text
model-generated Python
        ↓ request
persistent execution session
        ↓ typed bridge
authoritative host runtime
        ↓
provider · transcript · budget · approval · child policy · scheduler
```

The kernel is a programmable client of runtime capabilities, not the owner of
runtime truth.

For Orchest Code Agent:

- host methods should resolve to existing Tool or runtime-control contracts;
- every request should carry run, execution, and operation identity;
- host-side schema validation is mandatory;
- unsupported request types fail rather than disappearing;
- cancellation propagates in both directions; and
- execution-session restart must not invent completion for outstanding host
  requests.

The bridge must not become a private alternative to MCP or Tool registration.
It is appropriate only for capabilities whose authoritative state is owned by
the embedding runtime.

## Continual Harness State

Prime Agent stores supplemental prompts, memories, reusable skill descriptions,
sub-agent specifications, and refinement events in a harness ledger. A
dedicated review can propose small edits. Applied edits record before/after
state so they can be rolled back. The immutable base system prompt is not
rewritten.

The attractive part is not "the agent edits itself." It is the constrained
state model:

- base instructions remain immutable;
- supplemental state is typed and scoped;
- changes are small and reviewable;
- history is append-only;
- rollback is based on recorded snapshots; and
- session-local changes are distinct from explicitly global changes.

## Preserve Orchest's Boundaries

Prime Agent describes Python-backed skills as a superset of instruction-only
skills. Orchest should not adopt that definition.

In Orchest:

- a **Skill** remains procedural knowledge discovered through `SKILL.md`;
- a **Tool** remains an executable capability;
- MCP remains a provider protocol for Tools;
- a harness memory is supplemental runtime state, not automatically a Skill;
- a reusable agent specification is configuration, not a Skill; and
- an executable Python package exposes Tools or scripts even when distributed
  beside a Skill.

This keeps progressive disclosure compatible with the open Skill format and
prevents self-refinement from silently changing executable authority.

## Suggested Harness Model

A Code Agent harness ledger could contain:

```text
HarnessEntry
  SupplementalInstruction
  Memory
  AgentSpecification
  SkillRecommendation
  RefinementEvent

RefinementEvent
  proposal_id
  evidence_refs
  requested_scope
  changes[]
  before_revision
  after_revision
  decision
```

`SkillRecommendation` is deliberately not an installed Skill. It may recommend
creating or updating one through the normal reviewed Skill workflow.

## Refinement Policy

Self-improvement should begin conservative:

1. session-local by default;
2. proposal before application;
3. explicit evidence from the trajectory or eval result;
4. bounded number and size of changes;
5. no executable code or permission expansion;
6. recorded before/after revisions;
7. deterministic rollback; and
8. promotion to project/global scope only through an external decision.

Orchest's existing eval work is an important advantage. Refinement should be
accepted because a declared evaluation improves, not merely because a review
model says the new instruction sounds better.

## Security Boundary

Prime Agent clearly states that its worker and kernel processes are not
sandboxes. An Orchest Code Agent should go further because it already has an
injectable `ScriptExecutor` boundary.

Persistent execution must declare:

- filesystem visibility;
- network access;
- environment-variable policy;
- process-spawn capability;
- resource limits;
- snapshot confidentiality;
- secret-redaction behavior; and
- whether approved operations can escape the workspace.

Approval gates alone do not constrain paths or side effects inside arbitrary
Python. A persistent kernel running with full user permissions must be labelled
accordingly and should not be the safe default.

## What Belongs Where

### Orchest core

- existing executor injection and Tool policy;
- typed runtime controls used by the host bridge;
- event and budget attribution;
- extension hooks for persistence and compaction state.

### Orchest Code Agent runtime

- `ExecutionSession` implementations;
- kernel or REPL lifecycle;
- rich execution outputs;
- host-bridge method registry;
- session-local harness ledger;
- refinement proposal and rollback logic;
- workspace safety integration.

### Code Agent service

- persistent session residency;
- snapshot storage and encryption policy;
- orphan process cleanup;
- cross-process restoration;
- scheduled refinement evaluation.

## What Not to Copy

- Do not make the persistent kernel mandatory for every Orchest agent.
- Do not place provider credentials in the kernel namespace.
- Do not let Python own transcript or child lifecycle truth.
- Do not redefine Skill as executable package plus instructions.
- Do not allow refinement to mutate the immutable base prompt.
- Do not promote session-local memories globally without an external decision.
- Do not claim process separation is a sandbox.
