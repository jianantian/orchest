export interface AgentOptions {
  /** Human-readable identity used by run and handoff logs. */
  name: string;
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
  /**
   * Set to `true` to enable the recommended model retry policy
   * (429 / 5xx / timeout / stream-interrupt, 3 retries, exponential
   * backoff 1s→30s with jitter). Default: no retries.
   */
  retry?: boolean;
}

export interface CompletionOptions {
  model: string;
  user: string;
  system?: string;
  apiKey?: string;
  apiKeyEnv?: string;
  apiUrl?: string;
  jsonMode?: boolean;
  retry?: boolean;
  requestOptions?: RequestOptions;
}

export interface TranscribeOptions {
  format: "m4a" | "aac" | "wav" | "mp3" | "pcm";
  language?: string;
  provider?: string;
  apiKey?: string;
  apiKeyEnv?: string;
  apiUrl?: string;
  options?: Record<string, unknown>;
}

export interface AsrContextMessage {
  role: "user" | "assistant";
  text: string;
}

export interface AsrStreamOptions extends TranscribeOptions {
  sampleRate: number;
  context?: AsrContextMessage[];
}

export type AsrStreamEvent = Record<string, unknown>;

export class AsrStream {
  sendAudio(audio: Uint8Array): Promise<void>;
  finish(): void;
  wait(): Promise<void>;
}

export function startAsrStream(
  options: AsrStreamOptions,
  onEvent: (event: AsrStreamEvent) => void,
): Promise<AsrStream>;

export function complete(options: CompletionOptions): Promise<string>;
export function transcribe(audio: Uint8Array, options: TranscribeOptions): Promise<string>;

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

/**
 * Model stop reason for the completing turn, in the core serde JSON shape.
 * "EndTurn" means the output is complete; "MaxTokens" means it is truncated —
 * continue generation, retry with a larger token budget, or fail; do not
 * persist truncated output as-is.
 */
export type StopReason =
  | "EndTurn"
  | "ToolUse"
  | "MaxTokens"
  | "StopSequence"
  | "ContentFilter"
  | "Refusal"
  | "ContextWindowExceeded"
  | "Pause"
  | "Interrupted"
  | { Other: string };

/**
 * Structured run-failure classification, in the core serde JSON shape.
 * "BudgetExceeded" — the run's budget guard fired; "MaxStepsReached" — the
 * run hit its step ceiling; "Other" — any other failure, including events
 * emitted before this field existed. Dispatch on this, not on `error` text.
 */
export type RunFailureKind = "BudgetExceeded" | "MaxStepsReached" | "Other";

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
  | { type: "run_completed"; output: unknown; stop_reason: StopReason; run_depth: number }
  | { type: "run_failed"; error: string; kind: RunFailureKind; run_depth: number };

export interface HistoryMessage {
  /** "system" | "user" | "assistant" | "tool" (plus provider-specific roles). */
  role: string;
  /**
   * Content blocks in the core serde JSON shape, e.g. `{ Text: "..." }`,
   * `{ ToolUse: { id, name, input } }`, or `{ ToolResult: { tool_use_id, content } }`.
   * ToolUse blocks belong in assistant messages, each matching ToolResult in
   * the immediately following user message.
   */
  content: Array<Record<string, unknown>>;
}

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
  runSync(input: string, messages?: HistoryMessage[]): RuntimeEvent[];
  runStream(
    input: string,
    onEvent: (event: RuntimeEvent) => void,
    messages?: HistoryMessage[],
  ): void;
  respondApproval(runId: string, approved: boolean): void;
}
