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
2. For deterministic tests, construct a gated model whose first call waits
   for an explicit release signal. Construct the recording watcher and
   configure the LLM watcher as:

   ```rust
   let watcher = LlmWatcher::builder()
       .model(Arc::clone(&model))
       .build();
   ```

3. Start the supervisor and retain its `RunHandle` and `EventReceiver`.
4. Await both `attach_watcher()` calls, then release the deterministic model
   gate. Assert at least one attributable post-registration event without
   claiming that startup events were captured.
5. For the live path, attach both watchers immediately after start and record
   the unclosed race; do not reuse the deterministic ordering claim.
6. Delegate once with `Fresh` and once with `Fork { depth }`, reusing issue
   002's deterministic context assertions.
7. Capture forwarded nested events and prove they are visible through the
   supervisor stream without claiming a child subscription.
8. Trigger watcher injection/steering and external-handle
   injection/steering. Assert which actor's conversation changes.
9. Update the stable start/attach-race, missing-child-handle, and
   child-steering-target findings with the authentic attempt and public-source
   evidence.
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
