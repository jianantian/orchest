use async_trait::async_trait;

use super::{DecisionRequest, DecisionResponse};
use crate::{CapabilityDescriptor, ProtocolError};

/// An atomic decision engine: a remote model, local model, or application
/// implementation. No chat history, agent loop, HTTP transport, or token
/// accounting is required. Thresholds and actions belong to the caller.
///
/// Implementations validate requests with [`DecisionRequest::validate`] and
/// results with [`DecisionResponse::validate_for`]. Unsupported provider-specific
/// inputs must fail explicitly rather than silently changing the question.
#[async_trait]
pub trait Decision: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn descriptor(&self) -> CapabilityDescriptor;

    /// Evaluate all independent questions against the same state in one call.
    async fn decide(&self, request: DecisionRequest) -> Result<DecisionResponse, ProtocolError>;
}
