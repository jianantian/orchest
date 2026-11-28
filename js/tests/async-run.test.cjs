"use strict";

const assert = require("node:assert/strict");
const http = require("node:http");
const { once } = require("node:events");
const test = require("node:test");

const { Agent, RunError } = require("../index.js");

test("async run streams without blocking Node and shares agent occupancy", { timeout: 10_000 }, async () => {
  let releaseModel;
  const gate = new Promise((resolve) => { releaseModel = resolve; });
  const server = http.createServer(async (request, response) => {
    request.resume();
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.flushHeaders();
    await gate;
    for (const [event, data] of [
      ["message_start", { message: { usage: { input_tokens: 1 } } }],
      ["content_block_start", { content_block: { type: "text" } }],
      ["content_block_delta", { delta: { type: "text_delta", text: "done" } }],
      ["content_block_stop", {}],
      ["message_delta", { delta: { stop_reason: "end_turn" }, usage: { output_tokens: 1 } }],
      ["message_stop", {}],
    ]) {
      response.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
    }
    response.end();
  }).listen(0, "127.0.0.1");
  await once(server, "listening");

  const makeAgent = () => new Agent({
    name: "async-run-test",
    model: "anthropic/claude-sonnet-4-6",
    systemPrompt: "Answer briefly.",
    apiKey: "local-test",
    apiUrl: `http://127.0.0.1:${server.address().port}`,
  });
  const first = makeAgent();
  const isPending = (error) => error instanceof RunError && error.code === "run_events_pending";
  const isActive = (error) => error instanceof RunError && error.code === "run_active";

  try {
    await assert.rejects(first.startRun("invalid", [{}]), /invalid .*messages.* entry/);
    const run = await first.startRun("one");
    try {
      await assert.rejects(run.wait(), isPending);
      assert.equal((await run.next()).value.type, "run_started");
      await new Promise((resolve) => setTimeout(resolve, 0));
      await assert.rejects(first.startRun("two"), isActive);
      await assert.rejects(first.runSync("two"), isActive);
      assert.throws(() => first.runStream("two", () => {}), isActive);

      const other = await makeAgent().startRun("independent");
      for await (const event of other) {
        assert.equal(event.type, "run_started");
        break;
      }
      await other.close();

      assert.equal(first.abort("timeout"), true);
      assert.equal(first.abort("timeout"), false);
      const events = [];
      for await (const event of run) {
        events.push(event);
      }
      assert.equal(events.at(-1).type, "run_aborted");
      assert.equal(events.at(-1).cancel_reason, "timeout");
      assert.deepEqual(await run.wait(), events.at(-1));
    } finally {
      await run.close();
    }

    releaseModel();
    const next = await first.startRun("again");
    try {
      for await (const event of next) {
        if (event.type === "run_completed") {
          await assert.rejects(next.wait(), isPending);
        }
      }
      assert.equal((await next.wait()).type, "run_completed");
    } finally {
      await next.close();
    }
    assert.equal((await first.runSync("legacy")).at(-1).type, "run_completed");
  } finally {
    releaseModel();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});
