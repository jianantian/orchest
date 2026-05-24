export class Agent {
  constructor(options: {
    model: string;
    systemPrompt: string;
    skillsDir?: string;
    apiKey?: string;
    apiKeyEnv?: string;
    apiUrl?: string;
    maxTokens?: number;
    requestOptions?: {
      thinking?: "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
      thinkingBudgetTokens?: number;
      includeThinking?: boolean;
      compatibilityPolicy?: "coerce" | "strict";
      maxTokens?: number;
      temperature?: number;
      topP?: number;
      cachePolicy?: "none" | "auto" | "long";
    };
    budget?: {
      maxTokens?: number;
      maxToolCalls?: number;
      maxDurationSecs?: number;
      maxCostUsd?: number;
    };
  });

  /** Register a tool with schema only (no handler — tool calls will error). */
  registerTool(options: {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    requiresApproval?: boolean;
    sideEffect?: boolean;
  }): void;

  /** Register a tool with an executable handler function. */
  registerToolWithHandler(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: any) => any,
    options?: { requiresApproval?: boolean; sideEffect?: boolean },
  ): void;

  /** Run the agent synchronously, returning all events as an array. */
  runSync(input: string): unknown[];

  /** Respond to an approval request for an active run. */
  respondApproval(runId: string, approved: boolean): void;
}
