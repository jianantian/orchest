# Tool Surface Patterns

This guide covers the permission surface and three runtime patterns for larger
or higher-risk tool surfaces: Draft/Commit tools, deferred tool discovery, and
opt-in parallel tool execution.

## Effects and the Permission Decision

Every tool declares what a call may do as `ToolMetadata::effects`, a set of
`ToolEffect`s: `Read`, `WriteWorkspace`, `WriteOutside`, `Network`,
`Credential`, `Destructive`, `Process`. `ToolEffects::NONE` is a pure
computation; `ToolEffects::UNKNOWN` (the default) counts as having side
effects, so a tool that declares nothing asks before it runs under the default
`Approval::WhenRisky`.

```rust
ToolMetadata {
    effects: ToolEffects::NONE.with(ToolEffect::WriteWorkspace),
    approval: Approval::WhenRisky,
    ..ToolMetadata::default()
}
```

Before every tool call (in-process, MCP, skill, or a delegated child run) the
runtime asks one decision point, `RuntimeConfig::decide`, which answers allow,
ask, or deny and reports it as `RuntimeEvent::PermissionDecided`:

1. The host's `PermissionPolicy` rules, last match wins. A rule selects calls by
   tool name, command pattern, path pattern and effects (globs). Allow rules are
   strict: every command of a chain (`&&`, `||`, `;`, `|`) and every path must
   match, and a command using `$(...)` or backticks never matches an allow rule.
   Ask and deny rules match if any command or path does. Relative paths are
   resolved against `PermissionPolicy::workspace`.
2. With no matching rule, `PermissionPolicy::unmatched` if set.
3. Otherwise the run's custom approval predicate, Draft/Commit defaults,
   `ApprovalMode`, and the tool's `Approval` with its effects.

```rust
let policy = PermissionPolicy::new()
    .workspace("/work")
    .rule(PermissionRule::allow("*").effects([ToolEffect::Read]))
    .rule(PermissionRule::allow("shell").command("git status*"))
    .rule(PermissionRule::allow("write_file").path("/work/**"))
    .rule(PermissionRule::deny("*").path("/etc/**").reason("system files"));
let config = AgentConfig::builder("assistant", "mock/mock")
    .permissions(policy)
    .build()
    .unwrap();
```

The policy content belongs to the host: it maps its own permission levels to
rules over effects, commands and paths. A denied call gets a tool result with
code `PERMISSION_DENIED` and does not run. An ask goes through the request
channel (`ApprovalRequested`, answered with `RunHandle::respond`); answering
with `RequestResponse::approve().for_session()` remembers the approval for the
same tool and input (`grant_key`) for the rest of the session, recorded in the
session log. One-time approvals are not remembered, and a session grant never
overrides a deny rule.

MCP tools take the same path: their effects come from the server's
`annotations` (`readOnlyHint` gives `Read`, plus `Network` with
`openWorldHint`; anything else is unknown) and their approval is
`Approval::WhenRisky`.

### What is enforced

The decision point runs inside the process, so it is a soft guarantee.
`RuntimeConfig::enforcement()` returns `EnforcementFacts` a host can show as
they are: `permission_gate` is `Soft` (or `None` when every call is allowed),
`isolation` is what an injected `ExecutionBackend` reports, `code_execution` is
what the code-execution `ScriptExecutor` reports, and `read_only` is `Hard`
only when an injected backend reports `Hard`. Without such a backend a
read-only setting must not be presented as a hard guarantee. Skill capability
declarations are hints, not an execution boundary, unless a backend the host
injected enforces them.

## Cancellation and Interruptibility

`RunHandle::cancel(Cancellation)` (or `abort()` for a user cancellation) takes
effect at once, not at the next step: the in-flight model call is dropped,
pending approvals resolve as cancelled, and no tool call is dispatched after
it. A tool call already running is dropped only if the tool is interruptible;
otherwise it runs to completion and its result is recorded. By default
`Tool::interruptible` is true for tools without side effects and false for the
rest; a host overrides it per tool with `AgentConfigBuilder::interruptible`.
Tools that keep running can watch `ToolContext::cancel` to stop early.

The run ends with one `RunAborted` whose `cancel_reason` is `User`, `Timeout`,
`Upstream` (the parent run was cancelled), `HostDisconnected` or `Watcher`;
repeated cancels and a cancel racing a normal end add no second terminal event.
Calls the cancellation left without a result are closed in the session ledger
(`TOOL_ABORTED_BEFORE_DISPATCH` or `TOOL_OUTCOME_UNKNOWN`). A host that starts
external processes can inject a `ProcessTerminator`; it is called once on
cancellation and its outcome, failures included, is reported as
`ProcessesTerminated`.

## Draft/Commit vs Normal Approval

Use normal approval for a single action where the model already has enough
information to decide the final call and the user only needs to allow or deny
execution. A typical example is a `write_file` or `send_email` tool with write or network
effects and `approval: Approval::Always` or `Approval::WhenRisky`.

Use Draft/Commit when a high-risk action needs an inspectable preview before it
is applied. The draft tool must be side-effect-free and should return the exact
plan or diff that the commit tool will apply. The commit tool applies that plan
and requires approval by default, even if the run-level approval mode is
otherwise permissive.

Draft and commit tools are linked through metadata:

```rust
ToolMetadata {
    effects: ToolEffects::NONE,
    approval: Approval::Never,
    execution_mode: ToolExecutionMode::Draft {
        commit_tool: "commit_file_write".into(),
    },
    ..ToolMetadata::default()
}
```

```rust
ToolMetadata {
    effects: ToolEffects::NONE.with(ToolEffect::WriteWorkspace),
    approval: Approval::Never,
    execution_mode: ToolExecutionMode::Commit {
        draft_tool: "draft_file_write".into(),
    },
    ..ToolMetadata::default()
}
```

The registry validates that links are reciprocal, non-self-referential, and
non-ambiguous.

## Deferred Tool Discovery

Enable deferred discovery when a run has many tools and sending every schema to
the model would waste context. With `enable_tool_search()`, the first model call
only receives `search_tools`. The model searches by natural language, and the
runtime exposes matching tool schemas to later model calls in the same run.

State transition:

1. Hidden: registered and allowed, but not visible in model schemas and not
   callable by guessed name.
2. Returned: `search_tools` returns a matching schema.
3. Exposed: the schema is appended to `state.tool_defs`.
4. Callable: later model turns may call that tool.

Tradeoffs:

- Use deferred discovery for large registries or MCP-heavy applications.
- Keep it disabled for small registries; direct schemas are simpler and avoid an
  extra model/tool round trip.
- No-result searches expose nothing. Tools with zero text-match score are not
  returned.
- Disabling deferred discovery exposes all normal tool schemas directly and does
  not expose `search_tools`.

## Why Sequential Remains Default

Sequential execution is the default because it preserves the simplest safety
model: one approval at a time, deterministic budget accounting, deterministic
hook order, and straightforward event reading. This is the correct default for
side-effecting tools, approval-gated tools, retry-enabled runs, hooks, handoffs,
and tools that depend on previous tool outputs.

Parallel execution is opt-in:

```rust
let config = AgentConfig::builder("assistant", "mock/mock")
    .enable_parallel_tools()
    .build()
    .unwrap();
```

The runtime only runs a same-turn batch concurrently when every call is
side-effect-free, approval-free, marked `ToolParallelism::ParallelSafe`, and no
hooks or retry policy are active. Mixed serial/parallel batches fall back to the
existing ordered execution path.

## Metadata Fields

`ToolMetadata::execution_mode` controls Draft/Commit semantics:

- `ToolExecutionMode::Normal`: default behavior.
- `ToolExecutionMode::Draft { commit_tool }`: side-effect-free preview path.
- `ToolExecutionMode::Commit { draft_tool }`: approval-gated apply path linked
  to a draft tool.

`ToolMetadata::parallelism` controls whether a tool may participate in opt-in
parallel batches:

- `ToolParallelism::Serial`: default; never runs concurrently.
- `ToolParallelism::ParallelSafe`: eligible only when the run enables parallel
  tools and the tool is not side-effecting or approval-gated.

Parallel batch events include a batch id, model-requested order, and completion
order so consumers can reconstruct both the requested sequence and actual finish
sequence.

## App-Layer Policy Guardrails

Complex authority policy should stay outside the runtime core. The runtime
provides the enforcement hooks: tool input guardrails can allow, modify, reject,
or abort a call before execution, `PermissionPolicy` rules match tools,
commands and paths, and `ToolMetadata` still controls the default approval with
`Approval::Never`, `Approval::WhenRisky`, or `Approval::Always`.

Keep multi-level policy in application code when the decision depends on product
state such as the actor role, workspace, resource owner, risk tier, tenant plan,
or audit workflow. Those rules change more often than the SDK runtime contract,
and product teams need to test and version them independently.

The `guardrail_authority_policy` example shows this split:

- a tool input guardrail rejects a high-risk request from a support actor;
- the same guardrail allows a medium-risk request from a manager;
- the allowed side-effecting tool still triggers `Approval::WhenRisky` before it
  executes.

## Examples

Run the examples from the workspace root:

```bash
cargo run -p orchest --example draft_commit_approval
cargo run -p orchest --example tool_search_discovery
cargo run -p orchest --example parallel_tool_execution
cargo run -p orchest --example approval_when_risky_side_effect
cargo run -p orchest --example guardrail_authority_policy
```
