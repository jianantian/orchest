"use strict";

const assert = require("node:assert/strict");
const { spawnSync } = require("node:child_process");
const http = require("node:http");
const { once } = require("node:events");
const { existsSync, mkdtempSync, readFileSync, renameSync, rmSync } = require("node:fs");
const { tmpdir } = require("node:os");
const { join } = require("node:path");
const test = require("node:test");

const { Agent, EvidenceStore, RunError } = require("../index.js");


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
  let evidenceAttempts = 0;
  const gate = new Promise((resolve) => { releaseModel = resolve; });
  const server = http.createServer(async (request, response) => {
    const parts = [];
    for await (const part of request) parts.push(part);
    const body = JSON.parse(Buffer.concat(parts).toString());
    if (body.messages.at(-1).content[0].text === "retry evidence" && ++evidenceAttempts === 1) {
      response.writeHead(500).end();
      return;
    }
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

  const makeAgent = (retry = false) => new Agent({
    retry,
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
    const directory = mkdtempSync(join(tmpdir(), "orchest-session-"));
    const session = { sqlitePath: join(directory, "session.sqlite"), sessionId: "shared" };
    const drain = async (run) => {
      try {
        let runId;
        for await (const event of run) {
          if (event.type === "run_started") runId = event.run_id;
          if (event.type === "run_failed") assert.fail(event.error);
        }
        assert.equal((await run.wait()).type, "run_completed");
        return runId;
      } finally {
        await run.close();
      }
    };
    try {
      await assert.rejects(first.resumeSession(), { code: "session_not_configured" });
      await assert.rejects(first.resumeSession(session), { code: "session_not_found" });
      const activeAgent = makeAgent();
      activeAgent.registerToolWithHandler("probe", "fixture", { type: "object" }, () => "ok", { approval: "always" });
      const stored = await activeAgent.startRun("stored", undefined, session);
      try {
        const id = await requestFor(stored);
        assert.equal(stored.capabilities().resume, true);
        await assert.rejects(first.resumeSession(session), { code: "session_writer_conflict" });
        await assert.rejects(first.startRun("duplicate", undefined, session), { code: "session_writer_conflict" });
        await drain(await first.startRun("parallel", undefined, { ...session, sessionId: "parallel" }));
        await stored.respond(id, { decision: "approve" });
        await drain(stored);
      } finally {
        await stored.close();
      }
      await assert.rejects(first.startRun("existing", undefined, session), { code: "session_already_exists" });
      await drain(await makeAgent().resumeSession(session));
      await drain(await makeAgent().resumeSession(session, "follow-up"));

      const path = join(directory, "evidence.sqlite");
      const store = await EvidenceStore.open(path);
      const evidenceSession = { ...session, sessionId: "evidence" };
      await drain(await makeAgent().startRun("default off", undefined, evidenceSession));
      const authorize = ({ sqlitePath, sessionId }) => store.authorize(sqlitePath, sessionId);
      const reader = await authorize(evidenceSession);
      assert.deepEqual((await reader.read()).records, []);
      const enabled = { ...evidenceSession, evidence: store };
      await assert.rejects(
        makeAgent().startRun("shared database", undefined, { ...enabled, sqlitePath: path }),
        { code: "session_storage_error" },
      );
      const capturedId = await drain(await makeAgent(true).resumeSession(enabled, "retry evidence"));
      await drain(await makeAgent().resumeSession(evidenceSession, "off again"));
      const cursor = (await reader.read(undefined, 1)).next_cursor;
      assert.ok(cursor);
      await assert.rejects(reader.read(undefined, 65), { code: "evidence_invalid_page_size" });
      await assert.rejects(reader.read(cursor + "x"), { code: "evidence_invalid_cursor" });

      const other = { ...evidenceSession, sqlitePath: join(directory, "other.sqlite") };
      await drain(await makeAgent().startRun("different store", undefined, other));
      const otherReader = await authorize(other);
      await assert.rejects(otherReader.read(cursor), { code: "evidence_invalid_cursor" });
      assert.deepEqual((await otherReader.read()).records, []);

      const copiedScript = join(__dirname, "sdk-session-evidence.py");
      const script = existsSync(copiedScript) ? copiedScript
        : join(__dirname, "../../crates/orchest/tests/fixtures/sdk-session-evidence.py");
      const changeFixture = (...args) => {
        const result = spawnSync("python3", [script, ...args], { encoding: "utf8" });
        assert.equal(result.status, 0, result.stderr || result.error?.message);
      };
      changeFixture("seed", path);

      const rows = [];
      let next = null;
      let emptyWithCursor = false;
      let invalid = 0;
      do {
        const page = await reader.read(next, 1);
        rows.push(...page.records);
        invalid += page.invalid_records;
        if (page.next_cursor !== null) {
          assert.notEqual(page.next_cursor, next);
          emptyWithCursor ||= page.records.length === 0;
        }
        next = page.next_cursor;
      } while (next !== null);
      assert.ok(emptyWithCursor);
      assert.equal(invalid, 1);
      const starts = rows.filter(row => row.kind === "attempt_started");
      const finishes = rows.filter(row => row.kind === "attempt_finished");
      assert.equal(new Set(starts.map(row => row.attempt_id)).size, 3);
      assert.equal(new Set(finishes.map(row => row.attempt_id)).size, 2);
      assert.deepEqual(new Set(finishes.map(row => row.outcome)), new Set(["returned", "adapter_error"]));
      assert.deepEqual(new Set(finishes.map(row => row.run_id)), new Set([capturedId]));
      const concluded = starts.filter(row => finishes.some(finish => finish.attempt_id === row.attempt_id));
      assert.equal(concluded.length, 2);
      assert.equal(concluded[0].step, concluded[1].step);
      assert.deepEqual(starts.filter(row => !concluded.includes(row)).map(row => row.ordinal), [99]);
      for (const row of starts) {
        assert.deepEqual(Object.keys(row).sort(), ["attempt_id", "kind", "model", "ordinal", "provider", "purpose", "run_id", "step"]);
      }
      for (const row of finishes) {
        assert.deepEqual(Object.keys(row).sort(), ["attempt_id", "kind", "outcome", "run_id", "step"]);
      }

      changeFixture("delete", evidenceSession.sqlitePath, evidenceSession.sessionId);
      await assert.rejects(reader.read(), { code: "evidence_session_unavailable" });
      await drain(await makeAgent().startRun("replacement", undefined, enabled));
      await assert.rejects(reader.read(), { code: "evidence_session_unavailable" });
      const replacement = await authorize(evidenceSession);
      assert.equal((await replacement.read()).records.length, 2);
      await replacement.revoke();
      await assert.rejects(replacement.read(), { code: "evidence_session_unavailable" });
      await assert.rejects(authorize(evidenceSession), { code: "evidence_session_unavailable" });

      const replacedSession = { ...enabled, sqlitePath: join(directory, "replaceable.sqlite") };
      await drain(await makeAgent().startRun("before file replacement", undefined, replacedSession));
      const oldReader = await authorize(replacedSession);
      assert.equal((await oldReader.read()).records.length, 2);
      renameSync(replacedSession.sqlitePath, join(directory, "previous.sqlite"));
      await assert.rejects(oldReader.read(), { code: "evidence_session_unavailable" });
      await drain(await makeAgent().startRun("after file replacement", undefined, replacedSession));
      await assert.rejects(oldReader.read(), { code: "evidence_session_unavailable" });
      const newReader = await authorize(replacedSession);
      assert.equal((await newReader.read()).records.length, 2);

      await drain(await makeAgent().resumeSession({ ...other, evidence: store }, "before evidence fault"));
      changeFixture("break", path);
      await assert.rejects(otherReader.read(), { code: "evidence_storage_error" });
      await drain(await makeAgent().resumeSession({ ...other, evidence: store }, "after evidence fault"));
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  } finally {
    releaseModel();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
});

test("shared Node tool registrations use the Rust schema, results, and approvals", { timeout: 10_000 }, async () => {
  const directory = mkdtempSync(join(tmpdir(), "orchest-tools-"));
  const inputs = new Map();
  const requests = [];
  const server = http.createServer(async (request, response) => {
    const parts = [];
    for await (const part of request) parts.push(part);
    const body = JSON.parse(Buffer.concat(parts).toString());
    requests.push(body);
    const toolResult = body.messages.some(
      message => message.content.some(block => block.type === "tool_result"),
    );
    const name = body.tools?.[0]?.name;
    const input = inputs.get(name);
    response.writeHead(200, { "content-type": "text/event-stream" });
    for (const [event, data] of name && !toolResult ? fixture.tool : fixture.complete) {
      let payload = data;
      if (name && !toolResult && event === "content_block_start") {
        payload = { content_block: { ...data.content_block, name } };
      }
      if (name && !toolResult && event === "content_block_delta") {
        payload = { delta: { ...data.delta, partial_json: JSON.stringify(input ?? {}) } };
      }
      response.write(`event: ${event}\ndata: ${JSON.stringify(payload)}\n\n`);
    }
    response.end();
  }).listen(0, "127.0.0.1");
  await once(server, "listening");

  const makeAgent = (name) => new Agent({
    name,
    model: "anthropic/claude-sonnet-4-6",
    systemPrompt: `Instructions for ${name}`,
    apiKey: "local-test",
    apiUrl: `http://127.0.0.1:${server.address().port}`,
  });
  const runAgent = async (agent, approval = "deny") => {
    const run = await agent.startRun("run the fixture");
    try {
      const events = [];
      for await (const event of run) {
        events.push(event);
        if (event.type === "approval_requested") {
          await run.respond(event.request_id, { decision: approval });
        }
      }
      assert.equal((await run.wait()).type, "run_completed");
      return events;
    } finally {
      await run.close();
    }
  };

  try {
    for (const inputKey of [undefined, "task"]) {
      const parent = makeAgent("parent");
      const key = inputKey ?? "question";
      parent.registerAgentTool("delegate", "Delegate to a configured child", makeAgent("child"), inputKey);
      inputs.set("delegate", { [key]: `child input for ${key}` });
      const events = await runAgent(parent);
      const forwarded = events.filter(event => event.type === "sub_agent_event").map(event => event.event);
      assert.equal(forwarded[0].type, "run_started");
      assert.equal(forwarded.at(-1).type, "run_completed");
      assert.equal(forwarded.at(-1).output, "done");
      const toolResult = events.find(event => event.type === "tool_call_completed" && event.tool === "delegate");
      assert.equal(toolResult?.output.output, "done");
      assert.equal(typeof toolResult.output.child_run_id, "string");
      assert.equal(typeof toolResult.output.budget_used.tokens_used, "number");
      assert.deepEqual(requests.at(-3).tools[0].input_schema.required, [key]);
      assert.equal(requests.at(-2).messages.at(-1).content[0].text, `child input for ${key}`);
    }

    const writer = makeAgent("writer");
    writer.registerWriteFileTool();
    for (const approval of ["approve", "deny"]) {
      const output = join(directory, `${approval}.txt`);
      inputs.set("write_file", { path: output, content: "fixture output" });
      const events = await runAgent(writer, approval);
      assert.deepEqual(requests.at(-2).tools[0].input_schema.required, ["path", "content"]);
      assert(events.some(event => event.type === "approval_requested" && event.tool_call.name === "write_file"));
      if (approval === "approve") {
        assert.equal(readFileSync(output, "utf8"), "fixture output");
        const dispatch = events.find(event => event.type === "tool_call_started" && event.tool === "write_file");
        assert.deepEqual([...dispatch.metadata.effects].sort(), ["write_outside", "write_workspace"]);
      } else {
        assert.equal(existsSync(output), false);
        assert(events.some(event => event.type === "approval_denied"));
      }
    }

    const schemaOnly = makeAgent("schema only");
    schemaOnly.registerTool({ name: "probe", description: "fixture", inputSchema: { type: "object" } });
    assert((await runAgent(schemaOnly)).some(event => event.type === "tool_call_failed"));

    const polling = makeAgent("polling");
    polling.registerAsyncToolWithHandler(
      "probe", "fixture", { type: "object" },
      () => ({ job_id: "fixture-job", poll_interval_ms: 1 }),
      (jobId) => ({ status: "completed", result: { jobId } }),
    );
    const polled = await runAgent(polling);
    assert(polled.some(event => event.type === "async_tool_completed"));
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    rmSync(directory, { recursive: true, force: true });
  }
});


test("idle registered tool callbacks let a Node host exit", () => {
  const result = spawnSync(process.execPath, ["-e", `
    const { Agent } = require(process.argv[1]);
    const agent = new Agent({ name: "idle", model: "anthropic/claude-sonnet-4-6", systemPrompt: "" });
    agent.registerToolWithHandler("probe", "fixture", {}, () => "ok");
    agent.registerAsyncToolWithHandler("poll", "fixture", {}, () => ({ job_id: "test" }),
      () => ({ status: "completed", result: "ok" }));
  `, require.resolve("../index.js")], { timeout: 3_000, encoding: "utf8" });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
});
