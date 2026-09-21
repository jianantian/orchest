"use strict";

const native = require("../orchest_node.node");
const PROVIDER_ERROR_PREFIX = "__ORCHEST_PROVIDER_ERROR__:";

class ProviderError extends Error {
  constructor(details) {
    super(details.message);
    this.name = "ProviderError";
    Object.assign(this, details);
  }
}

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

async function complete(options) {
  try {
    return await native._complete(options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

async function decide(options) {
  try {
    return await native._decide(options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

async function transcribe(audio, options) {
  try {
    return await native._transcribe(audio, options);
  } catch (error) {
    throw normalizeProviderError(error);
  }
}

class AsrStream {
  constructor(session, callbackError) {
    this._session = session;
    this._callbackError = callbackError;
  }

  async sendAudio(audio) {
    await this._session.sendAudio(audio);
  }

  finish() {
    this._session.finish();
  }

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

async function startAsrStream(options, onEvent) {
  const callbackError = [];
  let session;
  const guardedOnEvent = (event) => {
    if (callbackError.length > 0) {
      return;
    }
    try {
      const result = onEvent(event);
      if (result !== null && result !== undefined && typeof result.then === "function") {
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
exports.Agent = native.Agent;
exports.complete = complete;
exports.decide = decide;
exports.transcribe = transcribe;
exports.ProviderError = ProviderError;
exports.AsrStream = AsrStream;
exports.startAsrStream = startAsrStream;
