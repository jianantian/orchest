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

export interface ToolWithHandler {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (input: any) => any;
  requiresApproval?: boolean;
  sideEffect?: boolean;
}

export type RuntimeEvent =
  | { type: "run_started"; run_id: string }
  | { type: "model_call_started"; step: number }
  | { type: "model_stream_chunk"; delta: unknown }
  | { type: "model_call_completed"; tokens: { input_tokens: number; output_tokens: number } }
  | { type: "tool_call_started"; tool: string; source: unknown; input: unknown }
  | { type: "tool_call_update"; tool: string; tool_call_id: string; partial: unknown }
  | { type: "tool_call_completed"; tool: string; output: unknown; duration: unknown }
  | { type: "tool_call_failed"; tool: string; error: string }
  | { type: "async_tool_started"; tool: string; job_id: string }
  | { type: "async_tool_progress"; tool: string; job_id: string; status: unknown }
  | { type: "async_tool_completed"; tool: string; job_id: string; output: unknown; elapsed: unknown }
  | { type: "skill_content_read"; skill_name: string; file: string; tokens: number }
  | { type: "approval_requested"; tool_call: unknown }
  | { type: "approval_granted"; tool_call: unknown }
  | { type: "approval_denied"; tool_call: unknown }
  | { type: "budget_warning"; used: unknown; limit: unknown }
  | { type: "run_completed"; output: unknown }
  | { type: "run_failed"; error: string };

export { Agent } from "./native";
