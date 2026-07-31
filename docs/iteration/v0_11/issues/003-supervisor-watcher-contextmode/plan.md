# 003 implementation plan

## Files to read

- `crates/orchest/src/tool/agent_as_tool.rs`
- `crates/orchest/src/run/handle.rs`
- `crates/orchest/src/run/supervisor.rs`
- `crates/orchest/src/run/llm_watcher.rs`
- `crates/orchest/src/run/watcher.rs`
- `crates/orchest/src/run/tests.rs`

## Files to change

- `examples/demo/research-pipeline/src/supervisor.rs`
- `examples/demo/research-pipeline/src/watcher.rs`
- `examples/demo/research-pipeline/src/main.rs`
- `examples/demo/research-pipeline/tests/supervisor_watcher.rs`
- `examples/demo/research-pipeline/findings.json`

## Steps

1. Build the worker as an `AgentAsTool` and register it with the supervisor.
2. For deterministic tests, construct a two-stage gated model whose first
   call returns a harmless supervisor probe and whose second call delegates
   only after both watcher processors prove activation. Construct the
   recording watcher and configure the LLM watcher as:

   ```rust
   let watcher = LlmWatcher::builder()
       .model(Arc::clone(&model))
       .build();
   ```

3. Start the supervisor and retain its `RunHandle` and `EventReceiver`.
4. Await both `attach_watcher()` calls, release the first gate, let the probe
   `RunStep` end so queued subscriptions activate, and gate the second model
   call. Release delegation only after both watcher wrappers record that
   second-step `ModelCallStarted`, without claiming startup-event capture.
5. For the live path, attach both watchers immediately after start and record
   the unclosed race; do not reuse the deterministic ordering claim.
6. Delegate once with `Fresh` and once with `Fork { depth }`, reusing issue
   002's deterministic context assertions.
7. Capture supervisor actor-emitted events through attached watchers and
   forwarded nested events through the primary supervisor `EventReceiver`.
   Record events only after each watcher's `on_event()` completes, gate both
   processors through a terminal supervisor event, and prove those completed
   event vectors did not receive the forwarded events. Record this routing
   gap as SB-8 without claiming a child subscription.
8. Trigger watcher injection/steering and external-handle
   injection/steering. Trigger the custom watcher only from the
   supervisor-level delegation
   `ToolCallStarted { tool: "research_worker", .. }`, never a child or nested
   event. Assert which actor's conversation changes.
9. Update the stable start/attach-race, missing-child-handle,
   child-steering-target, and forwarded-event-routing findings with the
   authentic attempt and public-source evidence.
10. Validate the canonical file after evidence updates.

## Verification

```bash
cargo test -p research-pipeline-demo --test supervisor_watcher
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```

No assertion may say “worker attached” or “worker steered” unless the event
and conversation evidence identify the nested worker as the target.
No live assertion may say “attached before delegation” unless a future public
runtime seam makes that ordering explicit.
