/**
 * Native addon surface (`orchest_node.node`) plus the wire types it returns.
 *
 * Shared option and wire types live in `index.d.ts`; this file declares only
 * the napi classes and functions, typed in terms of them.
 */

import type {
  AgentOptions,
  AsrStreamOptions,
  CancelReason,
  Capabilities,
  CompletionOptions,
  DecisionOptions,
  DecisionResponse,
  HistoryMessage,
  RequestResponse,
  RuntimeEvent,
  ToolEffect,
  TranscribeOptions,
} from "./types";

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
export function _decide(options: DecisionOptions): Promise<DecisionResponse>;
export function _transcribe(audio: Uint8Array, options: TranscribeOptions): Promise<string>;

export class Agent {
  constructor(options: AgentOptions);

  /** Register a tool with schema only (no handler — tool calls will error). */
  registerTool(options: {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
    sideEffect?: boolean;
    /** Declared effects; overrides `sideEffect` when given. */
    effects?: ToolEffect[];
    /** "never" | "whenRisky" | "always". */
    approval?: string;
  }): void;

  /** Register a tool with an executable handler function. */
  registerToolWithHandler<TInput = Record<string, unknown>, TOutput = unknown>(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: TInput) => TOutput,
    options?: { sideEffect?: boolean; effects?: ToolEffect[]; approval?: string },
  ): void;

  /**
   * Register an async tool with a separate poll handler.
   * `handler(input)` returns `{ job_id, poll_interval_ms? }`.
   * `pollHandler(jobId)` returns `{ status, progress?, message?, result?, error? }`.
   */
  registerAsyncToolWithHandler<
    TInput = Record<string, unknown>,
    TJob extends { job_id: string; poll_interval_ms?: number } = { job_id: string; poll_interval_ms?: number },
    TPoll = unknown,
  >(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: TInput) => TJob,
    pollHandler: (jobId: string) => TPoll,
    options?: { sideEffect?: boolean; effects?: ToolEffect[]; approval?: string },
  ): void;

  /**
   * Run the agent and return all events as an array. The underlying napi
   * method is async, so the promise always resolves with the events.
   * `messages` (optional) is the prior conversation for a multi-turn start;
   * `input` is the new user turn.
   */
  runSync(input: string, messages?: HistoryMessage[]): Promise<RuntimeEvent[]>;

  /**
   * Stream events as they arrive; calls `onEvent` for each one. Synchronous:
   * the callback drives delivery and the call returns once streaming starts.
   * `messages` (optional) is the prior conversation for a multi-turn start.
   */
  runStream(
    input: string,
    onEvent: (event: RuntimeEvent) => void,
    messages?: HistoryMessage[],
  ): void;

  /**
   * Answer request `requestId` of the active run (the `request_id` of an
   * `approval_requested` event, or `request.id` of a `request_opened` one).
   */
  respond(requestId: string, payload: RequestResponse): void;

  /**
   * Cancel the active run. Returns `true` when this call cancelled it,
   * `false` when it was already cancelled or no run is active.
   */
  abort(reason?: CancelReason, message?: string): boolean;

  /** What a run of this agent supports. */
  capabilities(): Capabilities;
}
