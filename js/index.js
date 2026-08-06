"use strict";

const native = require("../orchest_node.node");

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
      throw nativeError;
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
  session = await native._startAsrStream(options, guardedOnEvent);
  if (callbackError.length > 0) {
    session.finish();
  }
  return new AsrStream(session, callbackError);
}

module.exports = {
  Agent: native.Agent,
  complete: native.complete,
  transcribe: native.transcribe,
  AsrStream,
  startAsrStream,
};
