export { Agent };
export { Run };
export { RunError };
export { complete };
export { decide };
export { transcribe };
export { ProviderError };
export { AsrStream };
export { startAsrStream };
declare const native: typeof import("./native");
/**
 * Structured provider failure, normalized across the atomic APIs.
 */
declare class ProviderError extends Error {
    /** @type {string | undefined} */ code: string | undefined;
    /** @type {string | undefined} */ provider: string | undefined;
    /** @type {string | undefined} */ model: string | undefined;
    /** @type {number | undefined} */ status: number | undefined;
    /** @type {number | undefined} */ retryAfterSecs: number | undefined;
    /** @type {unknown} */ upstream: unknown;
    /** @type {unknown} */ diagnosticMetadata: unknown;
    /** @param {import("./types").ProviderErrorDetails} details */
    constructor(details: import("./types").ProviderErrorDetails);
}
/**
 * One provider-neutral chat completion, without an agent run.
 *
 * @param {import("./types").CompletionOptions} options
 * @returns {Promise<string>}
 */
declare function complete(options: import("./types").CompletionOptions): Promise<string>;
/**
 * Typed boolean/choice/score judgments over one shared state, without an agent
 * run.
 *
 * @param {import("./types").DecisionOptions} options
 * @returns {Promise<import("./types").DecisionResponse>}
 */
declare function decide(options: import("./types").DecisionOptions): Promise<import("./types").DecisionResponse>;
/**
 * One-shot speech recognition over finished audio bytes.
 *
 * @param {Uint8Array} audio
 * @param {import("./types").TranscribeOptions} options
 * @returns {Promise<string>}
 */
declare function transcribe(audio: Uint8Array, options: import("./types").TranscribeOptions): Promise<string>;
/**
 * Realtime ASR session: audio in, events out. `finalize()` asks the provider
 * to finalize what it has heard and keeps the session open; `finish()` closes
 * the input (idempotent); `wait()` resolves when the stream ends and rethrows
 * the first callback exception or a fatal provider error.
 */
declare class AsrStream {
    #private;
    /**
     * @param {import("./native").NativeAsrStream} session
     * @param {unknown[]} callbackError
     */
    constructor(session: import("./native").NativeAsrStream, callbackError: unknown[]);
    /** @param {Uint8Array} audio @returns {Promise<void>} */
    sendAudio(audio: Uint8Array): Promise<void>;
    /**
     * Ask the provider to finalize what it has heard so far, keeping the session
     * open. Resolving only means the request was submitted: the provider answers
     * through `onEvent` with a `turn` (after any `stable`), or with a non-fatal
     * `error` whose `operation` is `"finalize"`, after which the stream stays
     * usable. Rejects with a `ProviderError` whose code is `unsupported_option`
     * when the model does not support finalize (nothing is sent), and rejects
     * when the session is already finishing or closed.
     *
     * @returns {Promise<void>}
     */
    finalize(): Promise<void>;
    /** @returns {void} */
    finish(): void;
    /** @returns {Promise<void>} */
    wait(): Promise<void>;
}
/**
 * Open a realtime ASR session. Resolves once the provider acknowledges the
 * start; `onEvent` must be synchronous and return void.
 *
 * @param {import("./types").AsrStreamOptions} options
 * @param {(event: import("./types").AsrStreamEvent) => void} onEvent
 * @returns {Promise<AsrStream>}
 */
declare function startAsrStream(options: import("./types").AsrStreamOptions, onEvent: (event: import("./types").AsrStreamEvent) => void): Promise<AsrStream>;
/** A lifecycle error with a stable SDK code. */
declare class RunError extends Error {
    code: string;
    /** @param {string} code @param {string} message */
    constructor(code: string, message: string);
}
/** A running agent and its single, pull-based event iterator. */
declare class Run {
    #private;
    /** @param {import("./native").NativeRun} run */
    constructor(run: import("./native").NativeRun);
    [Symbol.asyncIterator](): this;
    /** @returns {Promise<IteratorResult<import("./types").RuntimeEvent, undefined>>} */
    next(): Promise<IteratorResult<import("./types").RuntimeEvent, undefined>>;
    /** @returns {Promise<IteratorReturnResult<undefined>>} */
    return(): Promise<IteratorReturnResult<undefined>>;
    /** @param {import("./types").CancelReason} [reason] @param {string} [message] */
    cancel(reason?: import("./types").CancelReason, message?: string): boolean;
    /** Return the final root-run event after the iterator reaches EOF. */
    wait(): Promise<{
        type: "run_completed";
        output: unknown;
        stop_reason: import("./types").StopReason;
        run_depth: number;
        child_run_id: string | null;
    } | {
        type: "run_failed";
        error: string;
        kind: import("./types").RunFailureKind;
        run_depth: number;
        child_run_id: string | null;
    } | {
        type: "run_aborted";
        reason: string | null;
        cancel_reason: import("./types").CancelReason | null;
        run_depth: number;
        child_run_id: string | null;
    }>;
    /** Release the event receiver and wait for active work to finish. */
    close(): Promise<void>;
}
declare class Agent extends native.Agent {
    /**
     * Start a run for asynchronous, pull-based event consumption.
     * @param {string} input
     * @param {import("./types").HistoryMessage[]} [messages]
     */
    startRun(input: string, messages?: import("./types").HistoryMessage[]): Promise<Run>;
}
