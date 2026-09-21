const assert = require("node:assert/strict");
const http = require("node:http");
const test = require("node:test");

const sdk = require("../index.js");

const questions = {
  urgent: { type: "boolean", instructions: { question: "Urgent?" } },
  team: { type: "choice", instructions: "Which team?", criteria: { billing: null, tech: { about: "bugs" } } },
  severity: { type: "score", instructions: ["How severe?"], criteria: ["low", { label: "medium" }, "high"] },
};

async function withServer(status, headers, response, run) {
  let captured;
  const server = http.createServer((request, reply) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      captured = {
        path: request.url,
        authorization: request.headers.authorization,
        body: JSON.parse(Buffer.concat(chunks).toString("utf8")),
      };
      reply.writeHead(status, { "content-type": "application/json", ...headers });
      reply.end(JSON.stringify(response));
    });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const address = server.address();
    const result = await run(`http://127.0.0.1:${address.port}/api/alpha/decisions`);
    return { result, captured };
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
}

test("decide uses the real addon and preserves the public contract", async () => {
  assert.equal(typeof sdk.decide, "function");
  const response = {
    id: "decision-1",
    model: "~typesafe/jev-latest",
    provider: "openrouter",
    answers: {
      urgent: { type: "noul", noul: 0.95 },
      team: { type: "choice", choice: "billing" },
      severity: { type: "score", score: 1.05 },
    },
    usage: { input_tokens: 12, output_tokens: 5, cost: 0.0003 },
  };
  const { result, captured } = await withServer(200, {}, response, (apiUrl) => sdk.decide({
    model: "openrouter/~typesafe/jev-latest",
    state: { message: "help", nested: [1, { ok: true }] },
    questions,
    apiKey: "secret",
    apiUrl,
    timeoutMs: 2000,
  }));
  assert.equal(captured.path, "/api/alpha/decisions");
  assert.equal(captured.authorization, "Bearer secret");
  assert.equal(captured.body.questions.urgent.type, "noul");
  assert.deepEqual(captured.body.state.nested[1], { ok: true });
  assert.deepEqual(result.answers.urgent, { type: "boolean", probability: 0.95 });
  assert.equal(result.answers.severity.score, 1.05);
  assert.equal("confidence" in result.answers.team, false);
  assert.equal(result.usage.cost_usd, 0.0003);
});

test("decide normalizes structured HTTP errors", async () => {
  await withServer(429, { "retry-after": "7" }, { error: { message: "slow down" } }, async (apiUrl) => {
    await assert.rejects(
      sdk.decide({ model: "openrouter/~typesafe/jev-latest", state: "case", questions: { q: questions.urgent }, apiKey: "secret", apiUrl }),
      (error) => error instanceof sdk.ProviderError && error.status === 429 && error.retryAfterSecs === 7 && error.provider === "openrouter",
    );
  });
});

test("decide rejects invalid requests", async () => {
  await assert.rejects(
    sdk.decide({ model: "openrouter/~typesafe/jev-latest", state: {}, questions: {}, apiKey: "secret" }),
    (error) => error instanceof sdk.ProviderError && error.code === "invalid_request",
  );
  await assert.rejects(
    sdk.decide({ model: "openrouter/~typesafe/jev-latest", state: {}, questions: { q: { type: "unknown", instructions: "What?" } }, apiKey: "secret" }),
    (error) => error instanceof sdk.ProviderError && error.code === "invalid_request",
  );
});
