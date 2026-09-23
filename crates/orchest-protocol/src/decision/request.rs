use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ErrorCode, ProtocolError};

/// Shared context plus independently evaluated questions, keyed by caller IDs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequest {
    /// Any JSON value. Individual providers may support a narrower input set.
    pub state: Value,
    pub questions: BTreeMap<String, DecisionQuestion>,
}

/// Descriptions of the positive and negative outcomes of a boolean judgment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BooleanCriteria {
    #[serde(rename = "true")]
    pub true_: Value,
    #[serde(rename = "false")]
    pub false_: Value,
}

/// A bounded judgment. Descriptions may be text, JSON objects, or arrays;
/// nested fields may contain any JSON value. Option names and question IDs
/// belong to the caller, not the provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum DecisionQuestion {
    /// Estimate P(true); consumers choose their own thresholds.
    Boolean {
        instructions: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<BooleanCriteria>,
    },
    /// Select one of the named options. Null descriptions are permitted.
    Choice {
        instructions: Value,
        criteria: BTreeMap<String, Value>,
    },
    /// Rate on ordered levels indexed from zero, allowing fractional scores.
    Score {
        instructions: Value,
        criteria: Vec<Value>,
    },
}

pub(super) fn is_description(value: &Value) -> bool {
    matches!(value, Value::String(_) | Value::Object(_) | Value::Array(_))
}

impl DecisionRequest {
    /// Validate portable question semantics, without imposing vendor limits
    /// on state, option counts, context windows, or token budgets.
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.questions.is_empty() {
            return Err(invalid("questions must not be empty"));
        }
        for question in self.questions.values() {
            question.validate()?;
        }
        Ok(())
    }
}

impl DecisionQuestion {
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    fn validate(&self) -> Result<(), ProtocolError> {
        let instructions = match self {
            Self::Boolean {
                instructions,
                criteria,
            } => {
                if let Some(criteria) = criteria {
                    if !is_description(&criteria.true_) || !is_description(&criteria.false_) {
                        return Err(invalid("boolean criteria must be descriptions"));
                    }
                }
                instructions
            }
            Self::Choice {
                instructions,
                criteria,
            } => {
                if criteria.is_empty()
                    || criteria
                        .values()
                        .any(|v| !v.is_null() && !is_description(v))
                {
                    return Err(invalid("choice criteria must contain descriptions or null"));
                }
                instructions
            }
            Self::Score {
                instructions,
                criteria,
            } => {
                if criteria.is_empty() || criteria.iter().any(|v| !is_description(v)) {
                    return Err(invalid("score criteria must contain ordered descriptions"));
                }
                instructions
            }
        };
        if !is_description(instructions) {
            return Err(invalid("instructions must be a string, object, or array"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidRequest, message)
}
