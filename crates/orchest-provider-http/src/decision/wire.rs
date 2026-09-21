//! OpenRouter wire DTOs, isolated from the portable Decision vocabulary.
//! Contract: https://openrouter.ai/openapi.json (2026-09-21).

use std::collections::BTreeMap;

use orchest_protocol::{
    BooleanCriteria, DecisionAnswer, DecisionQuestion, DecisionRequest, DecisionResponse,
    DecisionUsage,
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
    pub fn new(model: &'a str, request: &'a DecisionRequest) -> Self {
        Self {
            model,
            state: &request.state,
            questions: request
                .questions
                .iter()
                .map(|(id, q)| (id.as_str(), q.into()))
                .collect(),
        }
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

impl<'a> From<&'a DecisionQuestion> for Question<'a> {
    fn from(q: &'a DecisionQuestion) -> Self {
        match q {
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
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Response {
    model: String,
    answers: BTreeMap<String, Answer>,
    // Required on OpenRouter even though generic/local engines can omit usage.
    usage: Usage,
    id: Option<String>,
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
        probabilities: Option<BTreeMap<String, f64>>,
        confidence: Option<f64>,
    },
    Score {
        score: f64,
        legend: Option<BTreeMap<String, Value>>,
        probabilities: Option<BTreeMap<String, f64>>,
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
