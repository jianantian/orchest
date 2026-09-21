/** Classify one customer-support case with OpenRouter Decisions.
 * Requires: OPENROUTER_API_KEY
 */
import { decide } from "@orchest/sdk";

async function main(): Promise<void> {
  const result = await decide({
    model: "openrouter/~typesafe/jev-latest",
    apiKeyEnv: "OPENROUTER_API_KEY",
    state: {
      message: "I was charged twice and need this fixed today.",
      customer: { plan: "pro", openCases: 0 },
    },
    questions: {
      urgent: {
        type: "boolean",
        instructions: "Does this need urgent handling?",
      },
      team: {
        type: "choice",
        instructions: "Which team should own the case?",
        criteria: { billing: "Payment issues", support: "Product help" },
      },
      severity: {
        type: "score",
        instructions: "Rate customer impact.",
        criteria: ["Low", "Moderate", { label: "High", escalate: true }],
      },
    },
  });

  const urgent = result.answers.urgent;
  const severity = result.answers.severity;
  const team = result.answers.team;
  if (
    (urgent.type === "boolean" && urgent.probability >= 0.8) ||
    (severity.type === "score" && severity.score >= 1.5) ||
    (team.type === "choice" && team.confidence !== undefined && team.confidence < 0.6)
  ) {
    console.log("Escalate to a human");
  }
  console.log(result);
}

void main();
