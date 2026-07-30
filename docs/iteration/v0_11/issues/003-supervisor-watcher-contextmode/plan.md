# 003 implementation plan

## Files to read

- `crates/orchest/src/tool/agent_as_tool.rs`
- `crates/orchest/src/run/handle.rs`
- `crates/orchest/src/run/supervisor.rs`
- `crates/orchest/src/run/llm_watcher.rs`
- `crates/orchest/src/run/watcher.rs`

## Files to change

- `examples/demo/research-pipeline/src/supervisor.rs`
- `examples/demo/research-pipeline/src/watcher.rs`
- `examples/demo/research-pipeline/src/main.rs`
- `examples/demo/research-pipeline/tests/supervisor_watcher.rs`
- `examples/demo/research-pipeline/findings.json`

## Steps

1. Build the worker as an `AgentAsTool` and register it with the supervisor.
2. Start the supervisor, retain its `RunHandle` and `EventReceiver`, and
   attach the `LlmWatcher` plus a deterministic recording watcher.
3. Configure the LLM watcher as:

   ```rust
   let watcher = LlmWatcher::builder()
       .model(Arc::clone(&model))
       .build();
   ```

4. Delegate once with `Fresh` and once with `Fork { depth }`, reusing issue
   002's deterministic context assertions.
5. Capture forwarded nested events and prove they are visible through the
   supervisor stream without claiming a child subscription.
6. Trigger watcher injection/steering and external-handle
   injection/steering. Assert which actor's conversation changes.
7. Record the missing child handle and child steering target in their stable
   finding entries, including the authentic attempt and public-source
   evidence.
8. Validate the canonical file after evidence updates.

## Verification

```bash
cargo test -p research-pipeline-demo --test supervisor_watcher
cargo run -p research-pipeline-demo --bin seam-report -- validate \
  --findings examples/demo/research-pipeline/findings.json
```

No assertion may say “worker attached” or “worker steered” unless the event
and conversation evidence identify the nested worker as the target.
