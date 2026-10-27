/**
 * Hand-written public types for `@orchest/sdk`.
 *
 * Layout: `index.js` is the runtime and — through the JSDoc annotations on its
 * functions and classes — the source of the declarations in the generated
 * `index.d.ts`. Data shapes that have no runtime counterpart live here;
 * `native.d.ts` declares the napi addon surface in terms of them, and
 * `public.d.ts` re-exports all three as the package's `types` entry.
 */

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

export type DecisionDescription = string | unknown[] | Record<string, unknown>;
export type DecisionQuestion =
  | { type: "boolean"; instructions: DecisionDescription; criteria?: { true: DecisionDescription; false: DecisionDescription } }
  | { type: "choice"; instructions: DecisionDescription; criteria: Record<string, DecisionDescription | null> }
  | { type: "score"; instructions: DecisionDescription; criteria: DecisionDescription[] };
export type DecisionAnswer =
  | { type: "boolean"; probability: number }
  | { type: "choice"; choice: string; probabilities?: Record<string, number>; confidence?: number }
  | { type: "score"; score: number; legend?: Record<string, unknown>; probabilities?: Record<string, number>; confidence?: number };
export interface DecisionOptions {
  model: string;
  state: unknown;
  questions: Record<string, DecisionQuestion>;
  apiKey?: string;
  apiKeyEnv?: string;
  apiUrl?: string;
  timeoutMs?: number;
}
export interface DecisionResponse {
  model: string;
  answers: Record<string, DecisionAnswer>;
  usage?: { input_tokens: number; output_tokens: number; cost_usd?: number };
  id?: string;
  provider?: string;
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

export interface ProviderErrorDetails {
  message: string;
  code?: string;
  provider?: string;
  model?: string;
  status?: number;
  retryAfterSecs?: number;
  upstream?: unknown;
  diagnosticMetadata?: unknown;
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
  handler(input: Record<string, unknown>): unknown;
  sideEffect?: boolean;
  approval?: string;
}

/** Why a run was cancelled. */
export type CancelReason = "user" | "timeout" | "upstream" | "host_disconnected" | "watcher";

export type ToolEffect =
  | "read"
  | "write_workspace"
  | "write_outside"
  | "network"
  | "credential"
  | "destructive"
  | "process";

export interface ToolMetadata {
  /** Declared effects; `null` when unknown (treated as having side effects). */
  effects: ToolEffect[] | null;
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

/** The host's answer to a request (`Agent.respond`). */
export interface RequestResponse {
  /** A bare "approve" / "deny", or the tagged form (`answer` carries a payload). */
  decision:
    | "approve"
    | "deny"
    | { type: "approve" }
    | { type: "deny"; reason?: string | null }
    | { type: "answer"; payload: unknown };
  /** "session" remembers an approval for the rest of the session. Default "once". */
  scope?: "once" | "session";
  session_rules?: unknown;
}

/** What a run supports (`Agent.capabilities`). */
export interface Capabilities {
  resume: boolean;
  preemptive_cancel: boolean;
  approval_scopes: Array<"once" | "session">;
  builtin_request_kinds: string[];
  host_request_kinds: boolean;
  parallel_tools: boolean;
  delegation: boolean;
  effects: ToolEffect[];
  enforcement: {
    backend: string | null;
    isolation: "none" | "soft" | "hard";
    permission_gate: "none" | "soft" | "hard";
    code_execution: "none" | "soft" | "hard" | null;
    read_only: "none" | "soft" | "hard";
  };
  lossless_events: boolean;
}

/** One incremental piece of an assistant message (`message_updated`). */
export type MessageDelta =
  | { type: "text"; text: string }
  | { type: "thinking"; text: string }
  | { type: "tool_use_args"; tool_use_id: string; delta: string };

/** A blocking request waiting for the host (`request_opened`). */
export interface PendingRequest {
  id: string;
  /** The run that raised it (a child run for delegated work). */
  run_id: string;
  /** "tool_approval", "retry_approval", or a host-defined kind. */
  kind: string;
  payload: unknown;
  target: unknown;
}

/**
 * Events on the run stream, in the binding wire shape: serde's externally
 * tagged form plus `type` (snake_case discriminant) and an injected
 * `run_depth` / `child_run_id` pair. One variant per core `RuntimeEvent`
 * variant; `scripts/check-ts-event-variants.sh` fails when the two drift.
 */
export type RuntimeEvent =
  | { type: "run_started"; run_id: string; run_depth: number; child_run_id: string | null }
  | { type: "model_call_started"; step: number; run_depth: number; child_run_id: string | null }
  | { type: "model_stream_chunk"; delta: StreamEvent; run_depth: number; child_run_id: string | null }
  | { type: "message_started"; message_id: string; step: number; run_depth: number; child_run_id: string | null }
  | { type: "message_updated"; message_id: string; delta: MessageDelta; run_depth: number; child_run_id: string | null }
  | { type: "message_ended"; message_id: string; content: unknown[]; stop_reason: string | null; run_depth: number; child_run_id: string | null }
  | { type: "model_call_completed"; tokens: TokenUsage; option_adjustments?: OptionAdjustment[]; run_depth: number; child_run_id: string | null }
  | { type: "model_retry"; attempt: number; error: string; next_delay: unknown; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_started"; tool: string; metadata: ToolMetadata; input: unknown; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_update"; tool: string; tool_call_id: string; partial: unknown; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_completed"; tool: string; output: unknown; duration: unknown; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_failed"; tool: string; error: ToolExecutionError; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_retry"; tool: string; attempt: number; previous_error: ToolExecutionError; next_delay: unknown; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_batch_started"; batch_id: string; tool_count: number; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_batch_item_started"; batch_id: string; tool: string; requested_order: number; run_depth: number; child_run_id: string | null }
  | { type: "tool_call_batch_item_completed"; batch_id: string; tool: string; requested_order: number; completion_order: number; run_depth: number; child_run_id: string | null }
  | { type: "async_tool_started"; tool: string; job_id: string; run_depth: number; child_run_id: string | null }
  | { type: "async_tool_progress"; tool: string; job_id: string; status: unknown; run_depth: number; child_run_id: string | null }
  | { type: "async_tool_completed"; tool: string; job_id: string; output: unknown; elapsed: unknown; run_depth: number; child_run_id: string | null }
  | { type: "skill_content_read"; skill_name: string; file: string; tokens: number; run_depth: number; child_run_id: string | null }
  | { type: "tool_dispatched"; tool_use_id: string; tool_name: string; run_depth: number; child_run_id: string | null }
  | { type: "tool_settled"; tool_use_id: string; is_error: boolean; error_code: string | null; run_depth: number; child_run_id: string | null }
  | { type: "permission_decided"; tool_call_id: string; tool: string; decision: unknown; run_depth: number; child_run_id: string | null }
  | { type: "approval_requested"; request_id: string; kind: string; payload: unknown; tool_call: unknown; context?: unknown; run_depth: number; child_run_id: string | null }
  | { type: "approval_granted"; request_id: string; tool_call: unknown; context?: unknown; run_depth: number; child_run_id: string | null }
  | { type: "approval_denied"; request_id: string; tool_call: unknown; context?: unknown; run_depth: number; child_run_id: string | null }
  | { type: "request_opened"; request: PendingRequest; run_depth: number; child_run_id: string | null }
  | { type: "request_resolved"; request_id: string; kind: string; resolution: unknown; run_depth: number; child_run_id: string | null }
  | { type: "budget_warning"; used: unknown; limit: unknown; run_depth: number; child_run_id: string | null }
  | { type: "runtime_warning"; message: string; run_depth: number; child_run_id: string | null }
  | { type: "skill_missing_capabilities"; skill_name: string; run_depth: number; child_run_id: string | null }
  | { type: "skill_load_warning"; path: string; reason: string; run_depth: number; child_run_id: string | null }
  | { type: "context_compacted"; removed_messages: number; summary_tokens: number; run_depth: number; child_run_id: string | null }
  | { type: "sub_agent_started"; parent_run_id: string; child_run_id: string; config_summary: unknown; run_depth: number }
  | { type: "sub_agent_completed"; child_run_id: string; output: unknown; budget_used: unknown; run_depth: number }
  | { type: "sub_agent_failed"; child_run_id: string; error: string; run_depth: number }
  | { type: "child_run_event"; child_run_id: string; run_depth: number; event: RuntimeEvent }
  | { type: "sub_agent_event"; parent_run_id: string; child_run_id: string; run_depth: number; event: RuntimeEvent }
  | { type: "hook_panicked"; hook_name: string; message: string; run_depth: number; child_run_id: string | null }
  | { type: "agent_updated"; previous_agent: string; new_agent: string; run_depth: number; child_run_id: string | null }
  | { type: "events_dropped"; subscriber_id: number; count: number; from_seq: number; to_seq: number; run_depth: number; child_run_id: string | null }
  | { type: "run_restarted"; attempt: number; run_depth: number; child_run_id: string | null }
  | { type: "run_completed"; output: unknown; stop_reason: StopReason; run_depth: number; child_run_id: string | null }
  | { type: "run_failed"; error: string; kind: RunFailureKind; run_depth: number; child_run_id: string | null }
  | { type: "run_aborted"; reason: string | null; cancel_reason: CancelReason | null; run_depth: number; child_run_id: string | null }
  | { type: "processes_terminated"; terminated: number | null; error: string | null; run_depth: number; child_run_id: string | null };

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
