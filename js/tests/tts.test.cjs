"use strict";

const assert = require("node:assert/strict");
const { spawn } = require("node:child_process");
const { existsSync } = require("node:fs");
const path = require("node:path");
const readline = require("node:readline");
const { test } = require("node:test");
const { ProviderError, startTtsDuplex, streamSynthesize, synthesize } = require("../index.js");

test("TTS through the installed native package", { timeout: 25000 }, async () => {
  const local = path.join(__dirname, "sdk-tts-server.py");
  const fixture = existsSync(local) ? local : path.join(__dirname, "../../crates/orchest/tests/fixtures/sdk-tts-server.py");
  const child = spawn(process.env.PYTHON || "python3", [fixture], { stdio: ["ignore", "pipe", "inherit"] });
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const waitClosed = async () => assert.equal((await lines.next()).value, "closed");
  try {
    const { endpoint, ca } = JSON.parse((await lines.next()).value);
    const options = {
      provider: "minimax", voice: "fixture", format: "mp3", apiKey: "fixture-key",
      apiUrl: endpoint + "/one-shot", providerOptions: { ca_cert_pem: ca },
    };
    const bytes = [0, 97, 255];
    const result = await synthesize("Hello.", options);
    assert(result.audio instanceof Uint8Array);
    assert.deepEqual([...result.audio], bytes);
    assert.equal(result.format, "mp3");
    assert.equal(result.usage.billed_characters, 6);
    assert.equal(result.usage.audio_duration_ms.value, 125);
    await waitClosed();

    const stream = await streamSynthesize("Hello.", options);
    const events = [];
    for await (const event of stream) events.push(event);
    assert.deepEqual(events.map(event => event.kind), ["audio", "done"]);
    assert.deepEqual([...events[0].audio], bytes);
    assert.equal(events[0].context_id, null);
    assert.equal((await stream.usage()).outcome, "completed");
    await waitClosed();

    options.apiUrl = endpoint + "/duplex";
    const duplex = await startTtsDuplex(options);
    const eventsRead = duplex[Symbol.asyncIterator]();
    const expect = async (kind, fields = {}) => {
      const { value } = await eventsRead.next();
      assert.equal(value.kind, kind);
      for (const [key, expected] of Object.entries(fields)) assert.equal(value[key], expected);
      return value;
    };
    const first = expect("audio", { context_id: 1, flush_id: null });
    await duplex.append("First.");
    assert.deepEqual([...(await first).audio], bytes);
    await duplex.flush();
    await expect("flush_accepted", { context_id: 1, flush_id: 1 });
    await expect("flushed", { context_id: 1, flush_id: 1 });
    await duplex.append("Cancel.");
    await expect("audio", { context_id: 2 });
    const cancelling = performance.now();
    await duplex.cancel();
    await expect("cancelled", { context_id: 2 });
    const cancelMs = performance.now() - cancelling;
    await duplex.append("Third.");
    await expect("audio", { context_id: 3 });
    const closing = performance.now();
    await duplex.close();
    await duplex.close();
    await expect("context_ended", { context_id: 3 });
    await expect("session_closed");
    assert.equal((await eventsRead.next()).done, true);
    assert.equal((await duplex.usage()).input_characters, 19);
    await waitClosed();

    const closeMs = performance.now() - closing;

    await assert.rejects(startTtsDuplex({
      ...options, options: { require_capabilities: ["words"] },
    }), error => error instanceof ProviderError && error.code === "unsupported_option");

    await assert.rejects(async () => {
      const fatal = await startTtsDuplex(options);
      await fatal.append("error");
      for await (const event of fatal) assert.fail("unexpected event " + event.kind);
    }, error => error instanceof ProviderError && error.code === "invalid_api_key");
    await waitClosed();

    let aborting;
    await assert.rejects(async () => {
      const early = await startTtsDuplex(options);
      await early.append("Abort.");
      for await (const event of early) {
        if (event.kind === "audio") {
          aborting = performance.now();
          throw new Error("handler");
        }
      }
    }, /handler/);
    await waitClosed();
    if (process.env.ORCHEST_TTS_BENCHMARK) {
      console.log(`Node cancel_ms=${cancelMs.toFixed(2)} close_ms=${closeMs.toFixed(2)} abort_ms=${(performance.now() - aborting).toFixed(2)}`);
    }
  } finally {
    child.kill();
  }
});

test("TTS fixture performance", {
  skip: !process.env.ORCHEST_TTS_BENCHMARK, timeout: 25000,
}, async () => {
  const local = path.join(__dirname, "sdk-tts-server.py");
  const fixture = existsSync(local) ? local : path.join(__dirname, "../../crates/orchest/tests/fixtures/sdk-tts-server.py");
  const child = spawn(process.env.PYTHON || "python3", [fixture], { stdio: ["ignore", "pipe", "inherit"] });
  try {
    const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    const { endpoint, ca } = JSON.parse((await lines.next()).value);
    const options = {
      provider: "minimax", voice: "fixture", format: "mp3", apiKey: "fixture-key",
      apiUrl: endpoint + "/one-shot", providerOptions: { ca_cert_pem: ca },
    };
    for (const pause of [0, 100, 0, 100, 0, 100]) {
      const start = performance.now();
      let first;
      let total = 0;
      for await (const event of await streamSynthesize("benchmark", options)) {
        if (event.kind !== "audio") continue;
        if (first === undefined) {
          first = performance.now() - start;
          await new Promise(resolve => setTimeout(resolve, pause));
        }
        total += event.audio.length;
      }
      assert.equal(total, 128 * 4096);
      console.log(`Node pause_ms=${pause} first_ms=${first.toFixed(2)} total_ms=${(performance.now() - start).toFixed(2)}`);
    }
  } finally {
    child.kill();
  }
});
