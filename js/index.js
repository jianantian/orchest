"use strict";

// The addon comes from the platform package or a local `npm run build:native`;
// js/native.d.ts declares its surface.
const native = /** @type {typeof import("./native")} */ (require("./load-native.js").loadNative());
const PROVIDER_ERROR_PREFIX = "__ORCHEST_PROVIDER_ERROR__:";

/**
 * Structured provider failure, normalized across the atomic APIs.
 */
class ProviderError extends Error {
  /** @type {string | undefined} */ code;
  /** @type {string | undefined} */ provider;
  /** @type {string | undefined} */ model;
  /** @type {number | undefined} */ status;
  /** @type {number | undefined} */ retryAfterSecs;
  /** @type {unknown} */ upstream;
  /** @type {unknown} */ diagnosticMetadata;

  /** @param {import("./types").ProviderErrorDetails} details */
  constructor(details) {
    super(details.message);
    this.name = "ProviderError";
    Object.assign(this, details);
  }
}

/**
 * @param {unknown} value
 * @returns {value is PromiseLike<unknown>}
 */
function isThenable(value) {
  return (
    ((typeof value === "object" && value !== null) || typeof value === "function") &&
    "then" in value &&
    typeof value.then === "function"
  );
}

/**
 * Rebuild a `ProviderError` from the prefixed message the native layer throws;
 * pass anything else through untouched.
 *
 * @param {unknown} error
 * @returns {unknown}
 */
function normalizeProviderError(error) {
  const message = error instanceof Error ? error.message : String(error);
  if (!message.startsWith(PROVIDER_ERROR_PREFIX)) {
    return error;
  }
  try {
    return new ProviderError(JSON.parse(message.slice(PROVIDER_ERROR_PREFIX.length)));
  } catch {
    return error;
  }
}

/**
 * One provider-neutral chat completion, without an agent run.
 *
 * @param {import("./types").CompletionOptions} options
 * @returns {Promise<string>}
 */
async function complete(options) {
  try {
    return await native._complete(options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

/**
 * Typed boolean/choice/score judgments over one shared state, without an agent
 * run.
 *
 * @param {import("./types").DecisionOptions} options
 * @returns {Promise<import("./types").DecisionResponse>}
 */
async function decide(options) {
  try {
    return await native._decide(options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

/**
 * One-shot speech recognition over finished audio bytes.
 *
 * @param {Uint8Array} audio
 * @param {import("./types").TranscribeOptions} options
 * @returns {Promise<string>}
 */
async function transcribe(audio, options) {
  try {
    return await native._transcribe(audio, options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

/**
 * Realtime ASR session: audio in, events out. `finalize()` asks the provider
 * to finalize what it has heard and keeps the session open; `finish()` closes
 * the input (idempotent); `wait()` resolves when the stream ends and rethrows
 * the first callback exception or a fatal provider error.
 */
class AsrStream {
  /** @type {import("./native").NativeAsrStream} */ #session;
  /** @type {unknown[]} */ #callbackError;

  /**
   * @param {import("./native").NativeAsrStream} session
   * @param {unknown[]} callbackError
   */
  constructor(session, callbackError) {
    this.#session = session;
    this.#callbackError = callbackError;
  }

  /** @param {Uint8Array} audio @returns {Promise<void>} */
  async sendAudio(audio) {
    await this.#session.sendAudio(audio);
  }

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
  async finalize() {
    try {
      await this.#session.finalize();
    } catch (error) {
      throw normalizeProviderError(error);
    }
  }

  /** @returns {void} */
  finish() {
    this.#session.finish();
  }

  /** @returns {Promise<void>} */
  async wait() {
    let nativeError;
    try {
      await this.#session.wait();
    } catch (error) {
      nativeError = error;
    }
    if (this.#callbackError.length > 0) {
      throw this.#callbackError[0];
    }
    if (nativeError !== undefined) {
      throw normalizeProviderError(nativeError);
    }
  }
}

/**
 * Open a realtime ASR session. Resolves once the provider acknowledges the
 * start; `onEvent` must be synchronous and return void.
 *
 * @param {import("./types").AsrStreamOptions} options
 * @param {(event: import("./types").AsrStreamEvent) => void} onEvent
 * @returns {Promise<AsrStream>}
 */
async function startAsrStream(options, onEvent) {
  /** @type {unknown[]} */
  const callbackError = [];
  /** @type {import("./native").NativeAsrStream | undefined} */
  let session;
  /** @param {import("./types").AsrStreamEvent} event */
  const guardedOnEvent = (event) => {
    if (callbackError.length > 0) {
      return;
    }
    try {
      /** @type {unknown} */
      const result = onEvent(event);
      if (isThenable(result)) {
        Promise.resolve(result).catch(() => {});
        throw new TypeError("onEvent must be synchronous and return void");
      }
    } catch (error) {
      callbackError.push(error);
      if (session !== undefined) {
        session.finish();
      }
    }
  };
  try {
    session = await native._startAsrStream(options, guardedOnEvent);
  } catch (error) {
    throw normalizeProviderError(error);
  }
  if (callbackError.length > 0) {
    session.finish();
  }
  return new AsrStream(session, callbackError);
}

/** A lifecycle error with a stable SDK code. */
class RunError extends Error {
  /** @param {string} code @param {string} message */
  constructor(code, message) {
    super(message);
    this.name = "RunError";
    this.code = code;
  }
}

/** @param {unknown} error */
function normalizeRunError(error) {
  const message = error instanceof Error ? error.message : String(error);
  const match = /^__ORCHEST_RUN_ERROR__:([^:]+):([\s\S]*)$/.exec(message);
  return match === null ? error : new RunError(match[1], match[2]);
}

/** A running agent and its single, pull-based event iterator. */
class Run {
  /** @type {import("./native").NativeRun} */ #native;
  #readQueue = Promise.resolve();

  /** @param {import("./native").NativeRun} run */
  constructor(run) {
    this.#native = run;
  }

  [Symbol.asyncIterator]() {
    return this;
  }

  /** @returns {Promise<IteratorResult<import("./types").RuntimeEvent, undefined>>} */
  async next() {
    const pending = this.#readQueue.then(() => this.#native.nextEvent());
    // Submit one native read at a time, even if the caller issues multiple pulls.
    this.#readQueue = pending.then(() => {}, () => {});
    const event = await pending;
    return event == null
      ? { done: true, value: undefined }
      : { done: false, value: event };
  }

  /** @returns {Promise<IteratorReturnResult<undefined>>} */
  async return() {
    await this.close();
    return { done: true, value: undefined };
  }

  /** @param {import("./types").CancelReason} [reason] @param {string} [message] */
  cancel(reason, message) {
    return this.#native.cancel(reason, message);
  }

  /**
   * Answer a pending request on this run.
   * @param {string} requestId
   * @param {import("./types").RequestResponse} payload
   */
  async respond(requestId, payload) {
    try {
      await this.#native.respond(requestId, payload);
    } catch (error) {
      throw normalizeRunError(error);
    }
  }

  capabilities() {
    return this.#native.capabilities();
  }

  /** Return the final root-run event after the iterator reaches EOF. */
  async wait() {
    try {
      return await this.#native.wait();
    } catch (error) {
      throw normalizeRunError(error);
    }
  }

  /** Release the event receiver and wait for active work to finish. */
  async close() {
    await this.#native.close();
  }
}

class Agent extends native.Agent {
  /**
   * Start a run for asynchronous, pull-based event consumption.
   * @param {string} input
   * @param {import("./types").HistoryMessage[]} [messages]
   * @param {import("./types").SessionOptions} [session]
   */
  async startRun(input, messages, session) {
    try {
      return new Run(await this._startRun(input, messages, session));
    } catch (error) {
      throw normalizeRunError(error);
    }
  }

  /**
   * Resume a local session, optionally appending a new user turn.
   * @param {import("./types").SessionOptions} session
   * @param {string} [input]
   */
  async resumeSession(session, input) {
    try {
      return new Run(await this._resumeSession(session, input));
    } catch (error) {
      throw normalizeRunError(error);
    }
  }

  /** @type {import("./native").Agent["runSync"]} */
  async runSync(input, messages) {
    try {
      return await super.runSync(input, messages);
    } catch (error) {
      throw normalizeRunError(error);
    }
  }

  /** @type {import("./native").Agent["runStream"]} */
  runStream(input, onEvent, messages) {
    try {
      super.runStream(input, onEvent, messages);
    } catch (error) {
      throw normalizeRunError(error);
    }
  }
}

// Explicit `exports.X = ...` assignments: cjs-module-lexer only detects these
// for ESM consumers, so `import { decide } from "orchest-sdk"` works.
exports.Agent = Agent;
exports.Run = Run;
exports.RunError = RunError;
exports.complete = complete;
exports.decide = decide;
exports.transcribe = transcribe;
exports.ProviderError = ProviderError;
exports.AsrStream = AsrStream;
exports.startAsrStream = startAsrStream;
