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

export class NativeAsrStream {
  sendAudio(audio: Uint8Array): Promise<void>;
  finish(): void;
  wait(): Promise<void>;
}

export function _startAsrStream(
  options: AsrStreamOptions,
  onEvent: (event: Record<string, unknown>) => void,
): Promise<NativeAsrStream>;

export function _complete(options: CompletionOptions): Promise<string>;
export function _transcribe(audio: Uint8Array, options: TranscribeOptions): Promise<string>;

export class Agent {
  constructor(options: {
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
    /**
     * One-line recommended model retry policy (429 / 5xx / timeout /
     * stream-interrupt; 3 retries, exponential backoff with jitter).
     * Defaults to no retries.
     */
    retry?: boolean;
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

  /**
   * Run the agent and return all events as an array.
   * `messages` (optional) is the prior conversation for a multi-turn start;
   * `input` is the new user turn.
   */
  runSync(input: string, messages?: HistoryMessage[]): Promise<unknown[]>;

  /**
   * Stream events as they arrive; calls `onEvent` for each one.
   * `messages` (optional) is the prior conversation for a multi-turn start.
   */
  runStream(
    input: string,
    onEvent: (event: unknown) => void,
    messages?: HistoryMessage[],
  ): void;

  /** Respond to an approval request for an active run. */
  respondApproval(runId: string, approved: boolean): void;
}
