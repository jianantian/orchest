import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Agent, RunError, type Run, type SessionOptions } from "orchest-sdk";

function makeAgent(): Agent {
  const agent = new Agent({
    name: "async-example",
    model: "anthropic/claude-sonnet-4-6",
    systemPrompt: "Answer briefly.",
    apiUrl: process.env.ORCHEST_EXAMPLE_API_URL,
    apiKeyEnv: "ANTHROPIC_API_KEY",
  });
  agent.registerToolWithHandler(
    "probe", "Return a small status message for the user.", { type: "object" },
    () => "The example is running.", { approval: "always" },
  );
  return agent;
}

async function consume(run: Run): Promise<void> {
  try {
    try {
      await run.wait();
    } catch (error) {
      if (!(error instanceof RunError) || error.code !== "run_events_pending") {
        throw error;
      }
    }

    for await (const event of run) {
      if (event.type === "approval_requested") {
        const tool = event.tool_call;
        const approved = typeof tool === "object" && tool !== null && "name" in tool && tool.name === "probe";
        await run.respond(event.request_id, { decision: approved ? "approve" : "deny" });
      }
      console.log(event.type);
    }
    const result = await run.wait();
    if (result.type !== "run_completed") {
      throw new Error(JSON.stringify(result));
    }
    console.log(result.output);
  } finally {
    await run.close();
  }
}

async function main(): Promise<void> {
  const directory = await mkdtemp(join(tmpdir(), "orchest-example-"));
  try {
    const session: SessionOptions = {
      sqlitePath: join(directory, "example.sqlite"),
      sessionId: "async-example",
    };
    await consume(await makeAgent().startRun("Call probe and summarize its result.", undefined, session));
    await consume(await makeAgent().resumeSession(session, "Summarize the last result."));
  } finally {
    await rm(directory, { recursive: true, force: true });
  }

  // Breaking out of the iterator closes the receiver and cancels unfinished work.
  for await (const event of await makeAgent().startRun("Explain backpressure.")) {
    console.log(event.type);
    break;
  }
}

void main();
