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
    /** Run-level approval policy: "perTool" | "none" | "all" | "sideEffectOnly". */
    approvalMode?: string;
  });

  /** Register a tool with schema only (no handler — tool calls will error). */
  registerTool(options: {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    /** Deprecated: use `approval` instead. */
    requiresApproval?: boolean;
    sideEffect?: boolean;
    /** "never" | "whenRisky" | "always". Takes priority over `requiresApproval`. */
    approval?: string;
  }): void;

  /** Register a tool with an executable handler function. */
  registerToolWithHandler(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: any) => any,
    options?: { requiresApproval?: boolean; sideEffect?: boolean; approval?: string },
  ): void;

  /** Run the agent synchronously, returning all events as an array. */
  runSync(input: string): unknown[];

  /** Stream events as they arrive; calls `onEvent` for each one. */
  runStream(input: string, onEvent: (event: unknown) => void): void;

  /** Respond to an approval request for an active run. */
  respondApproval(runId: string, approved: boolean): void;
}
