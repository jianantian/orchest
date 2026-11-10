//! Execution backend injection point and the enforcement facts a host can
//! show its users.
//!
//! Orchest runs tools in-process. Its permission decision
//! ([`policy`](super::policy)) and the effects tools declare are checks the
//! process makes on itself: they hold only as long as nothing else in the
//! process works around them, which is a soft guarantee. A hard guarantee
//! (for example "this run cannot write outside the workspace") needs
//! isolation the host provides — an OS sandbox, a container, a remote
//! executor — and declares by injecting an [`ExecutionBackend`] or a
//! [`ScriptExecutor`](crate::skill::executor::ScriptExecutor) that reports
//! [`Enforcement::Hard`].
//!
//! Skill capability declarations are hints for the model and the host, not
//! an execution boundary, unless a backend the host injected enforces them.

use serde::{Deserialize, Serialize};

/// How strongly a boundary is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Enforcement {
    /// Nothing enforces it.
    None,
    /// The process checks itself (classification, permission rules).
    Soft,
    /// Isolation outside the process's control enforces it.
    Hard,
}

/// An isolated environment the host runs this process's tools in. Orchest
/// does not verify the claim; it reports it as the host made it.
pub trait ExecutionBackend: Send + Sync {
    /// A short name shown next to the enforcement facts.
    fn name(&self) -> &str;
    /// How strongly the backend confines what tools can do.
    fn enforcement(&self) -> Enforcement;
}

/// What a host can truthfully say about a run's boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EnforcementFacts {
    /// The injected execution backend, if any.
    pub backend: Option<String>,
    /// Isolation from the injected execution backend.
    pub isolation: Enforcement,
    /// The permission decision point: `Soft` when it can ask or deny,
    /// `None` when every call is allowed.
    pub permission_gate: Enforcement,
    /// Isolation of code execution tools, when code execution is enabled.
    pub code_execution: Option<Enforcement>,
    /// How far a read-only setting can be relied on: `Hard` only with a hard
    /// backend, `Soft` with the permission gate alone. Without a backend it
    /// must not be presented as a hard guarantee.
    pub read_only: Enforcement,
}

impl EnforcementFacts {
    pub(crate) fn new(
        backend: Option<&dyn ExecutionBackend>,
        permission_gate: Enforcement,
        code_execution: Option<Enforcement>,
    ) -> Self {
        let isolation = backend.map_or(Enforcement::None, |b| b.enforcement());
        Self {
            backend: backend.map(|b| b.name().to_string()),
            isolation,
            permission_gate,
            code_execution,
            read_only: isolation.max(permission_gate),
        }
    }
}
