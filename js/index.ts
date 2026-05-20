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
  | { type: "RunStarted"; run_id: string }
  | { type: "ModelCallStarted"; step: number }
  | { type: "ModelStreamChunk"; delta: unknown }
  | { type: "ModelCallCompleted"; tokens: { input_tokens: number; output_tokens: number } }
  | { type: "ToolCallStarted"; tool: string; source: unknown; input: unknown }
  | { type: "ToolCallUpdate"; tool: string; tool_call_id: string; partial: unknown }
  | { type: "ToolCallCompleted"; tool: string; output: unknown; duration: unknown }
  | { type: "ToolCallFailed"; tool: string; error: string }
  | { type: "AsyncToolStarted"; tool: string; job_id: string }
  | { type: "AsyncToolProgress"; tool: string; job_id: string; status: unknown }
  | { type: "AsyncToolCompleted"; tool: string; job_id: string; output: unknown; elapsed: unknown }
  | { type: "SkillContentRead"; skill_name: string; file: string; tokens: number }
  | { type: "ApprovalRequested"; tool_call: unknown }
  | { type: "ApprovalGranted"; tool_call: unknown }
  | { type: "ApprovalDenied"; tool_call: unknown }
  | { type: "BudgetWarning"; used: unknown; limit: unknown }
  | { type: "RunCompleted"; output: unknown }
  | { type: "RunFailed"; error: string };

export { Agent } from "./native";
