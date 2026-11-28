"use strict";

const assert = require("node:assert/strict");
const http = require("node:http");
const { once } = require("node:events");
const { existsSync, readFileSync } = require("node:fs");
const { join } = require("node:path");
const test = require("node:test");

const { Agent, RunError } = require("../index.js");


const copiedFixture = join(__dirname, "sdk-runtime-events.json");
const fixture = JSON.parse(readFileSync(existsSync(copiedFixture) ? copiedFixture
  : join(__dirname, "../../crates/orchest/tests/fixtures/sdk-runtime-events.json"), "utf8"));

function normalize(event) {
  const normalized = { ...event };
  for (const key of ["run_id", "message_id"]) {
    if (typeof normalized[key] === "string") normalized[key] = `<${key}>`;
  }
  return normalized;
}

test("async run streams without blocking Node and shares agent occupancy", { timeout: 10_000 }, async () => {
  let releaseModel;
  const gate = new Promise((resolve) => { releaseModel = resolve; });
  const server = http.createServer(async (request, response) => {
    const parts = [];
    for await (const part of request) parts.push(part);
    const body = JSON.parse(Buffer.concat(parts).toString());
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.flushHeaders();
    await gate;
    const asksForTool = body.tools?.length && !body.messages.some(
      message => message.content.some(block => block.type === "tool_result"),
    );
    for (const [event, data] of fixture[asksForTool ? "tool" : "complete"]) {
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
      const events = [];
      for await (const event of next) {
        events.push(normalize(event));
        if (event.type === "run_completed") {
          await assert.rejects(next.wait(), isPending);
        }
      }
      assert.equal((await next.wait()).type, "run_completed");
      assert.deepEqual(events, fixture.expected);
    } finally {
      await next.close();
    }
    let calls = 0;
    const agents = [makeAgent(), makeAgent()];
    for (const agent of agents) {
      agent.registerToolWithHandler("probe", "fixture tool", { type: "object" }, () => {
        calls++;
        return "ok";
      }, { approval: "always" });
    }

    const requestFor = async (run) => {
      // Manual pulls leave the run open while the request is pending.
      for (;;) {
        const { value, done } = await run.next();
        assert.equal(done, false);
        if (value.type === "approval_requested") return value.request_id;
      }
    };

    const [left, right] = await Promise.all(agents.map(agent => agent.startRun("approval")));
    try {
      const [leftId, rightId] = await Promise.all([requestFor(left), requestFor(right)]);
      assert.deepEqual(left.capabilities(), agents[0].capabilities());
      await assert.rejects(left.respond(rightId, { decision: "approve" }), { code: "request_not_found" });
      await assert.rejects(left.respond(leftId, { decision: "bad" }), { code: "invalid_response" });
      await left.respond(leftId, { decision: "approve" });
      await assert.rejects(left.respond(leftId, { decision: "approve" }), { code: "request_not_found" });
      await right.respond(rightId, { decision: "deny" });
      for (const run of [left, right]) {
        const events = [];
        for await (const event of run) events.push(event);
        assert.equal(events.at(-1).type, "run_completed");
      }
      assert.equal(calls, 1);
    } finally {
      await Promise.all([left.close(), right.close()]);
    }

    const waiting = await agents[1].startRun("cancel approval");
    try {
      await requestFor(waiting);
      assert.equal(waiting.cancel("timeout"), true);
      assert.equal(waiting.cancel("timeout"), false);
      const events = [];
      for await (const event of waiting) events.push(event);
      assert.equal(events.at(-1).cancel_reason, "timeout");
    } finally {
      await waiting.close();
    }

    assert.equal((await first.runSync("legacy")).at(-1).type, "run_completed");
  } finally {
    releaseModel();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});
