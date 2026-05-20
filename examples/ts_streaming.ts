/**
 * Streaming example: process events from an agent run.
 *
 * Usage: npx ts-node examples/ts_streaming.ts
 * (requires building the native addon first with @napi-rs/cli)
 */

console.log("TypeScript SDK Streaming Example");
console.log("================================\n");

// This example shows how to process streaming events.
// The v0.1 SDK returns events as an array (runSync).
// AsyncIterator support is planned for v0.2.

/*
import { Agent, RuntimeEvent } from '@orchest/agent-runtime';

const agent = new Agent({
  model: "claude-sonnet-4-20250514",
  systemPrompt: "You are a helpful assistant.",
  budget: { maxToolCalls: 5, maxCostUsd: 0.10 },
});

const events = agent.runSync("Tell me a short joke") as RuntimeEvent[];

for (const event of events) {
  switch (event.type) {
    case "ModelStreamChunk":
      // In streaming mode, print text deltas as they arrive
      const delta = event.delta as Record<string, unknown>;
      if ("Text" in delta) {
        process.stdout.write(String(delta.Text));
      }
      break;

    case "ToolCallStarted":
      console.log(`\n[Tool] ${event.tool} called`);
      break;

    case "ToolCallCompleted":
      console.log(`[Tool] ${event.tool} → ${JSON.stringify(event.output)}`);
      break;

    case "RunCompleted":
      console.log(`\n[Done] Run completed`);
      break;

    case "RunFailed":
      console.error(`\n[Error] ${event.error}`);
      break;

    case "BudgetWarning":
      console.warn("[Budget] Approaching limit");
      break;
  }
}
*/

console.log("Build the native addon to run this example.");
console.log("See crates/agent-runtime-node/README.md for instructions.");
