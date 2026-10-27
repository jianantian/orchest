//! Blocking request channel: a run stops and waits for the host, under a
//! request id the host can answer, withdraw or look up.
//!
//! Orchest does not interpret what a request is for. A request carries a
//! host-defined [`RequestKind`] and a JSON payload; tool approval is one kind
//! among them ([`RequestKind::TOOL_APPROVAL`]). One [`RequestBus`] is shared
//! by a whole run tree, so a host holding the root [`RunHandle`] sees and
//! answers requests from delegated child runs too.
//!
//! [`RunHandle`]: crate::run::RunHandle

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{oneshot, Mutex};

use super::config::RunId;

/// Identifies one request. Built from the run id and a per-run sequence
/// number, so it is unique within the run tree and stays the same for a
/// host that reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(String);

impl RequestId {
    pub(crate) fn new(run_id: RunId, seq: u64) -> Self {
        Self(format!("{run_id}/{seq}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for RequestId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for RequestId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// What a request asks for. The vocabulary belongs to the host; Orchest only
/// defines the kinds it raises itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestKind(String);

impl RequestKind {
    /// Approve a tool call before it runs.
    pub const TOOL_APPROVAL: &'static str = "tool_approval";
    /// Approve retrying a tool call whose failure may have had effects.
    pub const RETRY_APPROVAL: &'static str = "retry_approval";

    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RequestKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a pending request is holding up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RequestTarget {
    /// A tool call that runs only once the request is answered.
    ToolCall {
        tool_use_id: String,
        tool_name: String,
    },
    /// A step of the run.
    Step { step: u32 },
    /// The run as a whole.
    Run,
}

/// A request waiting for the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingRequest {
    pub id: RequestId,
    /// The run that raised the request (a child run for delegated work).
    pub run_id: RunId,
    pub kind: RequestKind,
    pub payload: Value,
    pub target: RequestTarget,
}

/// The host's decision on a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RequestDecision {
    Approve,
    Deny {
        reason: Option<String>,
    },
    /// An answer with content, for kinds that ask a question.
    Answer {
        payload: Value,
    },
}

/// How long an approval holds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApprovalScope {
    /// This call only.
    #[default]
    Once,
    /// This call and every later call of the same tool with the same input
    /// in the session (see [`grant_key`](crate::tool::grant_key)).
    Session,
}

/// The host's reply to a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestResponse {
    pub decision: RequestDecision,
    /// How long an approving decision holds. Only explicit session grants
    /// are remembered.
    #[serde(default)]
    pub scope: ApprovalScope,
    /// Rules the host wants applied for the rest of the session (for
    /// example "always allow this command"). Carried to whoever raised the
    /// request; the runtime does not interpret them here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_rules: Option<Value>,
}

impl RequestResponse {
    pub fn approve() -> Self {
        Self {
            decision: RequestDecision::Approve,
            scope: ApprovalScope::Once,
            session_rules: None,
        }
    }

    pub fn deny(reason: Option<String>) -> Self {
        Self {
            decision: RequestDecision::Deny { reason },
            scope: ApprovalScope::Once,
            session_rules: None,
        }
    }

    pub fn answer(payload: Value) -> Self {
        Self {
            decision: RequestDecision::Answer { payload },
            scope: ApprovalScope::Once,
            session_rules: None,
        }
    }

    /// Makes an approval hold for the rest of the session.
    pub fn for_session(mut self) -> Self {
        self.scope = ApprovalScope::Session;
        self
    }

    pub fn with_session_rules(mut self, rules: Value) -> Self {
        self.session_rules = Some(rules);
        self
    }

    /// Whether the decision lets the blocked work go ahead.
    pub fn is_approved(&self) -> bool {
        matches!(
            self.decision,
            RequestDecision::Approve | RequestDecision::Answer { .. }
        )
    }
}

/// How a request ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RequestResolution {
    Responded {
        response: RequestResponse,
    },
    /// The host withdrew the request; the run treats it as not approved.
    Withdrawn,
    /// No answer within the run's request timeout; not approved.
    TimedOut,
    /// The run stopped while the request was pending.
    Cancelled,
}

impl RequestResolution {
    /// Only an approving response lets the blocked work go ahead; every other
    /// ending fails closed.
    pub fn is_approved(&self) -> bool {
        matches!(self, Self::Responded { response } if response.is_approved())
    }
}

/// Why a reply to a request was not delivered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RequestError {
    #[error("no pending request {0}")]
    NotFound(RequestId),
}

struct Entry {
    request: PendingRequest,
    tx: oneshot::Sender<RequestResolution>,
}

/// Pending requests of a run tree, keyed by request id.
#[derive(Clone, Default)]
pub struct RequestBus {
    pending: Arc<Mutex<HashMap<RequestId, Entry>>>,
}

impl std::fmt::Debug for RequestBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestBus").finish_non_exhaustive()
    }
}

impl RequestBus {
    /// Registers `request`; the receiver yields its resolution.
    pub(crate) async fn open(
        &self,
        request: PendingRequest,
    ) -> oneshot::Receiver<RequestResolution> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .await
            .insert(request.id.clone(), Entry { request, tx });
        rx
    }

    /// Ends a request the run stopped waiting for (timeout), if still open.
    pub(crate) async fn close(&self, id: &RequestId) {
        self.pending.lock().await.remove(id);
    }

    /// Resolves every pending request of `run_id` as cancelled.
    pub(crate) async fn cancel_run(&self, run_id: RunId) {
        let mut guard = self.pending.lock().await;
        let ids: Vec<RequestId> = guard
            .values()
            .filter(|e| e.request.run_id == run_id)
            .map(|e| e.request.id.clone())
            .collect();
        for id in ids {
            if let Some(entry) = guard.remove(&id) {
                let _ = entry.tx.send(RequestResolution::Cancelled);
            }
        }
    }

    /// Pending requests, ordered by id.
    pub async fn pending(&self) -> Vec<PendingRequest> {
        let mut requests: Vec<PendingRequest> = self
            .pending
            .lock()
            .await
            .values()
            .map(|e| e.request.clone())
            .collect();
        requests.sort_by(|a, b| a.id.cmp(&b.id));
        requests
    }

    /// Looks up one pending request.
    pub async fn get(&self, id: &RequestId) -> Option<PendingRequest> {
        self.pending.lock().await.get(id).map(|e| e.request.clone())
    }

    /// Answers a pending request.
    pub async fn respond(
        &self,
        id: &RequestId,
        response: RequestResponse,
    ) -> Result<(), RequestError> {
        self.resolve(id, RequestResolution::Responded { response })
            .await
    }

    /// Withdraws a pending request; the run continues as if it was denied.
    pub async fn withdraw(&self, id: &RequestId) -> Result<(), RequestError> {
        self.resolve(id, RequestResolution::Withdrawn).await
    }

    async fn resolve(
        &self,
        id: &RequestId,
        resolution: RequestResolution,
    ) -> Result<(), RequestError> {
        let entry = self
            .pending
            .lock()
            .await
            .remove(id)
            .ok_or_else(|| RequestError::NotFound(id.clone()))?;
        entry
            .tx
            .send(resolution)
            .map_err(|_| RequestError::NotFound(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(run_id: RunId, seq: u64) -> PendingRequest {
        PendingRequest {
            id: RequestId::new(run_id, seq),
            run_id,
            kind: RequestKind::new("question"),
            payload: json!({"q": seq}),
            target: RequestTarget::Run,
        }
    }

    #[tokio::test]
    async fn several_requests_wait_and_resolve_by_id() {
        let bus = RequestBus::default();
        let run = RunId::new();
        let rx1 = bus.open(request(run, 1)).await;
        let rx2 = bus.open(request(run, 2)).await;
        assert_eq!(bus.pending().await.len(), 2);

        let id2 = RequestId::new(run, 2);
        bus.respond(&id2, RequestResponse::answer(json!("yes")))
            .await
            .unwrap();
        assert!(rx2.await.unwrap().is_approved());
        assert_eq!(bus.pending().await.len(), 1);
        assert!(bus.get(&id2).await.is_none());

        let id1 = RequestId::new(run, 1);
        bus.withdraw(&id1).await.unwrap();
        assert_eq!(rx1.await.unwrap(), RequestResolution::Withdrawn);
        assert!(matches!(
            bus.respond(&id1, RequestResponse::approve()).await,
            Err(RequestError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn cancel_run_only_touches_that_run() {
        let bus = RequestBus::default();
        let (a, b) = (RunId::new(), RunId::new());
        let rx_a = bus.open(request(a, 1)).await;
        let _rx_b = bus.open(request(b, 1)).await;
        bus.cancel_run(a).await;
        assert_eq!(rx_a.await.unwrap(), RequestResolution::Cancelled);
        let left = bus.pending().await;
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].run_id, b);
    }

    #[test]
    fn only_an_approving_response_approves() {
        assert!(RequestResolution::Responded {
            response: RequestResponse::approve()
        }
        .is_approved());
        assert!(!RequestResolution::Responded {
            response: RequestResponse::deny(None)
        }
        .is_approved());
        assert!(!RequestResolution::Withdrawn.is_approved());
        assert!(!RequestResolution::TimedOut.is_approved());
        assert!(!RequestResolution::Cancelled.is_approved());
    }
}
