//! Shared FFI-independent helpers used by language bindings.

use std::time::Duration;

use serde_json::Value;

use crate::budget::BudgetConfig;
use crate::events::RuntimeEvent;
use crate::run::{ApprovalMode, CancelReason, Cancellation, RequestResponse};
use crate::tool::{Approval, ToolEffect, ToolEffects};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindingNameStyle {
    Python,
    Node,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BindingBudgetConfig {
    pub max_tokens: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_duration_secs: Option<u64>,
    pub max_cost_usd: Option<f64>,
}

pub fn parse_binding_approval(value: Option<&str>, default: Approval) -> Approval {
    match value {
        Some("never") => Approval::Never,
        Some("when_risky") | Some("whenRisky") => Approval::WhenRisky,
        Some("always") => Approval::Always,
        Some(_) => Approval::WhenRisky,
        None => default,
    }
}

/// Effects for a binding tool declared with the boolean `side_effect` flag:
/// unknown when set (so the default approval asks), none otherwise.
pub fn effects_from_side_effect_flag(side_effect: bool) -> ToolEffects {
    if side_effect {
        ToolEffects::UNKNOWN
    } else {
        ToolEffects::NONE
    }
}

/// Effects for a binding tool: the declared `effects` names (snake_case, as
/// in [`ToolEffect`]) when given, otherwise the `side_effect` flag.
pub fn parse_binding_effects(
    effects: Option<&[String]>,
    side_effect: bool,
) -> Result<ToolEffects, String> {
    let Some(names) = effects else {
        return Ok(effects_from_side_effect_flag(side_effect));
    };
    names
        .iter()
        .map(|name| {
            serde_json::from_value::<ToolEffect>(Value::String(name.clone())).map_err(|_| {
                format!(
                    "invalid effect '{name}'; expected read|write_workspace|write_outside|network|credential|destructive|process"
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(ToolEffects::of)
}

/// Parses a binding's answer to a request: the serde shape of
/// [`RequestResponse`] (`{"decision": {"type": "approve"}, "scope": "session"}`),
/// with `decision` also accepted as a bare string (`"approve"`, `"deny"`).
pub fn parse_binding_response(mut payload: Value) -> Result<RequestResponse, String> {
    if let Some(decision) = payload.get_mut("decision") {
        if let Value::String(kind) = decision {
            *decision = serde_json::json!({ "type": kind });
        }
    }
    serde_json::from_value(payload).map_err(|e| format!("invalid request response: {e}"))
}

/// Builds a cancellation from a binding's optional reason name (snake_case,
/// default `user`) and message.
pub fn parse_binding_cancellation(
    reason: Option<&str>,
    message: Option<String>,
) -> Result<Cancellation, String> {
    let reason = match reason {
        None => CancelReason::User,
        Some(name) => serde_json::from_value(Value::String(name.to_string())).map_err(|_| {
            format!(
                "invalid cancel reason '{name}'; expected user|timeout|upstream|host_disconnected|watcher"
            )
        })?,
    };
    let cancellation = Cancellation::new(reason);
    Ok(match message {
        Some(message) => cancellation.with_message(message),
        None => cancellation,
    })
}

pub fn parse_binding_approval_mode(
    value: Option<&str>,
    style: BindingNameStyle,
) -> Result<ApprovalMode, String> {
    match (style, value) {
        (_, None) => Ok(ApprovalMode::PerTool),
        (BindingNameStyle::Python, Some("per_tool" | "PerTool")) => Ok(ApprovalMode::PerTool),
        (BindingNameStyle::Node, Some("perTool" | "PerTool")) => Ok(ApprovalMode::PerTool),
        (_, Some("none" | "None")) => Ok(ApprovalMode::None),
        (_, Some("all" | "All")) => Ok(ApprovalMode::All),
        (BindingNameStyle::Python, Some(other)) => Err(format!(
            "invalid approval_mode '{other}'; expected per_tool|none|all"
        )),
        (BindingNameStyle::Node, Some(other)) => Err(format!(
            "invalid approvalMode '{other}'; expected perTool|none|all"
        )),
    }
}

pub fn budget_config_from_binding(input: Option<BindingBudgetConfig>) -> BudgetConfig {
    input.map_or_else(BudgetConfig::default, |b| BudgetConfig {
        max_tokens: b.max_tokens,
        max_tool_calls: b.max_tool_calls,
        max_duration: b.max_duration_secs.map(Duration::from_secs),
        max_cost_usd: b.max_cost_usd,
    })
}

/// Parses binding-supplied conversation history for
/// [`crate::run::AgentRun::start_with_messages`]. Each value is the serde
/// JSON shape of [`crate::model::Message`] (e.g.
/// `{"role": "user", "content": [{"Text": "..."}]}`) — the same shape
/// session snapshots serialize to.
pub fn messages_from_wire_values(
    values: Vec<Value>,
) -> Result<Vec<crate::model::Message>, serde_json::Error> {
    values.into_iter().map(serde_json::from_value).collect()
}

pub fn runtime_event_to_wire_value(event: &RuntimeEvent) -> Result<Value, serde_json::Error> {
    serde_json::to_value(event).map(runtime_event_value_to_wire_value)
}

pub fn runtime_event_value_to_wire_value(value: Value) -> Value {
    match value {
        Value::Object(outer) if outer.len() == 1 => {
            let Some((variant, fields)) = outer.into_iter().next() else {
                return Value::Object(serde_json::Map::new());
            };
            let mut result = match fields {
                Value::Object(fields) => fields,
                other => {
                    let mut fields = serde_json::Map::new();
                    fields.insert("value".into(), other);
                    fields
                }
            };
            result.insert("type".into(), Value::String(to_snake_case(&variant)));
            result.entry("run_depth").or_insert(Value::from(0));
            result.entry("child_run_id").or_insert(Value::Null);
            Value::Object(result)
        }
        other => other,
    }
}

pub fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        budget_config_from_binding, messages_from_wire_values, parse_binding_approval,
        parse_binding_approval_mode, parse_binding_cancellation, parse_binding_effects,
        parse_binding_response, runtime_event_value_to_wire_value, to_snake_case,
        BindingBudgetConfig, BindingNameStyle,
    };
    use crate::run::{ApprovalMode, ApprovalScope, CancelReason, RequestDecision};
    use crate::tool::{Approval, ToolEffect, ToolEffects};
    use serde_json::{json, Value};
    use std::time::Duration;

    #[test]
    fn effects_come_from_names_or_the_flag() {
        assert_eq!(parse_binding_effects(None, true), Ok(ToolEffects::UNKNOWN));
        assert_eq!(parse_binding_effects(None, false), Ok(ToolEffects::NONE));
        let names = vec!["read".to_string(), "network".to_string()];
        assert_eq!(
            parse_binding_effects(Some(&names), true),
            Ok(ToolEffects::of([ToolEffect::Read, ToolEffect::Network]))
        );
        assert!(parse_binding_effects(Some(&["fly".to_string()]), false).is_err());
    }

    #[test]
    fn responses_accept_the_serde_shape_and_bare_decisions() {
        let approve =
            parse_binding_response(json!({"decision": "approve", "scope": "session"})).unwrap();
        assert_eq!(approve.decision, RequestDecision::Approve);
        assert_eq!(approve.scope, ApprovalScope::Session);
        let answer = parse_binding_response(
            json!({"decision": {"type": "answer", "payload": {"choice": 2}}}),
        )
        .unwrap();
        assert_eq!(
            answer.decision,
            RequestDecision::Answer {
                payload: json!({"choice": 2})
            }
        );
        assert!(parse_binding_response(json!({"decision": "maybe"})).is_err());
    }

    #[test]
    fn cancellations_default_to_user() {
        assert_eq!(
            parse_binding_cancellation(None, None).unwrap().reason,
            CancelReason::User
        );
        let timeout = parse_binding_cancellation(Some("timeout"), Some("late".into())).unwrap();
        assert_eq!(timeout.reason, CancelReason::Timeout);
        assert_eq!(timeout.message.as_deref(), Some("late"));
        assert!(parse_binding_cancellation(Some("bored"), None).is_err());
    }

    #[test]
    fn approval_parser_covers_python_and_node_spellings() {
        assert_eq!(
            parse_binding_approval(Some("when_risky"), Approval::Never),
            Approval::WhenRisky
        );
        assert_eq!(
            parse_binding_approval(Some("whenRisky"), Approval::Never),
            Approval::WhenRisky
        );
        assert_eq!(
            parse_binding_approval(None, Approval::Always),
            Approval::Always
        );
    }

    #[test]
    fn approval_mode_parser_preserves_language_specific_spellings() {
        assert_eq!(
            parse_binding_approval_mode(Some("per_tool"), BindingNameStyle::Python),
            Ok(ApprovalMode::PerTool)
        );
        assert_eq!(
            parse_binding_approval_mode(Some("perTool"), BindingNameStyle::Node),
            Ok(ApprovalMode::PerTool)
        );
        assert!(parse_binding_approval_mode(Some("perTool"), BindingNameStyle::Python).is_err());
        assert!(parse_binding_approval_mode(Some("per_tool"), BindingNameStyle::Node).is_err());
    }

    #[test]
    fn budget_config_maps_duration_seconds() {
        let config = budget_config_from_binding(Some(BindingBudgetConfig {
            max_tokens: Some(100),
            max_tool_calls: Some(3),
            max_duration_secs: Some(7),
            max_cost_usd: Some(0.5),
        }));

        assert_eq!(config.max_tokens, Some(100));
        assert_eq!(config.max_tool_calls, Some(3));
        assert_eq!(config.max_duration, Some(Duration::from_secs(7)));
        assert_eq!(config.max_cost_usd, Some(0.5));
    }

    #[test]
    fn event_value_conversion_flattens_tagged_runtime_event() {
        let converted = runtime_event_value_to_wire_value(json!({
            "RunStarted": {
                "run_id": "r1",
            }
        }));

        assert_eq!(converted["type"], "run_started");
        assert_eq!(converted["run_id"], "r1");
        assert_eq!(converted["run_depth"], 0);
        assert_eq!(converted["child_run_id"], Value::Null);
    }

    #[test]
    fn wire_messages_parse_in_snapshot_shape() {
        let values = vec![
            json!({"role": "user", "content": [{"Text": "hi"}]}),
            json!({"role": "assistant", "content": [{"ToolUse": {"id": "t1", "name": "echo", "input": {}}}]}),
        ];
        let messages = messages_from_wire_values(values).expect("valid wire messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, crate::model::Role::User);
        assert_eq!(messages[1].role, crate::model::Role::Assistant);

        assert!(messages_from_wire_values(vec![json!({"role": "nope", "content": []})]).is_err());
    }

    #[test]
    fn snake_case_conversion_handles_runtime_event_names() {
        assert_eq!(to_snake_case("RunStarted"), "run_started");
        assert_eq!(to_snake_case("ApprovalDenied"), "approval_denied");
        assert_eq!(to_snake_case("AsyncToolProgress"), "async_tool_progress");
        assert_eq!(to_snake_case("SkillLoadWarning"), "skill_load_warning");
    }

    #[test]
    fn run_completed_wire_value_carries_stop_reason() {
        // Py/Node bindings receive events through this wire shape; the
        // truncation marker must survive the conversion.
        let event = crate::events::RuntimeEvent::RunCompleted {
            output: json!("cut off"),
            stop_reason: crate::model::StopReason::MaxTokens,
        };

        let converted = super::runtime_event_to_wire_value(&event).expect("serialize event");

        assert_eq!(converted["type"], "run_completed");
        assert_eq!(converted["output"], json!("cut off"));
        assert_eq!(converted["stop_reason"], json!("MaxTokens"));
    }
}
