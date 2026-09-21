/**
 * Native addon surface (`orchest_node.node`) plus the wire types it returns.
 *
 * Shared option and wire types live in `index.d.ts`; this file declares only
 * the napi classes and functions, typed in terms of them.
 */

import type {
  AgentOptions,
  AsrStreamOptions,
  CompletionOptions,
  DecisionOptions,
  DecisionResponse,
  HistoryMessage,
  RuntimeEvent,
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
    /** "never" | "whenRisky" | "always". */
    approval?: string;
  }): void;

  /** Register a tool with an executable handler function. */
  registerToolWithHandler<TInput = Record<string, unknown>, TOutput = unknown>(
    name: string,
    description: string,
    inputSchema: Record<string, unknown>,
    handler: (input: TInput) => TOutput,
    options?: { sideEffect?: boolean; approval?: string },
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
    options?: { sideEffect?: boolean; approval?: string },
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

  /** Respond to an approval request for an active run. */
  respondApproval(runId: string, approved: boolean): void;
}
