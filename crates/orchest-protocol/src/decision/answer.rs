use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::request::is_description;
use super::{DecisionQuestion, DecisionRequest};
use crate::{ErrorCode, ProtocolError};

/// A structured answer. Missing confidence or distribution means unavailable,
/// never zero certainty. Values are preserved, not recomputed or thresholded.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Boolean {
        /// Probability of the positive outcome, in [0, 1].
        probability: f64,
    },
    Choice {
        choice: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probabilities: Option<BTreeMap<String, f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Score {
        /// Probability-weighted zero-based level; fractional values are valid.
        score: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        legend: Option<BTreeMap<String, Value>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        probabilities: Option<BTreeMap<String, f64>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
}

/// Token accounting, when the implementation supplies it. Local decision
/// engines need not report usage; an absent cost is not a claim of zero cost.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionResponse {
    pub model: String,
    pub answers: BTreeMap<String, DecisionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<DecisionUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

impl DecisionResponse {
    /// Check correlation and answer domains against the original questions.
    /// This works for any Decision implementation and any transport.
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    pub fn validate_for(&self, request: &DecisionRequest) -> Result<(), ProtocolError> {
        request.validate()?;
        if self.model.trim().is_empty() || !self.answers.keys().eq(request.questions.keys()) {
            return Err(invalid("response model or question IDs are invalid"));
        }
        if self
            .usage
            .as_ref()
            .and_then(|u| u.cost_usd)
            .is_some_and(|c| !c.is_finite() || c < 0.0)
        {
            return Err(invalid("response cost must be finite and nonnegative"));
        }
        for (id, question) in &request.questions {
            self.answers[id].validate_for(question)?;
        }
        Ok(())
    }
}

impl DecisionAnswer {
    #[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
    fn validate_for(&self, question: &DecisionQuestion) -> Result<(), ProtocolError> {
        match (self, question) {
            (Self::Boolean { probability }, DecisionQuestion::Boolean { .. }) => {
                probability_value(*probability)?;
            }
            (
                Self::Choice {
                    choice,
                    probabilities,
                    confidence,
                },
                DecisionQuestion::Choice { criteria, .. },
            ) => {
                if !criteria.contains_key(choice) {
                    return Err(invalid("choice is not a supplied option"));
                }
                distribution(probabilities.as_ref(), criteria.keys().cloned().collect())?;
                optional_confidence(*confidence)?;
            }
            (
                Self::Score {
                    score,
                    legend,
                    probabilities,
                    confidence,
                },
                DecisionQuestion::Score { criteria, .. },
            ) => {
                if !score.is_finite() || !(0.0..=(criteria.len() - 1) as f64).contains(score) {
                    return Err(invalid("score is outside the supplied levels"));
                }
                let keys: BTreeSet<_> = (0..criteria.len()).map(|i| i.to_string()).collect();
                if let Some(legend) = legend {
                    if legend.keys().cloned().collect::<BTreeSet<_>>() != keys
                        || legend.values().any(|v| !is_description(v))
                    {
                        return Err(invalid("score legend does not match supplied levels"));
                    }
                }
                distribution(probabilities.as_ref(), keys)?;
                optional_confidence(*confidence)?;
            }
            _ => return Err(invalid("answer type does not match question type")),
        }
        Ok(())
    }
}

#[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
fn distribution(
    values: Option<&BTreeMap<String, f64>>,
    keys: BTreeSet<String>,
) -> Result<(), ProtocolError> {
    if let Some(values) = values {
        if values.keys().cloned().collect::<BTreeSet<_>>() != keys {
            return Err(invalid("probability keys do not match the question domain"));
        }
        for value in values.values() {
            probability_value(*value)?;
        }
        // Accommodate providers that round decimal probabilities on the wire.
        if (values.values().sum::<f64>() - 1.0).abs() > 0.001 {
            return Err(invalid("probabilities must sum to one"));
        }
    }
    Ok(())
}

#[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
fn optional_confidence(value: Option<f64>) -> Result<(), ProtocolError> {
    if let Some(value) = value {
        probability_value(value)?;
    }
    Ok(())
}

#[allow(clippy::result_large_err)] // justified: shared ProtocolError carries structured diagnostics
fn probability_value(value: f64) -> Result<(), ProtocolError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(invalid(
            "probability and confidence must be finite values in [0, 1]",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidResponse, message)
}
