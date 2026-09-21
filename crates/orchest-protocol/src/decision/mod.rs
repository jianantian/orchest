//! Provider-independent structured judgments. Code owns control flow; a
//! decision implementation answers a batch of independent, bounded questions.

mod answer;
mod provider;
mod request;

pub use answer::{DecisionAnswer, DecisionResponse, DecisionUsage};
pub use provider::Decision;
pub use request::{BooleanCriteria, DecisionQuestion, DecisionRequest};
