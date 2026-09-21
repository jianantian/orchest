//! Independent structured judgments, composed by application code.
//! OPENROUTER_API_KEY=... cargo run -p orchest --example decisions

use orchest_protocol::{BooleanCriteria, DecisionAnswer, DecisionQuestion, DecisionRequest};
use orchest_provider::{decide, DecisionConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request = DecisionRequest {
        state: json!({"message": "Help! My payouts have been failing for 3 days."}),
        questions: [
            (
                "is_urgent".into(),
                DecisionQuestion::Boolean {
                    instructions: json!("Does this message convey urgency?"),
                    criteria: Some(BooleanCriteria {
                        true_: json!("Explicitly time-sensitive"),
                        false_: json!("No urgency expressed"),
                    }),
                },
            ),
            (
                "department".into(),
                DecisionQuestion::Choice {
                    instructions: json!({"question": "Which team should handle this?"}),
                    criteria: [
                        ("billing".into(), json!("Payments, invoicing, refunds")),
                        ("technical".into(), json!("Bugs, outages, integrations")),
                        ("sales".into(), json!("Pricing, upgrades, new accounts")),
                    ]
                    .into(),
                },
            ),
            (
                "frustration".into(),
                DecisionQuestion::Score {
                    instructions: json!("How frustrated is the customer?"),
                    criteria: vec![json!("Calm"), json!("Frustrated"), json!("Very angry")],
                },
            ),
        ]
        .into(),
    };
    // Only this configuration selects a deployment. The request, result, and
    // application policy also work with other registered Decision engines.
    let response = decide(
        &DecisionConfig::new("openrouter/~typesafe/jev-latest"),
        request,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    match response.answers.get("department") {
        Some(DecisionAnswer::Choice {
            choice,
            confidence: Some(confidence),
            ..
        }) if *confidence >= 0.8 => {
            println!("Route to {choice}");
        }
        _ => println!(
            "Request human review: confidence is missing or below the application threshold"
        ),
    }
    Ok(())
}
