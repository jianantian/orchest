# 003 · TypeScript / Python Type Stub 对齐

## 背景

`js/index.d.ts` 和 `python/agent_runtime/__init__.pyi` 的 `RuntimeEvent` union 缺少 Rust runtime 实际发出的两个事件变体。消费者 exhaustive switch 会遗漏这些事件。

## 缺失的事件

### `run_aborted`

Rust 定义：`RuntimeEvent::RunAborted`（`actor.rs:358-362`）

在 `SteeringCmd::Abort` 触发时发出，表示 run 被外部主动中止。

### `events_dropped`

Rust 定义：`RuntimeEvent::EventsDropped`（`actor.rs:1189`）

在 secondary event subscriber channel 满时发出，通知消费者有事件丢失。

## 修复

### Rust 实际定义（已验证）

```rust
RuntimeEvent::RunAborted { reason: Option<String> }
RuntimeEvent::EventsDropped { subscriber_id: u64, count: u64 }
```

注意：Rust enum 变体本身没有 `run_depth` 或 `child_run_id` 字段。不要在 Rust 事件构造处添加这些字段。

语言绑定当前会在 Rust enum 序列化后补默认 metadata：

- Python `runtime_event_to_dict` 补 `run_depth` 和 `child_run_id`
- Node `runtime_event_to_value` 补 run depth 和 child run id

因此 type stub 应匹配语言绑定公开给用户的 wire shape，而不是只匹配裸 Rust enum。Node binding 还必须把补充字段命名为 `run_depth` / `child_run_id`，与 `js/index.d.ts` 保持一致；当前 `runDepth` / `childRunId` 是既有错配，需在本 issue 中修复。

### TypeScript（`js/index.d.ts`）

在 `RuntimeEvent` union 中添加：

```typescript
| { type: "run_aborted"; reason: string | null; run_depth: number; child_run_id: string | null }
| { type: "events_dropped"; subscriber_id: number; count: number; run_depth: number; child_run_id: string | null }
```

### Python（`python/agent_runtime/__init__.pyi`）

添加 TypedDict 定义并加入 union：

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

将两者加入 `RuntimeEvent` union。

## 验收标准

- [ ] `js/index.d.ts` 的 `RuntimeEvent` 包含 `run_aborted` 和 `events_dropped`
- [ ] `python/agent_runtime/__init__.pyi` 的 `RuntimeEvent` 包含 `RunAbortedEvent` 和 `EventsDroppedEvent`
- [ ] Rust `RuntimeEvent::RunAborted` / `EventsDropped` 构造处不新增 `run_depth` 或 `child_run_id` 字段
- [ ] 语言绑定 type stubs 包含 binding-injected `run_depth` 和 `child_run_id` metadata
- [ ] Node binding 补充字段使用 `run_depth` / `child_run_id`，不使用 `runDepth` / `childRunId`
- [ ] `rg "runDepth|childRunId" crates/agent-runtime-node/src js/index.d.ts js/index.ts` 无结果
- [ ] `./scripts/check-ts-event-wire-naming.sh` 通过
