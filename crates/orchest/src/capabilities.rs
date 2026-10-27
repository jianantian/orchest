//! Capability probe: what this runtime, under a given configuration, supports.
//!
//! A host branches on [`Capabilities`] instead of guessing from versions or
//! catching errors. The values are runtime facts derived from the config
//! (for example the isolation level comes from the injected execution
//! backend), not compile-time constants. [`Capabilities::require`] turns an
//! unsupported capability into an explicit [`NotSupported`] error.

use serde::{Deserialize, Serialize};

use crate::run::config::ToolExecutionPolicy;
use crate::run::{AgentConfig, ApprovalScope, RequestKind};
use crate::tool::{Enforcement, EnforcementFacts, ToolEffect};

/// One thing a host may depend on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Capability {
    /// Continuing a run from its session log (`AgentRun::resume`).
    Resume,
    /// Cancellation that takes effect inside a step, not only between steps.
    PreemptiveCancel,
    /// An approval scope a host can answer with.
    ApprovalScope { scope: ApprovalScope },
    /// Blocking requests of this kind.
    RequestKind { kind: String },
    /// Running same-turn tool calls concurrently.
    ParallelTools,
    /// Delegating to child runs (`AgentAsTool`).
    Delegation,
    /// Classifying tools by this effect.
    Effect { effect: ToolEffect },
    /// Isolation of at least this level.
    Isolation { level: Enforcement },
    /// Every event reaches every subscriber (no lossy delivery).
    LosslessEvents,
}

/// Returned by [`Capabilities::require`] for a capability that is not
/// available.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not supported: {capability:?}: {reason}")]
#[non_exhaustive]
pub struct NotSupported {
    pub capability: Capability,
    pub reason: String,
}

/// What a run supports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Capabilities {
    /// A session store and session id are configured, so the run can be
    /// resumed from its log.
    pub resume: bool,
    pub preemptive_cancel: bool,
    pub approval_scopes: Vec<ApprovalScope>,
    /// Request kinds the runtime raises itself.
    pub builtin_request_kinds: Vec<String>,
    /// Tools may raise requests of any host-defined kind
    /// (`ToolContext::request`).
    pub host_request_kinds: bool,
    /// The run's tool execution policy allows parallel batches.
    pub parallel_tools: bool,
    pub delegation: bool,
    /// The effect classification tools declare against.
    pub effects: Vec<ToolEffect>,
    /// What the run's boundaries can be relied on for.
    pub enforcement: EnforcementFacts,
    pub lossless_events: bool,
}

impl Capabilities {
    /// The capabilities of a run started with `config`.
    pub fn of(config: &AgentConfig) -> Self {
        Self {
            resume: config.session_store.is_some() && config.session_id.is_some(),
            preemptive_cancel: true,
            approval_scopes: vec![ApprovalScope::Once, ApprovalScope::Session],
            builtin_request_kinds: vec![
                RequestKind::TOOL_APPROVAL.to_string(),
                RequestKind::RETRY_APPROVAL.to_string(),
            ],
            host_request_kinds: true,
            parallel_tools: config.runtime.tool_execution_policy
                == ToolExecutionPolicy::ParallelSafe,
            delegation: true,
            effects: ToolEffect::ALL.to_vec(),
            enforcement: config.runtime.enforcement(),
            lossless_events: false,
        }
    }

    pub fn supports(&self, capability: &Capability) -> bool {
        self.unsupported_reason(capability).is_none()
    }

    /// `Ok` when `capability` is available, otherwise why not.
    pub fn require(&self, capability: Capability) -> Result<(), NotSupported> {
        match self.unsupported_reason(&capability) {
            None => Ok(()),
            Some(reason) => Err(NotSupported {
                capability,
                reason: reason.into(),
            }),
        }
    }

    fn unsupported_reason(&self, capability: &Capability) -> Option<&'static str> {
        match capability {
            Capability::Resume => {
                (!self.resume).then_some("no session store and session id are configured")
            }
            Capability::PreemptiveCancel => (!self.preemptive_cancel).then_some("not available"),
            Capability::ApprovalScope { scope } => (!self.approval_scopes.contains(scope))
                .then_some("this approval scope is not available"),
            Capability::RequestKind { kind } => (!self.host_request_kinds
                && !self.builtin_request_kinds.contains(kind))
            .then_some("the runtime does not raise this kind"),
            Capability::ParallelTools => {
                (!self.parallel_tools).then_some("the tool execution policy is sequential")
            }
            Capability::Delegation => (!self.delegation).then_some("not available"),
            Capability::Effect { effect } => (!self.effects.contains(effect))
                .then_some("this effect is not part of the classification"),
            Capability::Isolation { level } => (self.enforcement.isolation < *level)
                .then_some("no execution backend provides this isolation"),
            Capability::LosslessEvents => {
                (!self.lossless_events).then_some("secondary subscribers may drop events")
            }
        }
    }
}

impl AgentConfig {
    /// What a run started with this config supports.
    pub fn capabilities(&self) -> Capabilities {
        Capabilities::of(self)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::session::InMemorySessionStore;
    use crate::tool::ExecutionBackend;

    struct Sandbox;

    impl ExecutionBackend for Sandbox {
        fn name(&self) -> &str {
            "sandbox"
        }
        fn enforcement(&self) -> Enforcement {
            Enforcement::Hard
        }
    }

    fn config() -> AgentConfig {
        AgentConfig::builder("a", "mock").build().unwrap()
    }

    #[test]
    fn capabilities_follow_the_config() {
        let plain = config().capabilities();
        assert!(!plain.supports(&Capability::Resume));
        assert!(!plain.supports(&Capability::ParallelTools));
        assert!(plain.supports(&Capability::PreemptiveCancel));
        assert!(plain.supports(&Capability::ApprovalScope {
            scope: ApprovalScope::Session
        }));
        assert!(plain.supports(&Capability::RequestKind {
            kind: "plan_review".into()
        }));
        assert!(plain.supports(&Capability::Effect {
            effect: ToolEffect::Credential
        }));
        assert!(plain.supports(&Capability::Isolation {
            level: Enforcement::None
        }));

        let configured = AgentConfig::builder("a", "mock")
            .enable_parallel_tools()
            .execution_backend(Arc::new(Sandbox))
            .build()
            .unwrap()
            .with_session_store(Arc::new(InMemorySessionStore::new()), "s");
        let caps = configured.capabilities();
        assert!(caps.supports(&Capability::Resume));
        assert!(caps.supports(&Capability::ParallelTools));
        assert!(caps.supports(&Capability::Isolation {
            level: Enforcement::Hard
        }));
        assert_eq!(caps.enforcement.read_only, Enforcement::Hard);
    }

    #[test]
    fn unsupported_capabilities_are_explicit_errors() {
        let caps = config().capabilities();
        let err = caps
            .require(Capability::Isolation {
                level: Enforcement::Hard,
            })
            .unwrap_err();
        assert_eq!(
            err.capability,
            Capability::Isolation {
                level: Enforcement::Hard
            }
        );
        assert!(caps.require(Capability::LosslessEvents).is_err());
        assert!(caps.require(Capability::Resume).is_err());
        assert!(caps.require(Capability::Delegation).is_ok());
    }
}
