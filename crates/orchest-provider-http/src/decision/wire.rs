//! OpenRouter wire DTOs, isolated from the portable Decision vocabulary.
//! Contract: <https://openrouter.ai/openapi.json> (2026-09-21).

use std::collections::BTreeMap;

use orchest_protocol::{
    BooleanCriteria, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
    DecisionUsage, ErrorCode, ProtocolError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct Request<'a> {
    pub model: &'a str,
    pub state: &'a Value,
    pub questions: BTreeMap<&'a str, Question<'a>>,
}

impl<'a> Request<'a> {
    pub fn new(model: &'a str, request: &'a DecisionRequest) -> Result<Self, ProtocolError> {
        Ok(Self {
            model,
            state: &request.state,
            questions: request
                .questions
                .iter()
                .map(|(id, q)| Ok((id.as_str(), Question::try_from(q)?)))
                .collect::<Result<_, ProtocolError>>()?,
        })
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Question<'a> {
    Noul {
        instructions: &'a Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<&'a BooleanCriteria>,
    },
    Choice {
        instructions: &'a Value,
        criteria: &'a BTreeMap<String, Value>,
    },
    Score {
        instructions: &'a Value,
        criteria: &'a Vec<Value>,
    },
}

impl<'a> TryFrom<&'a DecisionQuestion> for Question<'a> {
    type Error = ProtocolError;

    /// Fails for question kinds added to the protocol after this dialect was
    /// written (`DecisionQuestion` is `#[non_exhaustive]`).
    fn try_from(q: &'a DecisionQuestion) -> Result<Self, Self::Error> {
        Ok(match q {
            DecisionQuestion::Boolean {
                instructions,
                criteria,
            } => Self::Noul {
                instructions,
                criteria: criteria.as_ref(),
            },
            DecisionQuestion::Choice {
                instructions,
                criteria,
            } => Self::Choice {
                instructions,
                criteria,
            },
            DecisionQuestion::Score {
                instructions,
                criteria,
            } => Self::Score {
                instructions,
                criteria,
            },
            _ => {
                return Err(ProtocolError::new(
                    ErrorCode::InvalidRequest,
                    "OpenRouter Decisions does not support this question kind",
                ))
            }
        })
    }
}

#[derive(Deserialize)]
pub(super) struct Response {
    model: String,
    answers: BTreeMap<String, Answer>,
    // Required on OpenRouter even though generic/local engines can omit usage.
    usage: Usage,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: u64,
    output_tokens: u64,
    cost: Option<f64>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Answer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        #[serde(default)]
        probabilities: Option<BTreeMap<String, f64>>,
        #[serde(default)]
        confidence: Option<f64>,
    },
    Score {
        score: f64,
        #[serde(default)]
        legend: Option<BTreeMap<String, Value>>,
        #[serde(default)]
        probabilities: Option<BTreeMap<String, f64>>,
        #[serde(default)]
        confidence: Option<f64>,
    },
}

impl From<Answer> for DecisionAnswer {
    fn from(answer: Answer) -> Self {
        match answer {
            Answer::Noul { noul } => Self::Boolean { probability: noul },
            Answer::Choice {
                choice,
                probabilities,
                confidence,
            } => Self::Choice {
                choice,
                probabilities,
                confidence,
            },
            Answer::Score {
                score,
                legend,
                probabilities,
                confidence,
            } => Self::Score {
                score,
                legend,
                probabilities,
                confidence,
            },
        }
    }
}

impl From<Response> for DecisionResponse {
    fn from(response: Response) -> Self {
        Self {
            model: response.model,
            id: response.id,
            provider: response.provider,
            answers: response
                .answers
                .into_iter()
                .map(|(id, a)| (id, a.into()))
                .collect(),
            usage: Some(DecisionUsage {
                input_tokens: response.usage.input_tokens,
                output_tokens: response.usage.output_tokens,
                cost_usd: response.usage.cost,
            }),
        }
    }
}
