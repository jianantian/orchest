export class Agent {
  constructor(options: {
    model: string;
    systemPrompt: string;
    skillsDir?: string;
    budget?: {
      maxTokens?: number;
      maxToolCalls?: number;
      maxDurationSecs?: number;
      maxCostUsd?: number;
    };
  });

  registerTool(options: {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    requiresApproval?: boolean;
    sideEffect?: boolean;
  }): void;

  runSync(input: string): unknown[];

  respondApproval(runId: string, approved: boolean): void;
}
