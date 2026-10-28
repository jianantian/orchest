"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const native = require("../../orchest_node.node");
const realStartAsrStream = native._startAsrStream;

function loadSdkWithStart(start) {
  native._startAsrStream = start;
  const entry = require.resolve("../index.js");
  delete require.cache[entry];
  return require(entry);
}

function fakeSession(finalizeError) {
  return {
    finished: false,
    finalized: 0,
    chunks: [],
    async sendAudio(audio) {
      this.chunks.push(audio);
    },
    async finalize() {
      if (finalizeError !== undefined) {
        throw finalizeError;
      }
      if (this.finished) {
        throw new Error("ASR session is finishing or closed");
      }
      this.finalized += 1;
    },
    finish() {
      this.finished = true;
    },
    async wait() {},
  };
}

test("exports all atomic APIs without leaking native helpers", () => {
  const sdk = loadSdkWithStart(async () => fakeSession());
  for (const name of ["complete", "transcribe", "startAsrStream", "AsrStream"]) {
    assert.equal(typeof sdk[name], "function");
  }
  assert.equal("_startAsrStream" in sdk, false);
});

test("completion preserves structured model errors", async () => {
  const sdk = loadSdkWithStart(async () => fakeSession());
  const envName = "ORCHEST_TEST_MISSING_COMPLETION_KEY";
  delete process.env[envName];

  await assert.rejects(
    sdk.complete({
      model: "deepseek/deepseek-flash",
      user: "hello",
      apiKeyEnv: envName,
    }),
    (error) =>
      error instanceof sdk.ProviderError &&
      error.code === "missing_api_key" &&
      error.status === undefined &&
      error.retryAfterSecs === undefined,
  );
});

test("transcribe preserves structured protocol errors", async () => {
  const sdk = loadSdkWithStart(async () => fakeSession());

  await assert.rejects(
    sdk.transcribe(Buffer.from("audio"), {
      format: "wav",
      provider: "missing",
      model: "model",
      apiKey: "test-key",
    }),
    (error) =>
      error instanceof sdk.ProviderError &&
      error.code === "no_matching_provider" &&
      error.diagnosticMetadata === undefined,
  );
});

test("wait rethrows the first callback exception and suppresses later callbacks", async () => {
  const session = fakeSession();
  const marker = new Error("callback failed");
  let calls = 0;
  const sdk = loadSdkWithStart(async (_options, callback) => {
    callback({ type: "first" });
    callback({ type: "suppressed" });
    return session;
  });
  const stream = await sdk.startAsrStream(
    { format: "pcm", sampleRate: 16_000 },
    () => {
      calls += 1;
      throw marker;
    },
  );
  assert.equal(session.finished, true);
  assert.equal(calls, 1);
  await assert.rejects(stream.wait(), (error) => error === marker);
});

test("wait rejects a Promise-returning callback", async () => {
  const session = fakeSession();
  const sdk = loadSdkWithStart(async (_options, callback) => {
    callback({ type: "partial" });
    return session;
  });
  const stream = await sdk.startAsrStream(
    { format: "pcm", sampleRate: 16_000 },
    () => Promise.resolve(),
  );
  await assert.rejects(stream.wait(), /synchronous/);
});

test("transcribe rejects a combined provider/model", async () => {
  const sdk = loadSdkWithStart(async () => fakeSession());
  await assert.rejects(
    sdk.transcribe(Buffer.from("audio"), {
      format: "wav",
      provider: "aliyun/qwen-audio-3.0-asr-flash",
      apiKey: "test-key",
    }),
    /pass the model separately/,
  );
  await assert.rejects(
    sdk.transcribe(Buffer.from("audio"), { format: "wav", model: "nova-3", apiKey: "k" }),
    /model requires provider/,
  );
});

test("native startAsrStream rejects a combined provider/model", async () => {
  await assert.rejects(
    realStartAsrStream(
      { format: "pcm", sampleRate: 16_000, provider: "deepgram/nova-3", apiKey: "k" },
      () => {},
    ),
    /pass the model separately/,
  );
});

test("finalize keeps the session open", async () => {
  const session = fakeSession();
  const sdk = loadSdkWithStart(async () => session);
  const stream = await sdk.startAsrStream(
    { format: "pcm", sampleRate: 16_000, provider: "deepgram", model: "nova-3" },
    () => {},
  );
  await stream.sendAudio(Buffer.from("a"));
  await stream.finalize();
  await stream.sendAudio(Buffer.from("b"));
  assert.equal(session.finalized, 1);
  assert.equal(session.chunks.length, 2);
  assert.equal(session.finished, false);
});

test("finalize surfaces unsupported_option as a ProviderError", async () => {
  const sdk = loadSdkWithStart(async () => fakeSession(new Error("unsupported")));
  const stream = await sdk.startAsrStream({ format: "pcm", sampleRate: 16_000 }, () => {});
  await assert.rejects(stream.finalize(), /unsupported/);
});

test("finalize after finish rejects", async () => {
  const sdk = loadSdkWithStart(async () => fakeSession());
  const stream = await sdk.startAsrStream({ format: "pcm", sampleRate: 16_000 }, () => {});
  stream.finish();
  await assert.rejects(stream.finalize(), /finishing or closed/);
});

test("a runtime finalize rejection is a non-fatal error event", async () => {
  const rejection = {
    kind: "error",
    code: "timeout",
    message: "no answer",
    fatal: false,
    operation: "finalize",
  };
  const received = [];
  let emit;
  const session = fakeSession();
  const sdk = loadSdkWithStart(async (_options, callback) => {
    emit = callback;
    return session;
  });
  const stream = await sdk.startAsrStream({ format: "pcm", sampleRate: 16_000 }, (event) => {
    received.push(event);
  });
  await stream.finalize(); // resolves on submission
  emit(rejection);
  await stream.sendAudio(Buffer.from("a")); // still usable
  await stream.wait();
  assert.deepEqual(received, [rejection]);
  assert.equal(session.chunks.length, 1);
});

test("events are delivered with kind", async () => {
  const events = [
    { kind: "draft", text: "he", segment_id: "s0" },
    { kind: "stable", text: "hello", segment_id: "s0", update: "snapshot" },
    { kind: "turn", segment_id: "s0" },
    { kind: "session_closed" },
  ];
  const received = [];
  const sdk = loadSdkWithStart(async (_options, callback) => {
    for (const event of events) {
      callback(event);
    }
    return fakeSession();
  });
  const stream = await sdk.startAsrStream({ format: "pcm", sampleRate: 16_000 }, (event) => {
    received.push(event.kind);
  });
  await stream.wait();
  assert.deepEqual(received, ["draft", "stable", "turn", "session_closed"]);
});
