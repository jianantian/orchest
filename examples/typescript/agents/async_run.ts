import { Agent, RunError } from "orchest-sdk";

async function main(): Promise<void> {
  const agent = new Agent({
    name: "async-example",
    model: "anthropic/claude-sonnet-4-6",
    systemPrompt: "Answer briefly.",
  });

  const run = await agent.startRun("Why does backpressure matter?");
  try {
    try {
      await run.wait();
    } catch (error) {
      if (!(error instanceof RunError) || error.code !== "run_events_pending") {
        throw error;
      }
    }
    for await (const event of run) {
      console.log(event);
    }
    console.log(await run.wait());
  } finally {
    await run.close();
  }

  // Breaking out of the iterator closes the receiver and cancels an unfinished run.
  for await (const event of await agent.startRun("Explain again with an example.")) {
    console.log(event);
    break;
  }
}

void main();
