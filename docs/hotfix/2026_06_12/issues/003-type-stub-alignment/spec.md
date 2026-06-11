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

注意：这两个变体没有 `run_depth` 字段（不同于其他事件）。

### TypeScript（`js/index.d.ts`）

在 `RuntimeEvent` union 中添加：

```typescript
| { type: "run_aborted"; reason: string | null }
| { type: "events_dropped"; subscriber_id: number; count: number }
```

### Python（`python/agent_runtime/__init__.pyi`）

添加 TypedDict 定义并加入 union：

```python
class RunAbortedEvent(TypedDict):
    type: Literal["run_aborted"]
    reason: Optional[str]

class EventsDroppedEvent(TypedDict):
    type: Literal["events_dropped"]
    subscriber_id: int
    count: int
```

将两者加入 `RuntimeEvent` union。

## 验收标准

- [ ] `js/index.d.ts` 的 `RuntimeEvent` 包含 `run_aborted` 和 `events_dropped`
- [ ] `python/agent_runtime/__init__.pyi` 的 `RuntimeEvent` 包含 `RunAbortedEvent` 和 `EventsDroppedEvent`
- [ ] 字段名和类型与 Rust `RuntimeEvent` enum 的实际定义一致（无多余的 `run_depth`）
- [ ] `./scripts/check-ts-event-wire-naming.sh` 通过
