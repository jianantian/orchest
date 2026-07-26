export interface AgentOptions {
  model: string;
  systemPrompt: string;
  skillsDir?: string;
  /** Progressive skill disclosure; pass `false` to disable prompt injection and the load_skill tool. */
  skillDisclosure?: boolean;
  apiKey?: string;
  apiKeyEnv?: string;
  apiUrl?: string;
  maxTokens?: number;
  requestOptions?: RequestOptions;
  budget?: BudgetOptions;
  /** Run-level approval policy: "perTool" | "none" | "all". */
  approvalMode?: string;
}

export interface RequestOptions {
  thinking?: "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
  thinkingBudgetTokens?: number;
  includeThinking?: boolean;
  compatibilityPolicy?: "coerce" | "strict";
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  cachePolicy?: "none" | "auto" | "long";
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
  sideEffect?: boolean;
  /** "never" | "whenRisky" | "always". */
  approval?: string;
}

export interface ToolWithHandler {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
  handler: (input: any) => any;
  sideEffect?: boolean;
  approval?: string;
}

export interface ToolMetadata {
  side_effect: boolean;
  approval: string;
  cost_hint: unknown;
  timeout: unknown;
  max_output_tokens: number | null;
  source: unknown;
}

export interface ToolExecutionError {
  message: string;
  kind: string;
  retry: string;
  code: string | null;
  next_step: string | null;
}

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  reasoning_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  details: Record<string, number>;
}

export interface OptionAdjustment {
  option: string;
  requested: unknown;
  applied: unknown;
  reason: string;
}

export type StreamEvent =
  | { Text: { delta: string } }
  | "ThinkingStart"
  | { Thinking: { delta: string } }
  | { ThinkingEnd: { signature?: string | null; provider_details?: unknown } }
  | { ToolUseStart: { id: string; name: string } }
  | { ToolUseArgsChunk: { id: string; delta: string } }
  | { ToolUseEnd: { id: string } }
  | { Done: { usage: TokenUsage } }
  | unknown;

export type RuntimeEvent =
  | { type: "run_started"; run_id: string; run_depth: number }
  | { type: "model_call_started"; step: number; run_depth: number }
  | { type: "model_stream_chunk"; delta: StreamEvent; run_depth: number }
  | { type: "model_call_completed"; tokens: TokenUsage; option_adjustments?: OptionAdjustment[]; run_depth: number }
  | { type: "tool_call_started"; tool: string; metadata: ToolMetadata; input: unknown; run_depth: number }
  | { type: "tool_call_update"; tool: string; tool_call_id: string; partial: unknown; run_depth: number }
  | { type: "tool_call_completed"; tool: string; output: unknown; duration: unknown; run_depth: number }
  | { type: "tool_call_failed"; tool: string; error: ToolExecutionError; run_depth: number }
  | { type: "async_tool_started"; tool: string; job_id: string; run_depth: number }
  | { type: "async_tool_progress"; tool: string; job_id: string; status: unknown; run_depth: number }
  | { type: "async_tool_completed"; tool: string; job_id: string; output: unknown; elapsed: unknown; run_depth: number }
  | { type: "skill_content_read"; skill_name: string; file: string; tokens: number; run_depth: number }
  | { type: "approval_requested"; tool_call: unknown; run_depth: number }
  | { type: "approval_granted"; tool_call: unknown; run_depth: number }
  | { type: "approval_denied"; tool_call: unknown; run_depth: number }
  | { type: "budget_warning"; used: unknown; limit: unknown; run_depth: number }
  | { type: "runtime_warning"; message: string; run_depth: number }
  | { type: "skill_missing_capabilities"; skill_name: string; run_depth: number }
  | { type: "skill_load_warning"; path: string; reason: string; run_depth: number }
  | { type: "context_compacted"; removed_messages: number; summary_tokens: number; run_depth: number }
  | { type: "child_run_event"; child_run_id: string; run_depth: number; event: RuntimeEvent }
  | { type: "sub_agent_started"; parent_run_id: string; child_run_id: string; config_summary: unknown; run_depth: number }
  | { type: "sub_agent_completed"; child_run_id: string; output: unknown; budget_used: unknown; run_depth: number }
  | { type: "sub_agent_failed"; child_run_id: string; error: string; run_depth: number }
  | { type: "run_restarted"; attempt: number; run_depth: number }
  | { type: "run_aborted"; reason: string | null; run_depth: number; child_run_id: string | null }
  | { type: "events_dropped"; subscriber_id: number; count: number; run_depth: number; child_run_id: string | null }
  | { type: "run_completed"; output: unknown; run_depth: number }
  | { type: "run_failed"; error: string; run_depth: number };

export class Agent {
  constructor(options: AgentOptions);
  registerTool(options: ToolRegistration): void;
  registerToolWithHandler(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: any) => any,
    options?: { sideEffect?: boolean; approval?: string },
  ): void;
  runSync(input: string): RuntimeEvent[];
  runStream(input: string, onEvent: (event: RuntimeEvent) => void): void;
  respondApproval(runId: string, approved: boolean): void;
}
