/**
 * Orchest Agent Runtime - TypeScript SDK
 *
 * Provides a native Node.js binding to the Rust agent runtime core.
 * Built with napi-rs for high performance.
 */

export interface AgentOptions {
  model: string;
  systemPrompt: string;
  skillsDir?: string;
  apiUrl?: string;
  budget?: BudgetOptions;
}

export interface BudgetOptions {
  maxTokens?: number;
  maxToolCalls?: number;
  maxDurationSecs?: number;
  maxCostUsd?: number;
}

export interface ToolRegistration {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  requiresApproval?: boolean;
  sideEffect?: boolean;
}

export type RuntimeEvent =
  | { type: "runStarted"; run_id: string }
  | { type: "modelCallStarted"; step: number }
  | { type: "modelStreamChunk"; delta: unknown }
  | { type: "modelCallCompleted"; tokens: { input_tokens: number; output_tokens: number } }
  | { type: "toolCallStarted"; tool: string; source: unknown; input: unknown }
  | { type: "toolCallUpdate"; tool: string; tool_call_id: string; partial: unknown }
  | { type: "toolCallCompleted"; tool: string; output: unknown; duration: unknown }
  | { type: "toolCallFailed"; tool: string; error: string }
  | { type: "asyncToolStarted"; tool: string; job_id: string }
  | { type: "asyncToolProgress"; tool: string; job_id: string; status: unknown }
  | { type: "asyncToolCompleted"; tool: string; job_id: string; output: unknown; elapsed: unknown }
  | { type: "skillContentRead"; skill_name: string; file: string; tokens: number }
  | { type: "approvalRequested"; tool_call: unknown }
  | { type: "approvalGranted"; tool_call: unknown }
  | { type: "approvalDenied"; tool_call: unknown }
  | { type: "budgetWarning"; used: unknown; limit: unknown }
  | { type: "runCompleted"; output: unknown }
  | { type: "runFailed"; error: string };

export { Agent } from "./native";
