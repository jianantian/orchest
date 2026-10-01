"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");

const native = require("../../orchest_node.node");

function loadSdkWithStart(start) {
  native._startAsrStream = start;
  const entry = require.resolve("../index.js");
  delete require.cache[entry];
  return require(entry);
}

function fakeSession() {
  return {
    finished: false,
    async sendAudio() {},
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
      provider: "missing/model",
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
