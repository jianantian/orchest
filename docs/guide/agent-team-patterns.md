# Agent Team Patterns

Orchest does not need a built-in peer-to-peer agent protocol for the current
team patterns. Keep team coordination in the application layer and compose the
runtime primitives that already preserve event visibility, budget accounting,
approval routing, and message history rules.

## Pattern 1: Parent Agent With Specialist Tools

Use Agent-as-Tool when one coordinator should remain in control and call
specialists as ordinary tools. This is the right shape for summarizers,
researchers, reviewers, planners, and validators whose work has a bounded input
and output.

Template:

```rust
let specialist_tool = specialist_config
    .as_tool("summariser", "Summarises long text")
    .model(Arc::clone(&specialist_model))
    .registry(specialist_registry)
    .context_mode(ContextMode::Fresh)
    .build();

parent_registry.register(Arc::new(specialist_tool))?;
```

Use `ContextMode::Fresh` when the specialist should receive only mapped input.
Use `ContextMode::Fork` when the specialist must inspect parent history. Do not
use removed `inherit_context(...)` helpers.

Runnable example:

```bash
cargo run -p orchest --example agent_as_tool
```

## Pattern 2: Triage Handoff

Use Handoff when ownership should transfer from one agent to another. This is
the right shape for routing from an intake agent to billing, support, legal,
incident response, or another durable role.

Template:

```rust
let triage_config = AgentConfig::builder("triage", "mock/routing")
    .system_prompt("you are a triage agent")
    .build()?
    .with_handoff(Handoff {
        tool_name: "route_to_billing".into(),
        tool_description: "Transfer the user to billing".into(),
        input_schema: json!({"type": "object"}),
        target: HandoffTarget::Static(Box::new(billing_config)),
        input_filter: None,
        nest_history: false,
    });
```

Add a `HandoffInputFilter` when the receiving agent should get a reduced or
rewritten history instead of the full conversation.

Runnable examples:

```bash
cargo run -p orchest --example handoff_routing
cargo run -p orchest --example handoff_input_filter
```

## Pattern 3: Supervisor Watcher

Use watchers when an external supervisor should observe and steer a running
agent without becoming another model turn or tool call. Watchers are suitable
for operational controls such as failure thresholds, steering after a long task,
or cancellation from a product UI.

Template:

```rust
struct SteeringWatcher;

#[async_trait]
impl Watcher for SteeringWatcher {
    async fn on_event(&self, event: &RuntimeEvent) -> WatcherAction {
        match event {
            RuntimeEvent::ToolCallCompleted { tool, .. } if tool == "long_task" => {
                WatcherAction::Steer("Focus on summarizing results.".into())
            }
            _ => WatcherAction::Continue,
        }
    }
}

// Prefer start_with_watchers when first-event observation is required:
let (handle, rx) = AgentRun::start_with_watchers(
    config,
    input,
    model,
    registry,
    vec![(Arc::new(SteeringWatcher) as Arc<dyn Watcher>, 256)],
)?;

// Post-start attach_watcher remains supported but is best-effort:
// handle.attach_watcher(Arc::new(SteeringWatcher), 256).await;
```

When a supervisor delegates through Agent-as-Tool, forwarded child lifecycle
and runtime events (`SubAgentStarted` / `SubAgentEvent` / `SubAgentCompleted` /
`SubAgentFailed`) are delivered to attached watcher subscriptions through the
same fan-out contract as other run events (`ToolContext::emit_event`). The
primary `EventReceiver` still receives each event once, in order; watchers
observe the same nested stream without duplicate primary delivery.
`LlmWatcher` formats those nested events structurally (not via `Debug`).

When multiple watchers return actions for the same fan-out event, the runtime
gates until every watcher in that delivery cohort finishes `on_event`, then
resolves with a public deterministic rule (`Abort` wins; otherwise `Inject` /
`Steer` apply in registration order). Per-watcher delivery FIFO is a separate
property. See [`arbitrate_watcher_actions`](../../crates/orchest/src/run/action_arbitration.rs).

Runnable examples:

```bash
cargo run -p orchest --example watcher_inject_message
cargo run -p orchest --example watcher_abort_on_pattern
cargo run -p orchest --example supervised_delegation
```

## When Existing Primitives Are Sufficient

Use existing primitives when the coordination relationship has one of these
shapes:

- bounded delegation: parent calls specialist and receives a result;
- ownership transfer: one agent hands the session to another agent;
- external supervision: watcher observes events and injects, steers, or aborts.

Do not add direct peer-to-peer runtime messaging unless an example needs
simultaneous autonomous agents exchanging messages outside those shapes. The
current examples do not prove that gap.
