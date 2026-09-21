/**
 * OpenRouter Decisions example: route one customer-support case with
 * independent boolean/choice/score judgments and act on the answers.
 *
 * Usage:
 *   npm run build:native
 *   OPENROUTER_API_KEY=sk-or-... node examples/typescript/providers/decisions.ts
 *
 * Requires: OPENROUTER_API_KEY
 */

type DecisionAnswer =
  | { type: "boolean"; probability: number }
  | { type: "choice"; choice: string; confidence?: number }
  | { type: "score"; score: number; confidence?: number };

interface DecisionResponse {
  model: string;
  answers: Record<string, DecisionAnswer>;
  usage?: { input_tokens: number; output_tokens: number; cost_usd?: number };
}

interface DecisionOptions {
  model: string;
  apiKeyEnv?: string;
  state: unknown;
  questions: Record<string, unknown>;
}

declare const __dirname: string;
declare const console: { log(...values: unknown[]): void; error(...values: unknown[]): void };
declare const process: {
  env: Record<string, string | undefined>;
  exit(code?: number): void;
};
declare function require(name: "node:path"): {
  join(...parts: string[]): string;
  resolve(...parts: string[]): string;
};
declare function require(name: string): {
  decide(options: DecisionOptions): Promise<DecisionResponse>;
};

const { join, resolve } = require("node:path");
const repoRoot = resolve(__dirname, "../../..");
const { decide } = require(join(repoRoot, "js/index.js"));

if (!process.env.OPENROUTER_API_KEY) {
  console.error("Set OPENROUTER_API_KEY first.");
  process.exit(1);
}

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
