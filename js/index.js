"use strict";

// The addon is a build artifact (`npm run build:native`); js/native.d.ts
// declares it, so the require below fails resolution by design.
/** @type {typeof import("./native")} */
// @ts-expect-error - resolved at runtime, not through TypeScript module resolution
const native = require("../orchest_node.node");
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
 * Realtime ASR session: audio in, events out. `finish()` closes the input
 * (idempotent); `wait()` resolves when the stream ends and rethrows the first
 * callback exception or a fatal provider error.
 */
class AsrStream {
  /**
   * @param {import("./native").NativeAsrStream} session
   * @param {unknown[]} callbackError
   */
  constructor(session, callbackError) {
    this._session = session;
    this._callbackError = callbackError;
  }

  /** @param {Uint8Array} audio @returns {Promise<void>} */
  async sendAudio(audio) {
    await this._session.sendAudio(audio);
  }

  /** @returns {void} */
  finish() {
    this._session.finish();
  }

  /** @returns {Promise<void>} */
  async wait() {
    let nativeError;
    try {
      await this._session.wait();
    } catch (error) {
      nativeError = error;
    }
    if (this._callbackError.length > 0) {
      throw this._callbackError[0];
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

// Explicit `exports.X = ...` assignments: cjs-module-lexer only detects these
// for ESM consumers, so `import { decide } from "@orchest/sdk"` works.
/** @type {typeof import("./native").Agent} */
exports.Agent = native.Agent;
exports.complete = complete;
exports.decide = decide;
exports.transcribe = transcribe;
exports.ProviderError = ProviderError;
exports.AsrStream = AsrStream;
exports.startAsrStream = startAsrStream;
