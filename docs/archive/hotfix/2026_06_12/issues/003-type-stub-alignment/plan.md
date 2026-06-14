# 003 · TypeScript / Python Type Stub 对齐 — 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` or `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Make TypeScript and Python event type stubs match the runtime events and binding wire shape for `run_aborted` and `events_dropped`.

**Architecture:** Do not change Rust `RuntimeEvent` variants for this issue. Update language stubs and Node wire metadata naming so generated JS objects and declared TypeScript types agree.

**Tech Stack:** Rust napi binding, TypeScript declaration files, Python `.pyi` stubs, shell lint script.

---

## 要读的现有代码

- `crates/agent-runtime-core/src/events.rs`
- `crates/agent-runtime-node/src/lib.rs`
- `crates/agent-runtime-py/src/lib.rs`
- `js/index.d.ts`
- `js/index.ts`
- `python/agent_runtime/__init__.pyi`
- `scripts/check-ts-event-wire-naming.sh`

## 文件改动

- Modify: `crates/agent-runtime-node/src/lib.rs`
- Modify: `js/index.d.ts`
- Modify: `js/index.ts` if kept as source-of-truth TS surface
- Modify: `python/agent_runtime/__init__.pyi`
- Test/Script: `scripts/check-ts-event-wire-naming.sh` only if additional pattern checks are needed

## 步骤

### 1. Node binding metadata 字段改为 snake_case

- [ ] In `runtime_event_to_value`, replace:

```rust
result.entry("runDepth").or_insert(serde_json::Value::from(0));
result.entry("childRunId").or_insert(serde_json::Value::Null);
```

with:

```rust
result.entry("run_depth".into()).or_insert(serde_json::Value::from(0));
result.entry("child_run_id".into()).or_insert(serde_json::Value::Null);
```

- [ ] Update or add a Rust unit test for `runtime_event_to_value(RuntimeEvent::RunAborted { reason: None })` asserting:

```rust
value["type"] == "run_aborted"
value["run_depth"] == 0
value["child_run_id"].is_null()
value.get("runDepth").is_none()
value.get("childRunId").is_none()
```

### 2. TypeScript RuntimeEvent union 补两个事件

- [ ] Add to `js/index.d.ts`:

```ts
| { type: "run_aborted"; reason: string | null; run_depth: number; child_run_id: string | null }
| { type: "events_dropped"; subscriber_id: number; count: number; run_depth: number; child_run_id: string | null }
```

- [ ] Mirror the same additions in `js/index.ts` if that file is maintained as source docs/types for the package.
- [ ] Ensure no `runDepth` / `childRunId` appears in JS sources:

```bash
rg "runDepth|childRunId" crates/agent-runtime-node/src js/index.d.ts js/index.ts
```

Expected: no output.

### 3. Python stub 补两个 TypedDict

- [ ] Add before the `RuntimeEvent` type alias:

```python
class RunAbortedEvent(TypedDict):
    type: Literal["run_aborted"]
    reason: str | None
    run_depth: int
    child_run_id: str | None


class EventsDroppedEvent(TypedDict):
    type: Literal["events_dropped"]
    subscriber_id: int
    count: int
    run_depth: int
    child_run_id: str | None
```

- [ ] Add `RunAbortedEvent` and `EventsDroppedEvent` to the `RuntimeEvent` union.
- [ ] Keep Rust enum construction unchanged; binding metadata is injected by `runtime_event_to_dict`.

### 4. 验证脚本

```bash
./scripts/check-ts-event-wire-naming.sh
rg "runDepth|childRunId" crates/agent-runtime-node/src js/index.d.ts js/index.ts
cargo test -p agent-runtime-node
uvx maturin develop
.venv/bin/python -m pytest python/tests/test_types_and_tools.py -v
```

If Node crate build/test is not wired for `cargo test`, run `cargo build -p agent-runtime-node` and note the limitation.

