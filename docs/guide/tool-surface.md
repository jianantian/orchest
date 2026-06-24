# Tool Surface Patterns

This guide covers three runtime patterns for larger or higher-risk tool
surfaces: Draft/Commit tools, deferred tool discovery, and opt-in parallel tool
execution.

## Draft/Commit vs Normal Approval

Use normal approval for a single action where the model already has enough
information to decide the final call and the user only needs to allow or deny
execution. A typical example is a `write_file` or `send_email` tool with
`side_effect: true` and `approval: Approval::Always` or `Approval::WhenRisky`.

Use Draft/Commit when a high-risk action needs an inspectable preview before it
is applied. The draft tool must be side-effect-free and should return the exact
plan or diff that the commit tool will apply. The commit tool applies that plan
and requires approval by default, even if the run-level approval mode is
otherwise permissive.

Draft and commit tools are linked through metadata:

```rust
ToolMetadata {
    side_effect: false,
    approval: Approval::Never,
    execution_mode: ToolExecutionMode::Draft {
        commit_tool: "commit_file_write".into(),
    },
    ..ToolMetadata::default()
}
```

```rust
ToolMetadata {
    side_effect: true,
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
let config = AgentConfig::builder("mock/mock")
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
or abort a call before execution, while `ToolMetadata` still controls coarse
side-effect approval with `Approval::Never`, `Approval::WhenRisky`, or
`Approval::Always`.

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
cargo run -p agent-runtime-core --example draft_commit_approval
cargo run -p agent-runtime-core --example tool_search_discovery
cargo run -p agent-runtime-core --example parallel_tool_execution
cargo run -p agent-runtime-core --example approval_when_risky_side_effect
cargo run -p agent-runtime-core --example guardrail_authority_policy
```
