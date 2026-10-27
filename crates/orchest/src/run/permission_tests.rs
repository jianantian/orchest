//! Permission decision point tests (v2.0 issue 005).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

use super::checkpoint_tests::{tool_result_code, ScriptedModel, Turn};
use super::*;
use crate::events::RuntimeEvent;
use crate::tool::registry::ToolRegistry;
use crate::tool::{
    Approval, DecisionBasis, Enforcement, ExecutionBackend, JsonSchema, PermissionAction,
    PermissionDecision, PermissionPolicy, PermissionRule, Tool, ToolContext, ToolEffect,
    ToolEffects, ToolError, ToolMetadata, ToolOutput, PERMISSION_DENIED,
};

/// A tool that counts its runs.
struct CountingTool {
    name: &'static str,
    metadata: ToolMetadata,
    runs: AtomicU32,
}

impl CountingTool {
    fn new(name: &'static str, effects: ToolEffects, approval: Approval) -> Arc<Self> {
        Arc::new(Self {
            name,
            metadata: ToolMetadata {
                effects,
                approval,
                ..ToolMetadata::default()
            },
            runs: AtomicU32::new(0),
        })
    }

    fn runs(&self) -> u32 {
        self.runs.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Tool for CountingTool {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        "counting tool"
    }
    fn input_schema(&self) -> &JsonSchema {
        &Value::Null
    }
    fn output_schema(&self) -> Option<&JsonSchema> {
        None
    }
    fn metadata(&self) -> &ToolMetadata {
        &self.metadata
    }
    async fn execute(&self, _input: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(ToolOutput::Immediate(json!("ok")))
    }
}

fn config(policy: PermissionPolicy) -> AgentConfig {
    let mut config = AgentConfig::builder("permission-agent", "mock")
        .system_prompt("system")
        .max_steps(10)
        .permissions(policy)
        .build()
        .unwrap();
    config.supervision_strategy = SupervisionStrategy::Stop;
    config
}

fn call(id: &'static str, name: &'static str) -> Turn {
    Turn::CallTool { id, name }
}

/// Runs `turns` against `tool`, answering every approval with `respond`.
async fn run(
    config: AgentConfig,
    tool: &Arc<CountingTool>,
    turns: Vec<Turn>,
    respond: impl Fn() -> RequestResponse,
) -> (Vec<RuntimeEvent>, Arc<ScriptedModel>) {
    let model = ScriptedModel::new(turns);
    let mut registry = ToolRegistry::new();
    registry
        .register(Arc::clone(tool) as Arc<dyn Tool>)
        .unwrap();
    let (handle, mut rx) = AgentRun::start(config, "go".into(), model.clone(), registry);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        if let RuntimeEvent::ApprovalRequested { request_id, .. } = &event {
            handle.respond(request_id, respond()).await.unwrap();
        }
        events.push(event);
    }
    handle.wait().await;
    (events, model)
}

fn decisions(events: &[RuntimeEvent]) -> Vec<PermissionDecision> {
    events
        .iter()
        .filter_map(|e| match e {
            RuntimeEvent::PermissionDecided { decision, .. } => Some(decision.clone()),
            _ => None,
        })
        .collect()
}

fn approvals(events: &[RuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, RuntimeEvent::ApprovalRequested { .. }))
        .count()
}

#[tokio::test]
async fn deny_rule_refuses_without_asking() {
    let tool = CountingTool::new(
        "shell",
        ToolEffects::of([ToolEffect::Process]),
        Approval::Never,
    );
    let policy = PermissionPolicy::new().rule(PermissionRule::deny("shell").reason("no shell"));
    let (events, model) = run(config(policy), &tool, vec![call("c1", "shell")], || {
        RequestResponse::approve()
    })
    .await;

    assert_eq!(tool.runs(), 0);
    assert_eq!(approvals(&events), 0);
    assert_eq!(
        decisions(&events),
        vec![PermissionDecision::Deny {
            basis: DecisionBasis::Rule { index: 0 },
            reason: "no shell".into(),
        }]
    );
    let last = model.seen().pop().unwrap();
    assert_eq!(
        tool_result_code(&last, "c1").as_deref(),
        Some(PERMISSION_DENIED)
    );
}

#[tokio::test]
async fn allow_rule_overrides_the_tool_declaration() {
    let tool = CountingTool::new("write", ToolEffects::UNKNOWN, Approval::Always);
    let policy = PermissionPolicy::new().rule(PermissionRule::allow("write").path("a.txt"));
    let (events, _) = run(config(policy), &tool, vec![call("c1", "write")], || {
        RequestResponse::deny(None)
    })
    .await;

    assert_eq!(tool.runs(), 1);
    assert_eq!(approvals(&events), 0);
    assert!(decisions(&events)[0].is_allow());
}

#[tokio::test]
async fn ask_rule_and_unknown_effects_fail_closed() {
    // Unknown effects under the default approval ask; a denial stops the call.
    let tool = CountingTool::new("mystery", ToolEffects::UNKNOWN, Approval::default());
    let (events, _) = run(
        config(PermissionPolicy::new()),
        &tool,
        vec![call("c1", "mystery")],
        || RequestResponse::deny(None),
    )
    .await;
    assert_eq!(tool.runs(), 0);
    assert_eq!(
        decisions(&events),
        vec![PermissionDecision::Ask {
            basis: DecisionBasis::ToolDeclaration
        }]
    );

    // `unmatched: ask` makes even a declared read-only tool ask.
    let tool = CountingTool::new("reader", ToolEffects::READ_ONLY, Approval::Never);
    let policy = PermissionPolicy::new().unmatched(PermissionAction::Ask);
    let (events, _) = run(config(policy), &tool, vec![call("c1", "reader")], || {
        RequestResponse::approve()
    })
    .await;
    assert_eq!(tool.runs(), 1);
    assert_eq!(approvals(&events), 1);
}

#[tokio::test]
async fn session_grant_is_remembered_for_the_same_call() {
    let tool = CountingTool::new("write", ToolEffects::UNKNOWN, Approval::Always);
    let turns = vec![call("c1", "write"), call("c2", "write")];
    let (events, _) = run(config(PermissionPolicy::new()), &tool, turns, || {
        RequestResponse::approve().for_session()
    })
    .await;

    assert_eq!(tool.runs(), 2);
    assert_eq!(approvals(&events), 1);
    assert_eq!(
        decisions(&events),
        vec![
            PermissionDecision::Ask {
                basis: DecisionBasis::ToolDeclaration
            },
            PermissionDecision::Allow {
                basis: DecisionBasis::SessionGrant
            },
        ]
    );
}

#[tokio::test]
async fn one_time_approval_is_not_remembered() {
    let tool = CountingTool::new("write", ToolEffects::UNKNOWN, Approval::Always);
    let turns = vec![call("c1", "write"), call("c2", "write")];
    let (events, _) = run(config(PermissionPolicy::new()), &tool, turns, || {
        RequestResponse::approve()
    })
    .await;
    assert_eq!(tool.runs(), 2);
    assert_eq!(approvals(&events), 2);
}

#[tokio::test]
async fn session_grant_never_overrides_a_deny() {
    let tool = CountingTool::new("write", ToolEffects::UNKNOWN, Approval::Always);
    let config = config(PermissionPolicy::new().rule(PermissionRule::deny("write")));
    let input = json!({"path": "a.txt"});
    let mut grants = std::collections::BTreeSet::new();
    grants.insert(crate::tool::grant_key("write", &input));
    let call = crate::tool::PermissionCall {
        tool_name: "write",
        input: &input,
        effects: &tool.metadata.effects,
        subject: crate::tool::PermissionSubject::from_input(&input),
    };
    assert!(matches!(
        config.runtime.decide(&call, &tool.metadata, &grants),
        PermissionDecision::Deny { .. }
    ));
}

struct Container(Enforcement);

impl ExecutionBackend for Container {
    fn name(&self) -> &str {
        "container"
    }
    fn enforcement(&self) -> Enforcement {
        self.0
    }
}

#[test]
fn enforcement_facts_follow_the_injected_backend() {
    let soft = config(PermissionPolicy::new()).runtime.enforcement();
    assert_eq!(soft.backend, None);
    assert_eq!(soft.isolation, Enforcement::None);
    assert_eq!(soft.permission_gate, Enforcement::Soft);
    assert_eq!(soft.read_only, Enforcement::Soft);
    assert_eq!(soft.code_execution, None);

    let hard = AgentConfig::builder("a", "mock")
        .execution_backend(Arc::new(Container(Enforcement::Hard)))
        .build()
        .unwrap()
        .runtime
        .enforcement();
    assert_eq!(hard.backend.as_deref(), Some("container"));
    assert_eq!(hard.isolation, Enforcement::Hard);
    assert_eq!(hard.read_only, Enforcement::Hard);

    let open = AgentConfig::builder("a", "mock")
        .approval_mode(ApprovalMode::None)
        .build()
        .unwrap()
        .runtime
        .enforcement();
    assert_eq!(open.permission_gate, Enforcement::None);
    assert_eq!(open.read_only, Enforcement::None);
}
