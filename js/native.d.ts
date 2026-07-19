export class Agent {
  constructor(options: {
    model: string;
    systemPrompt: string;
    skillsDir?: string;
    /** Progressive skill disclosure; pass `false` to disable prompt injection and the load_skill tool. */
    skillDisclosure?: boolean;
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
    /** Run-level approval policy: "perTool" | "none" | "all". */
    approvalMode?: string;
  });

  /** Register a tool with schema only (no handler — tool calls will error). */
  registerTool(options: {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    sideEffect?: boolean;
    /** "never" | "whenRisky" | "always". */
    approval?: string;
  }): void;

  /** Register a tool with an executable handler function. */
  registerToolWithHandler(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: any) => any,
    options?: { sideEffect?: boolean; approval?: string },
  ): void;

  /**
   * Register an async tool with a separate poll handler.
   * `handler(input)` returns `{ job_id, poll_interval_ms? }`.
   * `pollHandler(jobId)` returns `{ status, progress?, message?, result?, error? }`.
   */
  registerAsyncToolWithHandler(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: any) => { job_id: string; poll_interval_ms?: number },
    pollHandler: (jobId: string) => { status: string; progress?: number; message?: string; result?: any; error?: string },
    options?: { sideEffect?: boolean; approval?: string },
  ): void;

  /** Run the agent and return all events as an array. */
  runSync(input: string): Promise<unknown[]>;

  /** Stream events as they arrive; calls `onEvent` for each one. */
  runStream(input: string, onEvent: (event: unknown) => void): void;

  /** Respond to an approval request for an active run. */
  respondApproval(runId: string, approved: boolean): void;
}
